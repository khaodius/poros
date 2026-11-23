//! Searching with the server's own `find`, which looks through a tree far faster than listing
//! every folder over SFTP.
//!
//! Only GNU find is used: its `-printf` reports everything a listing shows, in records that
//! end in NUL, the one byte no file name can hold. Servers whose `find` lacks it (BusyBox,
//! BSD, macOS) are searched by listing folders instead.
//!
//! `find` only narrows down what is sent. Every name is matched again here, so a pattern
//! `find` cannot express exactly is still matched exactly.

use std::time::Duration;

use futures::stream::{self, StreamExt};
use russh::{ChannelMsg, Sig};
use tokio_util::sync::CancellationToken;

use super::criteria::{Criteria, KindFilter};
use super::Output;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::file_ops::Failure;
use crate::model::{EntryKind, FileEntry, LinkTarget};
use crate::protocol;
use crate::rsync::{run_command, shell_quote};
use crate::session::Session;
use crate::sftp::RemoteFs;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const PROBE_OUTPUT_LIMIT: usize = 4 * 1024;
const ERROR_OUTPUT_LIMIT: usize = 256 * 1024;
/// No path is this long; more bytes without a NUL are not records.
const RECORD_LIMIT: usize = 512 * 1024;
const PARALLEL_LOOKUPS: usize = 8;
/// One record per entry: `E`, the entry's own type, the type a link leads to, size, time of
/// last change, mode, owner, group, path.
const ENTRY_FIELDS: usize = 8;
const ENTRY_RECORD: &str = r"-printf 'E\0%y\0%Y\0%s\0%T@\0%m\0%u\0%g\0%p\0'";
/// The same when links are followed: `%y` then tells what the link leads to, so whether the
/// entry is a link is asked separately.
const FOLLOWED_ENTRY_RECORD: &str =
    r"\( -xtype l -printf 'E\0l\0' -o -printf 'E\0%y\0' \) -printf '%y\0%s\0%T@\0%m\0%u\0%g\0%p\0'";
/// One record per folder, with its depth, so the search can show where it is. Only those
/// find goes into are counted, as listing folders counts them: not the last level.
const FOLDER_RECORD: &str = r"\( -type d -printf 'D\0%d\0%p\0' -o -true \)";
/// The same, leaving out the hidden folders find does not go into.
const VISIBLE_FOLDER_RECORD: &str = r"\( -type d ! -name '.*' -printf 'D\0%d\0%p\0' -o -true \)";
const SKIP_HIDDEN: &str = r"\( ! -name '.*' -o -prune \)";

/// Why the server's `find` gave no answer.
pub(super) enum Stop {
    /// It cannot be used here, and nothing was reported yet: list folders instead.
    Unusable,
    Failed(AppError),
}

impl From<AppError> for Stop {
    fn from(error: AppError) -> Self {
        Self::Failed(error)
    }
}

/// Whether the server runs commands and has a `find` with everything [`command`] uses.
pub(super) async fn is_usable(session: &Session) -> bool {
    let Ok(channel) = session.open_command_channel().await else {
        return false;
    };
    let probe = tokio::time::timeout(
        PROBE_TIMEOUT,
        run_command(channel, &probe_command(), PROBE_OUTPUT_LIMIT),
    )
    .await;
    matches!(probe, Ok(Ok(output)) if output.exit_status == Some(0) && probe_passed(&output.output))
}

/// Has `find` describe `/` with every test and format the search uses.
fn probe_command() -> String {
    in_shell(&probe_script())
}

/// The probe holds no name, nor a backslash before a quote or another backslash, so every
/// login shell reads its quoting as `sh` does.
fn probe_script() -> String {
    format!(
        r"find / -maxdepth 0 -mindepth 0 \( -iname x -o -newermt @0 -o -size +0c -o -xtype l -o -true \) {ENTRY_RECORD}"
    )
}

fn probe_passed(output: &[u8]) -> bool {
    let mut parser = Parser::new("/");
    matches!(
        parser.push(output).as_deref(),
        Ok([Record::Entry(entry)]) if entry.path == "/" && entry.kind == EntryKind::Dir
    ) && parser.is_empty()
}

/// What the account's login shell is asked to run for a search: `sh`, reading the search from
/// standard input. Nothing of the search is in it, so a login shell that quotes otherwise than
/// `sh` (fish reads `\'` inside single quotes) never parses a name. Only `sh` does, quoted for
/// `sh` alone.
pub(super) const SEARCH_SHELL: &str = "sh -s";

