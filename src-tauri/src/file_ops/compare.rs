//! Comparing two files, each on this computer or a server, for the side-by-side view. Text is
//! compared line by line, and lines that changed word by word; other files only byte for byte.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices_deadline, Algorithm, DiffOp};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use super::{Channel, Location};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::session::{Session, SessionManager};

/// Larger files are compared byte for byte only.
const MAX_TEXT_BYTES: u64 = 8 * 1024 * 1024;
const MAX_LINES: usize = 200_000;
/// A NUL byte this early means the file is not text.
const BINARY_SNIFF_BYTES: usize = 8000;
const LINE_DIFF_TIME: Duration = Duration::from_secs(2);
/// For all changed lines together; lines past it are marked as changed as a whole.
const WORD_DIFF_TIME: Duration = Duration::from_millis(500);
const MAX_WORD_DIFF_CHARS: usize = 4000;
const BLOCK_PARTS: u64 = 4;
const BLOCK_PART_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub location: Location,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRequest {
    /// Names this comparison for cancelling.
    pub operation_id: String,
    pub left: FileRef,
    pub right: FileRef,
    /// Lines that differ only in spaces and tabs count as equal.
    #[serde(default)]
    pub ignore_whitespace: bool,
    #[serde(default)]
    pub ignore_case: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub left_size: u64,
    pub right_size: u64,
    /// Byte for byte the same.
    pub identical: bool,
    pub content: Content,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Content {
    Text {
        left_lines: Vec<String>,
        right_lines: Vec<String>,
        rows: Vec<Row>,
        added: u32,
        removed: u32,
        changed: u32,
        left_line_ending: LineEnding,
        right_line_ending: LineEnding,
    },
    /// Not text, or too large to show.
    Binary { too_large: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RowKind {
    Equal,
    Changed,
    Removed,
    Added,
}

/// One line of the side-by-side view. Changes are `[start, end)` in UTF-16 code units, as
/// the view's strings count them; a changed row without any is different throughout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub kind: RowKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub left_changes: Vec<[u32; 2]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub right_changes: Vec<[u32; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineEnding {
    None,
    Lf,
    Crlf,
    Mixed,
}

#[derive(Debug, Clone, Copy, Default)]
struct Options {
    ignore_whitespace: bool,
    ignore_case: bool,
}

pub async fn compare(
    sessions: &SessionManager,
    request: &CompareRequest,
    cancel: &CancellationToken,
) -> AppResult<Comparison> {
    let left = Side::locate(sessions, &request.left).await?;
    let right = Side::locate(sessions, &request.right).await?;
    let options = Options {
        ignore_whitespace: request.ignore_whitespace,
        ignore_case: request.ignore_case,
    };
    let work = async {
        let (left_size, right_size) = (left.size().await?, right.size().await?);
        if left_size > MAX_TEXT_BYTES || right_size > MAX_TEXT_BYTES {
            let identical = left_size == right_size && same_bytes(&left, &right, left_size).await?;
            return Ok(Comparison {
                left_size,
                right_size,
                identical,
                content: Content::Binary { too_large: true },
            });
        }
        let (left_bytes, right_bytes) = tokio::try_join!(left.read_all(), right.read_all())?;
        let identical = left_bytes == right_bytes;
        let content =
            tokio::task::spawn_blocking(move || content(&left_bytes, &right_bytes, options))
                .await?;
        Ok(Comparison {
            left_size,
            right_size,
            identical,
            content,
        })
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(AppError::cancelled()),
        compared = work => compared,
    }
}

enum Side {
    Local(PathBuf),
    Remote { session: Arc<Session>, path: String },
}

impl Side {
    async fn locate(sessions: &SessionManager, file: &FileRef) -> AppResult<Self> {
        Ok(match &file.location {
            Location::Local => Self::Local(PathBuf::from(&file.path)),
            Location::Remote { session_id } => {
                let session = sessions.get(session_id).await?;
                let path = session.fs.resolve(&file.path);
                Self::Remote { session, path }
            }
        })
    }

    fn path(&self) -> String {
        match self {
            Self::Local(path) => path.to_string_lossy().into_owned(),
            Self::Remote { path, .. } => path.clone(),
        }
    }

    async fn size(&self) -> AppResult<u64> {
        let not_a_file =
            || AppError::invalid(format!("{} is not a file", self.path())).with_path(self.path());
        match self {
            Self::Local(path) => {
                let metadata = tokio::fs::metadata(path)
                    .await
                    .map_err(|error| AppError::from(error).with_path(self.path()))?;
                if metadata.is_file() {
                    Ok(metadata.len())
                } else {
                    Err(not_a_file())
                }
            }
            Self::Remote { session, path } => match session.fs.stat(path).await? {
                Some(stat) if !stat.is_dir => Ok(stat.size),
                Some(_) => Err(not_a_file()),
                None => Err(
                    AppError::new(ErrorKind::NotFound, "The file no longer exists")
                        .with_path(path.clone()),
                ),
            },
        }
    }

    async fn read_all(&self) -> AppResult<Vec<u8>> {
        match self {
            Self::Local(path) => {
                let path = path.clone();
                let display = self.path();
                tokio::task::spawn_blocking(move || std::fs::read(path))
                    .await?
                    .map_err(|error| AppError::from(error).with_path(display))
            }
            Self::Remote { session, path } => {
                Channel::open(session)
                    .await
                    .read_to_end(path, MAX_TEXT_BYTES)
                    .await
            }
        }
    }

    async fn reader(&self) -> AppResult<Reader<'_>> {
        Ok(match self {
            Self::Local(path) => Reader::Local(
                tokio::fs::File::open(path)
                    .await
                    .map_err(|error| AppError::from(error).with_path(self.path()))?,
            ),
            Self::Remote { session, path } => {
                let fs = Channel::open(session).await;
                let handle = fs.open_for_read(path).await?;
                Reader::Remote { fs, handle }
            }
        })
    }
}

enum Reader<'a> {
    Local(tokio::fs::File),
    Remote { fs: Channel<'a>, handle: String },
}

impl Reader<'_> {
    /// The next block of the file; shorter only at its end.
    async fn next_block(&mut self, offset: u64) -> AppResult<Vec<u8>> {
        match self {
            Self::Local(file) => {
                let mut block = Vec::with_capacity((BLOCK_PARTS * BLOCK_PART_BYTES) as usize);
                file.take(BLOCK_PARTS * BLOCK_PART_BYTES)
                    .read_to_end(&mut block)
                    .await?;
                Ok(block)
            }
            Self::Remote { fs, handle } => {
                let parts = (0..BLOCK_PARTS).map(|part| {
                    fs.read_handle_range(
                        handle,
                        offset + part * BLOCK_PART_BYTES,
                        BLOCK_PART_BYTES as u32,
                    )
                });
                let parts = try_join_all(parts).await?;
                let mut block = Vec::with_capacity((BLOCK_PARTS * BLOCK_PART_BYTES) as usize);
                for part in parts {
                    let short = (part.len() as u64) < BLOCK_PART_BYTES;
                    block.extend_from_slice(&part);
                    if short {
                        break;
                    }
                }
                Ok(block)
            }
        }
    }

    async fn close(self) {
        if let Self::Remote { fs, handle } = self {
            let _ = fs.close_handle(handle).await;
        }
    }
}