/// The C locale, so that names are bytes and messages are in English.
const C_LOCALE: &str = "LC_ALL=C; export LC_ALL;";

/// Runs `script` in `sh` whatever the account's own shell is. Only for scripts that hold no
/// name or pattern: the script is quoted once more for the login shell.
fn in_shell(script: &str) -> String {
    format!("sh -c {}", shell_quote(&format!("{C_LOCALE} {script}")))
}

/// The script [`SEARCH_SHELL`] reads to search below `root`, an absolute path. The first
/// record describes `root` itself, to tell whether commands see the same folder as SFTP does.
pub(super) fn command(root: &str, criteria: &Criteria) -> String {
    let root = shell_quote(root);
    let mut search = vec!["find".to_string()];
    if criteria.follow_links {
        // Never loops: find notices a link that leads back up and does not enter it.
        search.push("-L".to_string());
    }
    search.push(root.clone());
    search.push(format!("-mindepth 1 -maxdepth {}", criteria.depth_limit()));
    if criteria.enter_hidden {
        search.push(FOLDER_RECORD.to_string());
    } else {
        search.push(VISIBLE_FOLDER_RECORD.to_string());
        search.push(SKIP_HIDDEN.to_string());
    }
    if let Some(pattern) = criteria.name.find_pattern() {
        let test = if criteria.name.case_sensitive() {
            "-name"
        } else {
            "-iname"
        };
        search.push(format!("{test} {}", shell_quote(pattern)));
    }
    match criteria.kind {
        KindFilter::Any => {}
        // A link may lead to either, which is decided here.
        KindFilter::Files => search.push("! -type d".to_string()),
        KindFilter::Folders => search.push(r"\( -type d -o -type l \)".to_string()),
    }
    let limits = size_and_date_tests(criteria);
    if !limits.is_empty() {
        // A listing shows the size and date of what a link leads to, which find does not
        // look at unless it follows links.
        search.push(format!(r"\( -type l -o \( {} \) \)", limits.join(" ")));
    }
    search.push(
        if criteria.follow_links {
            FOLLOWED_ENTRY_RECORD
        } else {
            ENTRY_RECORD
        }
        .to_string(),
    );
    format!(
        "{C_LOCALE} find {root} -maxdepth 0 {ENTRY_RECORD}; {}\n",
        search.join(" ")
    )
}

/// Tests that let through at least everything the size and date limits allow; find compares
/// times with fractions of a second, so the limits are widened by a second.
fn size_and_date_tests(criteria: &Criteria) -> Vec<String> {
    let mut tests = Vec::new();
    if let Some(least) = criteria.min_size.filter(|least| *least > 0) {
        tests.push(format!("-size +{}c", least - 1));
    }
    if let Some(most) = criteria.max_size.and_then(|most| most.checked_add(1)) {
        tests.push(format!("-size -{most}c"));
    }
    if let Some(after) = criteria.modified_after.filter(|after| *after > 0) {
        tests.push(format!("-newermt @{}", after - 1));
    }
    if let Some(before) = criteria
        .modified_before
        .filter(|before| *before >= 0)
        .and_then(|before| before.checked_add(1))
    {
        tests.push(format!("! -newermt @{before}"));
    }
    tests
}

#[derive(Debug, Clone)]
pub(super) enum Record {
    /// A folder `find` is looking at, and how far below the start folder it is.
    Folder {
        path: String,
        depth: u32,
    },
    Entry(FileEntry),
}

/// The output was not what [`command`] asks `find` to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Malformed;

/// Splits the output into records as it arrives, in pieces of any size.
pub(super) struct Parser {
    root: String,
    partial: Vec<u8>,
}

impl Parser {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
            partial: Vec::new(),
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Record>, Malformed> {
        self.partial.extend_from_slice(data);
        let mut records = Vec::new();
        let mut consumed = 0;
        while let Some((record, length)) = self.next_record(&self.partial[consumed..])? {
            records.push(record);
            consumed += length;
        }
        self.partial.drain(..consumed);
        if self.partial.len() > RECORD_LIMIT {
            return Err(Malformed);
        }
        Ok(records)
    }

    /// Whether the output ended at the end of a record.
    pub fn is_empty(&self) -> bool {
        self.partial.is_empty()
    }

    /// The first record in `bytes` and its length, or `None` while it is still arriving.
    fn next_record(&self, bytes: &[u8]) -> Result<Option<(Record, usize)>, Malformed> {
        let mut pieces = bytes.split(|byte| *byte == 0).peekable();
        // A piece is a whole field only when another follows it: the last is what came
        // after the last NUL.
        let Some(tag) = pieces.next().filter(|_| pieces.peek().is_some()) else {
            return Ok(None);
        };
        let wanted = match tag {
            b"D" => 2,
            b"E" => ENTRY_FIELDS,
            _ => return Err(Malformed),
        };
        let mut fields: Vec<&[u8]> = Vec::with_capacity(wanted);
        for _ in 0..wanted {
            match pieces.next().filter(|_| pieces.peek().is_some()) {
                Some(field) => fields.push(field),
                None => return Ok(None),
            }
        }
        let length = tag.len() + 1 + fields.iter().map(|field| field.len() + 1).sum::<usize>();
        let record = match tag {
            b"D" => Record::Folder {
                depth: text(fields[0])?.parse().map_err(|_| Malformed)?,
                path: self.path(fields[1])?,
            },
            _ => Record::Entry(self.entry(&fields)?),
        };
        Ok(Some((record, length)))
    }

    /// A path is taken only when it is the start folder or lies below it: nothing the server
    /// sends can point anywhere else.
    fn path(&self, bytes: &[u8]) -> Result<String, Malformed> {
        let path = String::from_utf8_lossy(bytes).into_owned();
        if path == self.root {
            return Ok(path);
        }
        let below = match self.root.as_str() {
            "/" => path.strip_prefix('/'),
            root => path
                .strip_prefix(root)
                .and_then(|rest| rest.strip_prefix('/')),
        };
        let sound = below.is_some_and(|below| {
            below
                .split('/')
                .all(|name| !name.is_empty() && name != "." && name != "..")
        });
        if sound {
            Ok(path)
        } else {
            Err(Malformed)
        }
    }

    fn entry(&self, fields: &[&[u8]]) -> Result<FileEntry, Malformed> {
        let [own, target, size, modified, mode, owner, group, path] = fields else {
            return Err(Malformed);
        };
        let path = self.path(path)?;
        let name = match path.as_str() {
            "/" => "/".to_string(),
            path => crate::remote_path::file_name(path).to_string(),
        };
        let (kind, link_target) = match (type_letter(own)?, type_letter(target)?) {
            (b'l', b'd') => (EntryKind::Symlink, Some(LinkTarget::Dir)),
            // `N` and `?`: nothing there, or no telling; `L`: a loop; `l`: a broken link seen
            // while following links.
            (b'l', b'N' | b'L' | b'?' | b'l') => (EntryKind::Symlink, Some(LinkTarget::Broken)),
            (b'l', _) => (EntryKind::Symlink, Some(LinkTarget::File)),
            (b'd', _) => (EntryKind::Dir, None),
            (b'f', _) => (EntryKind::File, None),
            _ => (EntryKind::Other, None),
        };
        let size: u64 = number(size)?;
        let modified = text(modified)?
            .parse::<f64>()
            .ok()
            .filter(|seconds| seconds.is_finite())
            .ok_or(Malformed)?
            .floor() as i64;
        let mode = u32::from_str_radix(text(mode)?, 8).map_err(|_| Malformed)?;
        Ok(FileEntry {
            hidden: name.starts_with('.'),
            name,
            path,
            kind,
            link_target,
            size: if kind == EntryKind::Dir { 0 } else { size },
            modified: Some(modified),
            permissions: Some(mode & 0o7777),
            owner: owner_name(owner),
            group: owner_name(group),
        })
    }
}

fn type_letter(field: &[u8]) -> Result<u8, Malformed> {
    match field {
        [letter] => Ok(*letter),
        _ => Err(Malformed),
    }
}

fn text(field: &[u8]) -> Result<&str, Malformed> {
    std::str::from_utf8(field).map_err(|_| Malformed)
}

fn number(field: &[u8]) -> Result<u64, Malformed> {
    text(field)?.parse().map_err(|_| Malformed)
}

fn owner_name(field: &[u8]) -> Option<String> {
    Some(String::from_utf8_lossy(field).into_owned()).filter(|name| !name.is_empty())
}