/// Reads both files side by side, stopping at the first difference.
async fn same_bytes(left: &Side, right: &Side, size: u64) -> AppResult<bool> {
    let mut left_reader = left.reader().await?;
    let mut right_reader = match right.reader().await {
        Ok(reader) => reader,
        Err(error) => {
            left_reader.close().await;
            return Err(error);
        }
    };
    let mut offset = 0;
    let result = loop {
        let blocks = tokio::try_join!(
            left_reader.next_block(offset),
            right_reader.next_block(offset)
        );
        match blocks {
            Ok((left_block, right_block)) if left_block != right_block => break Ok(false),
            Ok((left_block, _)) if left_block.is_empty() => break Ok(offset == size),
            Ok((left_block, _)) => offset += left_block.len() as u64,
            Err(error) => break Err(error),
        }
    };
    left_reader.close().await;
    right_reader.close().await;
    result
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0)
}

fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// Lines without their endings, and which ending the text uses.
fn split_lines(text: &str) -> (Vec<&str>, LineEnding) {
    let (mut unix, mut windows) = (0, 0);
    let lines = text
        .split_inclusive('\n')
        .map(|piece| {
            if let Some(line) = piece.strip_suffix("\r\n") {
                windows += 1;
                line
            } else if let Some(line) = piece.strip_suffix('\n') {
                unix += 1;
                line
            } else {
                piece
            }
        })
        .collect();
    let ending = match (unix, windows) {
        (0, 0) => LineEnding::None,
        (_, 0) => LineEnding::Lf,
        (0, _) => LineEnding::Crlf,
        _ => LineEnding::Mixed,
    };
    (lines, ending)
}

fn line_key(line: &str, options: Options) -> String {
    let line: String = if options.ignore_whitespace {
        line.chars()
            .filter(|character| !character.is_whitespace())
            .collect()
    } else {
        line.to_string()
    };
    if options.ignore_case {
        line.to_lowercase()
    } else {
        line
    }
}