/// The folders `find` said it could not read, from what it wrote to standard error.
pub(super) fn unreadable(error_output: &str, root: &str) -> Vec<Failure> {
    error_output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("find: "))
        // A link that leads back up is skipped by find, which is what is wanted; something
        // deleted during the search is simply no longer there.
        .filter(|problem| {
            !problem.contains("File system loop detected")
                && !problem.ends_with("No such file or directory")
        })
        .map(|problem| match problem.rsplit_once(": ") {
            Some((path, message)) => Failure {
                path: path
                    .trim_matches(|quote| matches!(quote, '\'' | '"' | '\u{2018}' | '\u{2019}'))
                    .to_string(),
                message: message.to_string(),
            },
            None => Failure {
                path: root.to_string(),
                message: problem.to_string(),
            },
        })
        .collect()
}

/// Searches below `folder` and reports what is found to `output`. Returns early, without an
/// error, when the search is cancelled or has found as much as it may.
pub(super) async fn search(
    session: &Session,
    fs: &RemoteFs,
    folder: &str,
    criteria: &Criteria,
    cancel: &CancellationToken,
    output: &mut Output<'_>,
) -> Result<(), Stop> {
    let root = fs.canonicalize(&fs.resolve(folder)).await?;
    let root = match root.trim_end_matches('/') {
        "" => "/".to_string(),
        trimmed => trimmed.to_string(),
    };
    if !root.starts_with('/') || root.contains('\0') {
        return Err(Stop::Unusable);
    }
    let seen_over_sftp = match fs.stat(&root).await? {
        Some(stat) if stat.is_dir => stat,
        Some(_) => return Err(protocol::not_a_folder(&root).into()),
        None => {
            return Err(
                AppError::new(ErrorKind::NotFound, format!("{root} does not exist"))
                    .with_path(root)
                    .into(),
            )
        }
    };

    let mut channel = session
        .open_command_channel()
        .await
        .map_err(|_| Stop::Unusable)?;
    channel
        .exec(true, SEARCH_SHELL)
        .await
        .map_err(|_| Stop::Unusable)?;
    channel
        .data(command(&root, criteria).as_bytes())
        .await
        .map_err(|_| Stop::Unusable)?;
    let _ = channel.eof().await;

    let mut parser = Parser::new(&root);
    let mut error_output: Vec<u8> = Vec::new();
    let mut root_seen = false;
    let mut exit_status = None;
    loop {
        let message = tokio::select! {
            biased;
            _ = cancel.cancelled() => None,
            message = channel.wait() => Some(message),
        };
        let Some(message) = message else {
            end(&mut channel).await;
            return Ok(());
        };
        match message {
            Some(ChannelMsg::Data { data }) => {
                let mut records = match parser.push(&data) {
                    Ok(records) => records.into_iter(),
                    Err(Malformed) => {
                        end(&mut channel).await;
                        return Err(unreadable_output(root_seen));
                    }
                };
                if !root_seen {
                    match records.next() {
                        Some(Record::Entry(entry))
                            if entry.path == root
                                && entry.kind == EntryKind::Dir
                                && (seen_over_sftp.modified.is_none()
                                    || entry.modified == seen_over_sftp.modified) =>
                        {
                            root_seen = true;
                            // Asked for through a link, the folder is searched by its real
                            // path, which is what the results are below.
                            if root != folder {
                                output.started_in(&root);
                            }
                            // Counted as listing folders counts it.
                            output.visit(&root);
                        }
                        // Commands run in another folder than the one SFTP shows.
                        Some(_) => {
                            end(&mut channel).await;
                            return Err(Stop::Unusable);
                        }
                        None => continue,
                    }
                }
                let mut candidates = Vec::new();
                for record in records {
                    match record {
                        Record::Folder { path, depth } if depth < criteria.depth_limit() => {
                            output.visit(&path);
                        }
                        Record::Folder { .. } => {}
                        Record::Entry(entry) if criteria.matches_name_and_kind(&entry) => {
                            candidates.push(entry);
                        }
                        Record::Entry(_) => {}
                    }
                }
                let lookups = with_link_targets(fs, candidates, criteria);
                let found = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => None,
                    found = lookups => Some(found),
                };
                let Some(found) = found else {
                    end(&mut channel).await;
                    return Ok(());
                };
                let found = match found {
                    Ok(found) => found,
                    Err(error) => {
                        end(&mut channel).await;
                        return Err(error.into());
                    }
                };
                for entry in found {
                    if criteria.matches(&entry) {
                        output.offer(entry);
                    }
                }
                if output.is_full() {
                    end(&mut channel).await;
                    return Ok(());
                }
                output.report_if_due();
            }
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                let room = ERROR_OUTPUT_LIMIT.saturating_sub(error_output.len());
                error_output.extend_from_slice(&data[..data.len().min(room)]);
            }
            Some(ChannelMsg::ExitStatus {
                exit_status: status,
            }) => exit_status = Some(status),
            Some(ChannelMsg::Failure) | Some(ChannelMsg::Close) | None => break,
            Some(_) => {}
        }
    }

    if !root_seen {
        return Err(Stop::Unusable);
    }
    if exit_status.is_none() {
        return Err(AppError::new(
            ErrorKind::Disconnected,
            "The connection was lost during the search",
        )
        .into());
    }
    if !parser.is_empty() {
        return Err(unreadable_output(true));
    }
    for failure in unreadable(&String::from_utf8_lossy(&error_output), &root) {
        output.unreadable(failure);
    }
    Ok(())
}