fn content(left_bytes: &[u8], right_bytes: &[u8], options: Options) -> Content {
    if is_binary(left_bytes) || is_binary(right_bytes) {
        return Content::Binary { too_large: false };
    }
    let (left_text, right_text) = (decode(left_bytes), decode(right_bytes));
    let (left_lines, left_line_ending) = split_lines(&left_text);
    let (right_lines, right_line_ending) = split_lines(&right_text);
    if left_lines.len() > MAX_LINES || right_lines.len() > MAX_LINES {
        return Content::Binary { too_large: true };
    }
    let rows = diff_rows(&left_lines, &right_lines, options);
    let count = |kind: RowKind| rows.iter().filter(|row| row.kind == kind).count() as u32;
    Content::Text {
        added: count(RowKind::Added),
        removed: count(RowKind::Removed),
        changed: count(RowKind::Changed),
        rows,
        left_lines: left_lines.into_iter().map(str::to_string).collect(),
        right_lines: right_lines.into_iter().map(str::to_string).collect(),
        left_line_ending,
        right_line_ending,
    }
}

fn diff_rows(left: &[&str], right: &[&str], options: Options) -> Vec<Row> {
    let left_keys: Vec<String> = left.iter().map(|line| line_key(line, options)).collect();
    let right_keys: Vec<String> = right.iter().map(|line| line_key(line, options)).collect();
    let deadline = Instant::now() + LINE_DIFF_TIME;
    let operations =
        capture_diff_slices_deadline(Algorithm::Myers, &left_keys, &right_keys, Some(deadline));
    let word_deadline = Instant::now() + WORD_DIFF_TIME;
    let mut rows = Vec::with_capacity(left.len().max(right.len()));
    let line = |index: usize| Some(index as u32);
    let one_sided = |kind: RowKind, left: Option<u32>, right: Option<u32>| Row {
        kind,
        left,
        right,
        left_changes: Vec::new(),
        right_changes: Vec::new(),
    };
    for operation in operations {
        match operation {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => rows.extend((0..len).map(|offset| {
                one_sided(
                    RowKind::Equal,
                    line(old_index + offset),
                    line(new_index + offset),
                )
            })),
            DiffOp::Delete {
                old_index, old_len, ..
            } => rows.extend(
                (old_index..old_index + old_len)
                    .map(|index| one_sided(RowKind::Removed, line(index), None)),
            ),
            DiffOp::Insert {
                new_index, new_len, ..
            } => rows.extend(
                (new_index..new_index + new_len)
                    .map(|index| one_sided(RowKind::Added, None, line(index))),
            ),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                let paired = old_len.min(new_len);
                for offset in 0..paired {
                    let (left_index, right_index) = (old_index + offset, new_index + offset);
                    let (left_changes, right_changes) = if Instant::now() < word_deadline {
                        word_changes(left[left_index], right[right_index], word_deadline)
                    } else {
                        (Vec::new(), Vec::new())
                    };
                    rows.push(Row {
                        kind: RowKind::Changed,
                        left: line(left_index),
                        right: line(right_index),
                        left_changes,
                        right_changes,
                    });
                }
                rows.extend(
                    (old_index + paired..old_index + old_len)
                        .map(|index| one_sided(RowKind::Removed, line(index), None)),
                );
                rows.extend(
                    (new_index + paired..new_index + new_len)
                        .map(|index| one_sided(RowKind::Added, None, line(index))),
                );
            }
        }
    }
    rows
}

/// Words, runs of spaces, and single other characters.
fn tokens(line: &str) -> Vec<&str> {
    let class = |character: char| {
        if character.is_alphanumeric() || character == '_' {
            1
        } else if character.is_whitespace() {
            2
        } else {
            0
        }
    };
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut previous = None;
    for (index, character) in line.char_indices() {
        let current = class(character);
        if index > start && (current == 0 || previous != Some(current)) {
            tokens.push(&line[start..index]);
            start = index;
        }
        previous = Some(current);
    }
    if start < line.len() {
        tokens.push(&line[start..]);
    }
    tokens
}

/// The parts of two lines that differ, in UTF-16 units. Empty when the lines have nothing in
/// common worth showing, so the view marks them whole.
fn word_changes(left: &str, right: &str, deadline: Instant) -> (Vec<[u32; 2]>, Vec<[u32; 2]>) {
    if left.len() > MAX_WORD_DIFF_CHARS || right.len() > MAX_WORD_DIFF_CHARS {
        return (Vec::new(), Vec::new());
    }
    let (left_tokens, right_tokens) = (tokens(left), tokens(right));
    let operations = capture_diff_slices_deadline(
        Algorithm::Myers,
        &left_tokens,
        &right_tokens,
        Some(deadline),
    );
    let common: usize = operations
        .iter()
        .map(|operation| match operation {
            DiffOp::Equal { old_index, len, .. } => left_tokens[*old_index..*old_index + len]
                .iter()
                .map(|token| token.trim().len())
                .sum(),
            _ => 0,
        })
        .sum();
    if common == 0 {
        return (Vec::new(), Vec::new());
    }
    let left_offsets = utf16_offsets(&left_tokens);
    let right_offsets = utf16_offsets(&right_tokens);
    let mut left_changes = Vec::new();
    let mut right_changes = Vec::new();
    for operation in operations {
        match operation {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete {
                old_index, old_len, ..
            } => push_range(&mut left_changes, &left_offsets, old_index, old_len),
            DiffOp::Insert {
                new_index, new_len, ..
            } => push_range(&mut right_changes, &right_offsets, new_index, new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                push_range(&mut left_changes, &left_offsets, old_index, old_len);
                push_range(&mut right_changes, &right_offsets, new_index, new_len);
            }
        }
    }
    (left_changes, right_changes)
}

/// Where each token starts in UTF-16 units, plus where the last one ends.
fn utf16_offsets(tokens: &[&str]) -> Vec<u32> {
    let mut offsets = Vec::with_capacity(tokens.len() + 1);
    let mut position = 0u32;
    offsets.push(0);
    for token in tokens {
        position += token.encode_utf16().count() as u32;
        offsets.push(position);
    }
    offsets
}

/// Adds tokens `[first, first + count)` as a range, joining it to the previous one if they
/// touch.
fn push_range(ranges: &mut Vec<[u32; 2]>, offsets: &[u32], first: usize, count: usize) {
    let (start, end) = (offsets[first], offsets[first + count]);
    if start == end {
        return;
    }
    match ranges.last_mut() {
        Some(last) if last[1] == start => last[1] = end,
        _ => ranges.push([start, end]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(left: &str, right: &str, options: Options) -> Vec<Row> {
        let (left_lines, _) = split_lines(left);
        let (right_lines, _) = split_lines(right);
        diff_rows(&left_lines, &right_lines, options)
    }

    #[test]
    fn splits_lines_and_reports_endings() {
        assert_eq!(split_lines("a\nb\n"), (vec!["a", "b"], LineEnding::Lf));
        assert_eq!(split_lines("a\r\nb"), (vec!["a", "b"], LineEnding::Crlf));
        assert_eq!(split_lines("a\r\nb\n"), (vec!["a", "b"], LineEnding::Mixed));
        assert_eq!(split_lines("one"), (vec!["one"], LineEnding::None));
        assert_eq!(split_lines(""), (vec![], LineEnding::None));
    }

    #[test]
    fn aligns_changes_side_by_side() {
        let rows = rows(
            "server {\n  listen 80;\n  root /srv/old;\n}\n",
            "server {\n  listen 443 ssl;\n  root /srv/old;\n  index index.html;\n}\n",
            Options::default(),
        );
        let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(
            kinds,
            [
                RowKind::Equal,
                RowKind::Changed,
                RowKind::Equal,
                RowKind::Added,
                RowKind::Equal
            ]
        );
        let changed = &rows[1];
        assert_eq!((changed.left, changed.right), (Some(1), Some(1)));
        // "  listen 80;" against "  listen 443 ssl;": only the port and what follows differ.
        assert_eq!(changed.left_changes, [[9, 11]]);
        assert_eq!(changed.right_changes, [[9, 16]]);
        assert_eq!((rows[3].left, rows[3].right), (None, Some(3)));
    }

    #[test]
    fn whitespace_and_case_can_be_ignored() {
        let left = "Key = Value\n";
        let right = "key=value\n";
        assert_eq!(
            rows(left, right, Options::default())[0].kind,
            RowKind::Changed
        );
        let relaxed = Options {
            ignore_whitespace: true,
            ignore_case: true,
        };
        assert_eq!(rows(left, right, relaxed)[0].kind, RowKind::Equal);
    }

    #[test]
    fn ranges_count_utf16_units() {
        let (left, right) =
            word_changes("naïve 😀 a", "naïve 😀 b", Instant::now() + WORD_DIFF_TIME);
        assert_eq!(left, [[9, 10]]);
        assert_eq!(right, [[9, 10]]);
    }

    #[test]
    fn unrelated_lines_are_marked_whole() {
        let (left, right) = word_changes("alpha", "omega", Instant::now() + WORD_DIFF_TIME);
        assert!(left.is_empty() && right.is_empty());
    }

    #[test]
    fn detects_binary_and_strips_bom() {
        assert!(is_binary(b"PK\x03\x04\0\0"));
        assert!(!is_binary(b"plain text"));
        assert_eq!(decode(b"\xEF\xBB\xBFhello"), "hello");
        assert!(matches!(
            content(b"a\0b", b"a\0b", Options::default()),
            Content::Binary { too_large: false }
        ));
    }
}