/// Stops the command on the server.
async fn end(channel: &mut russh::Channel<russh::client::Msg>) {
    let _ = channel.signal(Sig::TERM).await;
    let _ = channel.close().await;
}

fn unreadable_output(root_seen: bool) -> Stop {
    if root_seen {
        Stop::Failed(AppError::new(
            ErrorKind::Io,
            "The server's find sent something that could not be read",
        ))
    } else {
        Stop::Unusable
    }
}

/// Gives links to files the size and date of what they lead to, as a listing shows them.
async fn with_link_targets(
    fs: &RemoteFs,
    entries: Vec<FileEntry>,
    criteria: &Criteria,
) -> AppResult<Vec<FileEntry>> {
    let needs_lookup = |entry: &FileEntry| {
        !criteria.follow_links
            && entry.kind == EntryKind::Symlink
            && entry.link_target == Some(LinkTarget::File)
    };
    if !entries.iter().any(needs_lookup) {
        return Ok(entries);
    }
    stream::iter(entries)
        .map(|mut entry| async move {
            if needs_lookup(&entry) {
                match fs.stat(&entry.path).await {
                    Ok(Some(stat)) => {
                        entry.size = stat.size;
                        entry.modified = stat.modified.or(entry.modified);
                    }
                    Ok(None) => entry.link_target = Some(LinkTarget::Broken),
                    Err(error) if error.is_connection_lost() => return Err(error),
                    Err(_) => {}
                }
            }
            Ok(entry)
        })
        .buffered(PARALLEL_LOOKUPS)
        .collect::<Vec<AppResult<FileEntry>>>()
        .await
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::criteria::tests::request;
    use crate::search::{PatternKind, SearchRequest};

    fn criteria(request: SearchRequest) -> Criteria {
        Criteria::from_request(&request).unwrap()
    }

    /// The words a POSIX shell makes of `command`, which has to be quoted throughout.
    fn shell_words(command: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut in_word = false;
        let mut characters = command.chars();
        while let Some(character) = characters.next() {
            match character {
                '\'' => {
                    in_word = true;
                    for quoted in characters.by_ref() {
                        if quoted == '\'' {
                            break;
                        }
                        word.push(quoted);
                    }
                }
                '\\' => {
                    in_word = true;
                    word.push(characters.next().expect("a character after a backslash"));
                }
                ' ' => {
                    if in_word {
                        words.push(std::mem::take(&mut word));
                        in_word = false;
                    }
                }
                other => {
                    in_word = true;
                    word.push(other);
                }
            }
        }
        if in_word {
            words.push(word);
        }
        words
    }

    /// The words of the script's last command, the search itself.
    fn search_words(script: &str) -> Vec<String> {
        assert!(script.starts_with("LC_ALL=C; export LC_ALL; find "));
        let script = script.strip_suffix('\n').expect("a whole line");
        let search = script.rsplit_once("; find ").expect("two find commands").1;
        shell_words(&format!("find {search}"))
    }

    #[test]
    fn the_command_searches_below_the_folder() {
        let words = search_words(&command("/srv/data", &criteria(request("*.log"))));
        assert_eq!(
            &words[..6],
            ["find", "/srv/data", "-mindepth", "1", "-maxdepth", "200"]
        );
        let name = words.iter().position(|word| word == "-iname").unwrap();
        assert_eq!(words[name + 1], "*.log");
        assert!(!words.contains(&"-L".to_string()));
        assert!(words.contains(&"-prune".to_string()));
        assert_eq!(
            words.last().unwrap(),
            r"E\0%y\0%Y\0%s\0%T@\0%m\0%u\0%g\0%p\0"
        );
    }

    #[test]
    fn hostile_names_stay_single_arguments() {
        let hostile_folders = [
            "/srv/it's",
            "/srv/$(reboot)",
            "/srv/`reboot`",
            "/srv/a; rm -rf ~",
            "/srv/two\nlines",
            "/srv/quote\"d",
            "/srv/-delete",
            "/srv/a b\tc",
            "/srv/'; echo pwned; '",
            "/srv/back\\slash",
            "/srv/!history",
        ];
        for folder in hostile_folders {
            let words = search_words(&command(folder, &criteria(request(""))));
            assert_eq!(words[1], folder, "the folder is one argument");
            assert_eq!(words[2], "-mindepth");
        }

        let hostile_patterns = [
            "it's",
            "-delete",
            "$(reboot)",
            "`id`",
            "a; rm -rf /",
            "two\nlines",
            "-exec rm {} ;",
            "' -o -delete '",
            "\"",
            "\\",
            "(",
            "!",
        ];
        for pattern in hostile_patterns {
            // Hidden folders are searched too, so the only -name is the pattern's.
            let criteria = criteria(SearchRequest {
                case_sensitive: true,
                hidden: true,
                ..request(pattern)
            });
            let words = search_words(&command("/srv", &criteria));
            let name = words.iter().position(|word| word == "-name").unwrap();
            assert_eq!(
                words[name + 1],
                criteria.name.find_pattern().unwrap(),
                "the pattern is the one argument after -name"
            );
            for dangerous in ["-delete", "-exec", "-execdir", "-ok", "-fprint", "-fprintf"] {
                assert!(
                    !words.iter().any(|word| word == dangerous),
                    "{pattern:?} must not add {dangerous} to the command: {words:?}"
                );
            }
        }
    }

    /// A login shell that quotes otherwise than `sh` gets nothing of the search to read: fish
    /// takes `\'` inside single quotes as a quote, so a folder named like this one would run a
    /// command were it quoted for `sh` and handed to fish.
    #[test]
    fn a_name_fish_would_misread_never_reaches_the_login_shell() {
        let folder = r"/srv/x\';touch /tmp/pwned;'";
        let pattern = r"y\';touch /tmp/pwned;'";
        let criteria = criteria(SearchRequest {
            case_sensitive: true,
            hidden: true,
            ..request(pattern)
        });
        assert_eq!(SEARCH_SHELL, "sh -s");
        let words = search_words(&command(folder, &criteria));
        assert_eq!(words[1], folder, "the folder is one argument");
        let name = words.iter().position(|word| word == "-name").unwrap();
        assert_eq!(words[name + 1], criteria.name.find_pattern().unwrap());
        assert!(!words.iter().any(|word| word == "touch"));
        // The probe still goes through the login shell, holding nothing fish reads otherwise.
        let probe = probe_script();
        assert!(!probe.contains(r"\'") && !probe.contains(r"\\"), "{probe}");
    }

    #[test]
    fn regular_expressions_are_matched_here() {
        let words = search_words(&command(
            "/srv",
            &criteria(SearchRequest {
                pattern_kind: PatternKind::Regex,
                hidden: true,
                ..request(r"\.(jpe?g|png)$")
            }),
        ));
        assert!(!words.iter().any(|word| word == "-name" || word == "-iname"));
        assert!(!words.iter().any(|word| word.contains("jpe")));
    }

    #[test]
    fn options_become_tests() {
        let words = search_words(&command(
            "/srv",
            &criteria(SearchRequest {
                kind: KindFilter::Files,
                min_size: Some(1024),
                max_size: Some(4096),
                modified_after: Some(1_700_000_000),
                modified_before: Some(1_700_086_400),
                max_depth: Some(3),
                hidden: true,
                follow_links: true,
                ..request("")
            }),
        ));
        let joined = words.join(" ");
        assert!(joined.starts_with("find -L /srv -mindepth 1 -maxdepth 3 "));
        assert!(!joined.contains("-prune"));
        assert!(joined.contains("! -type d"));
        assert!(joined.contains(
            "( -type l -o ( -size +1023c -size -4097c -newermt @1699999999 ! -newermt @1700086401 ) )"
        ));
        assert!(joined.contains("-xtype l"));
    }

    #[test]
    fn limits_that_exclude_nothing_add_no_test() {
        let tests = size_and_date_tests(&criteria(SearchRequest {
            min_size: Some(0),
            max_size: Some(u64::MAX),
            modified_after: Some(0),
            modified_before: Some(i64::MAX),
            ..request("")
        }));
        assert!(tests.is_empty(), "{tests:?}");
    }

    fn record(fields: &[&str]) -> Vec<u8> {
        fields
            .iter()
            .flat_map(|field| field.bytes().chain(std::iter::once(0)))
            .collect()
    }

    fn entries(records: Vec<Record>) -> Vec<FileEntry> {
        records
            .into_iter()
            .filter_map(|record| match record {
                Record::Entry(entry) => Some(entry),
                Record::Folder { .. } => None,
            })
            .collect()
    }

    #[test]
    fn records_become_entries() {
        let mut output = record(&[
            "E",
            "f",
            "f",
            "2048",
            "1700000000.5000000000",
            "644",
            "ada",
            "staff",
            "/srv/data/report.pdf",
        ]);
        output.extend(record(&["D", "1", "/srv/data/.cache"]));
        output.extend(record(&[
            "E",
            "d",
            "d",
            "4096",
            "1700000100.0000000000",
            "2755",
            "root",
            "root",
            "/srv/data/.cache",
        ]));
        output.extend(record(&[
            "E",
            "l",
            "d",
            "9",
            "1700000200.9",
            "777",
            "1000",
            "1000",
            "/srv/data/latest",
        ]));
        output.extend(record(&[
            "E",
            "l",
            "N",
            "7",
            "1700000300.0",
            "777",
            "ada",
            "",
            "/srv/data/dangling",
        ]));
        output.extend(record(&[
            "E",
            "p",
            "p",
            "0",
            "-1.5",
            "600",
            "ada",
            "staff",
            "/srv/data/pipe",
        ]));
        let mut parser = Parser::new("/srv/data");
        let records = parser.push(&output).unwrap();
        assert!(parser.is_empty());
        assert!(matches!(
            &records[1],
            Record::Folder { path, depth: 1 } if path == "/srv/data/.cache"
        ));
        let entries = entries(records);
        assert_eq!(entries.len(), 5);

        let file = &entries[0];
        assert_eq!(file.name, "report.pdf");
        assert_eq!(file.path, "/srv/data/report.pdf");
        assert_eq!(file.kind, EntryKind::File);
        assert_eq!(file.size, 2048);
        assert_eq!(file.modified, Some(1_700_000_000));
        assert_eq!(file.permissions, Some(0o644));
        assert_eq!(file.owner.as_deref(), Some("ada"));
        assert_eq!(file.group.as_deref(), Some("staff"));
        assert!(!file.hidden);

        let folder = &entries[1];
        assert_eq!(folder.kind, EntryKind::Dir);
        assert_eq!(folder.size, 0);
        assert_eq!(folder.permissions, Some(0o2755));
        assert!(folder.hidden);

        assert_eq!(entries[2].kind, EntryKind::Symlink);
        assert_eq!(entries[2].link_target, Some(LinkTarget::Dir));
        assert!(entries[2].is_dir_like());
        assert_eq!(entries[3].link_target, Some(LinkTarget::Broken));
        assert_eq!(entries[3].group, None);
        assert_eq!(entries[4].kind, EntryKind::Other);
        assert_eq!(entries[4].modified, Some(-2));
    }

    #[test]
    fn names_with_line_breaks_quotes_and_dashes_survive() {
        let names = [
            "two\nlines.log",
            "three\n\nlines\n",
            "it's \"quoted\"",
            "-rf",
            "--help",
            " leading and trailing ",
            "tab\there",
            "bell\u{7}",
            "E",
            "D",
            "日本語.txt",
        ];
        let mut output = Vec::new();
        for name in names {
            output.extend(record(&[
                "E",
                "f",
                "f",
                "1",
                "1700000000.0",
                "644",
                "ada",
                "staff",
                &format!("/srv/{name}"),
            ]));
        }
        let mut parser = Parser::new("/srv");
        let found = entries(parser.push(&output).unwrap());
        let found: Vec<&str> = found.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(found, names);
    }

    #[test]
    fn output_can_arrive_in_pieces_of_any_size() {
        let mut output = Vec::new();
        for index in 0..20 {
            output.extend(record(&["D", "1", &format!("/srv/folder {index}")]));
            output.extend(record(&[
                "E",
                "f",
                "f",
                "10",
                "1700000000.25",
                "600",
                "ada",
                "staff",
                &format!("/srv/folder {index}/file\n{index}"),
            ]));
        }
        for piece in [1, 2, 3, 7, 64, output.len()] {
            let mut parser = Parser::new("/srv");
            let mut records = Vec::new();
            for chunk in output.chunks(piece) {
                records.extend(parser.push(chunk).unwrap());
            }
            assert!(parser.is_empty());
            assert_eq!(records.len(), 40, "in pieces of {piece}");
            assert_eq!(entries(records)[19].name, "file\n19");
        }
    }

    #[test]
    fn the_start_folder_can_be_the_root() {
        let mut parser = Parser::new("/");
        let output = [
            record(&["E", "d", "d", "4096", "1.0", "755", "root", "root", "/"]),
            record(&[
                "E",
                "f",
                "f",
                "5",
                "2.0",
                "644",
                "root",
                "root",
                "/etc/motd",
            ]),
        ]
        .concat();
        let entries = entries(parser.push(&output).unwrap());
        assert_eq!(entries[0].path, "/");
        assert_eq!(entries[1].name, "motd");
    }

    #[test]
    fn paths_outside_the_start_folder_are_refused() {
        let paths = [
            "/etc/passwd",
            "/srv/data/../../etc/passwd",
            "/srv/data/./x",
            "/srv/data//x",
            "/srv/database/x",
            "relative/x",
            "",
            "/srv/data/",
        ];
        for path in paths {
            let output = record(&["E", "f", "f", "1", "1.0", "644", "a", "b", path]);
            assert_eq!(
                Parser::new("/srv/data").push(&output).map(|_| ()),
                Err(Malformed),
                "{path:?}"
            );
        }
    }

    #[test]
    fn anything_else_is_not_taken_for_records() {
        let malformed = [
            record(&["X", "/srv/a"]),
            record(&["E", "f", "f", "big", "1.0", "644", "a", "b", "/srv/a"]),
            record(&["E", "f", "f", "1", "soon", "644", "a", "b", "/srv/a"]),
            record(&["E", "f", "f", "1", "1.0", "rw-", "a", "b", "/srv/a"]),
            record(&["E", "file", "f", "1", "1.0", "644", "a", "b", "/srv/a"]),
            b"find: unknown predicate\n\0".to_vec(),
        ];
        for output in malformed {
            assert!(Parser::new("/srv").push(&output).is_err(), "{output:?}");
        }
        let mut endless = Parser::new("/srv");
        assert!(endless.push(&vec![b'a'; RECORD_LIMIT + 1]).is_err());
        let mut cut_short = Parser::new("/srv");
        assert!(cut_short.push(b"E\0f\0f\0").unwrap().is_empty());
        assert!(!cut_short.is_empty());
    }

    #[test]
    fn the_probe_wants_a_record_for_the_root() {
        let good = record(&[
            "E",
            "d",
            "d",
            "4096",
            "1700000000.0",
            "755",
            "root",
            "root",
            "/",
        ]);
        assert!(probe_passed(&good));
        assert!(!probe_passed(b""));
        assert!(!probe_passed(b"poros-ready\n"));
        assert!(!probe_passed(&good[..good.len() - 1]));
        assert!(!probe_passed(&[good.clone(), good].concat()));
        let words = shell_words(&probe_command());
        assert_eq!(&words[..2], ["sh", "-c"]);
        assert_eq!(words.len(), 3);
    }

    #[test]
    fn unreadable_folders_are_read_from_the_errors() {
        let errors = "find: '/srv/data/private': Permission denied\n\
                      find: \u{2018}/srv/data/secret stuff\u{2019}: Permission denied\n\
                      find: File system loop detected; '/srv/data/loop' is part of the same \
                      file system loop as '/srv/data'.\n\
                      find: '/srv/data/gone': No such file or directory\n\
                      find: /srv/data/odd: Input/output error\n\
                      find: something else went wrong\n\
                      not from find\n";
        let failures = unreadable(errors, "/srv/data");
        let listed: Vec<(&str, &str)> = failures
            .iter()
            .map(|failure| (failure.path.as_str(), failure.message.as_str()))
            .collect();
        assert_eq!(
            listed,
            [
                ("/srv/data/private", "Permission denied"),
                ("/srv/data/secret stuff", "Permission denied"),
                ("/srv/data/odd", "Input/output error"),
                ("/srv/data", "something else went wrong"),
            ]
        );
    }
}
