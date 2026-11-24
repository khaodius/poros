mod browse;
mod exec;
mod find;
mod integrity;
mod manage;
mod schedule;
mod share;
mod sync;
mod transfer;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use super::args::Command;
use super::context::{Connected, Context};
use super::exit::Exit;
use super::output::Reply;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::FileEntry;
use crate::protocol::{self, RemoteFileSystem};
use crate::remote_path;

/// Runs one command. `json` asks for output that fits in the reply instead of streaming it,
/// and `input` is what `write` writes when it does not read standard input.
pub async fn run(
    context: &Context,
    command: &Command,
    json: bool,
    input: Option<Vec<u8>>,
) -> AppResult<Reply> {
    match command {
        Command::Info => browse::info(context).await,
        Command::Connections => browse::connections(context).await,
        Command::Ls(arguments) => browse::list(context, arguments).await,
        Command::Stat(arguments) => browse::stat(context, arguments).await,
        Command::Cat(arguments) => browse::cat(context, arguments, json).await,
        Command::Write(arguments) => browse::write(context, arguments, input).await,
        Command::Du(arguments) => browse::usage(context, arguments).await,
        Command::Find(arguments) => find::find(context, arguments).await,
        Command::Get(arguments) => transfer::download(context, arguments).await,
        Command::Put(arguments) => transfer::upload(context, arguments).await,
        Command::Mkdir(arguments) => manage::make_dirs_command(context, arguments).await,
        Command::Rm(arguments) => manage::remove(context, arguments).await,
        Command::Mv(arguments) => manage::place(context, arguments, true).await,
        Command::Cp(arguments) => manage::place(context, arguments, false).await,
        Command::Chmod(arguments) => manage::change_mode(context, arguments).await,
        Command::Chown(arguments) => manage::change_owner(context, arguments).await,
        Command::Checksum(arguments) => integrity::checksum(context, arguments).await,
        Command::Verify(arguments) => integrity::verify(context, arguments).await,
        Command::Sync(arguments) => sync::synchronize(context, arguments).await,
        Command::Share(arguments) => share::share(context, arguments).await,
        Command::Exec(arguments) => exec::execute(context, arguments, json).await,
        Command::Schedule(arguments) => schedule::schedule(context, arguments).await,
        Command::Batch => Err(AppError::invalid("batch cannot run inside batch")),
    }
}

/// One operand gives its own value, several give a list.
fn one_or_many<T: Serialize>(mut values: Vec<T>) -> Value {
    if values.len() == 1 {
        serde_json::to_value(values.remove(0)).unwrap_or(Value::Null)
    } else {
        serde_json::to_value(values).unwrap_or(Value::Null)
    }
}

/// Runs `operation` on each path in turn. A single path's error is the command's; with
/// several, failures are listed beside the results and the run is partial.
async fn each<'a, Operation, Pending>(
    paths: &'a [String],
    mut operation: Operation,
) -> AppResult<Reply>
where
    Operation: FnMut(&'a str) -> Pending,
    Pending: Future<Output = AppResult<(Value, String)>>,
{
    if let [path] = paths {
        let (value, text) = operation(path).await?;
        return Ok(Reply::new(value, text));
    }
    let mut values = Vec::with_capacity(paths.len());
    let mut lines = Vec::with_capacity(paths.len());
    let mut first_failure = None;
    let mut failed = 0;
    for path in paths {
        match operation(path).await {
            Ok((value, text)) => {
                values.push(value);
                lines.push(text);
            }
            Err(error) => {
                failed += 1;
                lines.push(format!("{path}: {}", error.message));
                first_failure.get_or_insert(error.kind);
                values.push(json!({ "path": path, "error": error }));
            }
        }
    }
    let all_failed = first_failure.map_or(Exit::Failure, Exit::for_error);
    Ok(Reply::new(values, lines.join("\n")).with_failures(failed, paths.len(), all_failed))
}

fn missing(path: &str) -> AppError {
    AppError::new(ErrorKind::NotFound, format!("{path} does not exist")).with_path(path)
}

/// Two of the items, as (name, path) pairs, that have one name, so that in `folder` one would
/// replace the other.
fn same_name<'a>(
    items: impl IntoIterator<Item = (&'a str, &'a str)>,
    folder: &str,
) -> Option<AppError> {
    let mut seen = HashMap::new();
    for (name, path) in items {
        if let Some(first) = seen.insert(name, path) {
            let message = format!(
                "{first} and {path} have the same name, so one would replace the other in {folder}"
            );
            return Some(AppError::new(ErrorKind::AlreadyExists, message).with_path(path));
        }
    }
    None
}

/// Each path once, where it is first named, as `cp` copies a source named twice only once.
fn distinct(paths: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

/// Creates `path` and the folders above it that are missing; returns whether it made any.
async fn make_dirs(files: &dyn RemoteFileSystem, path: &str) -> AppResult<bool> {
    let path = protocol::normalize(path);
    match files.stat(&path).await? {
        Some(stat) if stat.is_dir => return Ok(false),
        Some(_) => return Err(protocol::not_a_folder(&path)),
        None => {}
    }
    let mut current = String::from("/");
    for part in path.split('/').filter(|part| !part.is_empty()) {
        current = remote_path::join(&current, part);
        match files.stat(&current).await? {
            Some(stat) if stat.is_dir => {}
            Some(_) => return Err(protocol::not_a_folder(&current)),
            None => files.ensure_dir(&current).await?,
        }
    }
    Ok(true)
}

/// Permissions in octal, as `chmod` takes them.
fn parse_mode(text: &str) -> AppResult<u32> {
    let digits = text.trim().trim_start_matches("0o");
    match u32::from_str_radix(digits, 8) {
        Ok(mode) if !digits.is_empty() && mode <= 0o7777 => Ok(mode),
        _ => Err(AppError::invalid(format!(
            "{text} is not a mode in octal, such as 644"
        ))),
    }
}

fn has_wildcards(name: &str) -> bool {
    name.contains(['*', '?'])
}

/// `*` matches any run of characters and `?` one, as a shell matches names.
fn matches_wildcards(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut at_pattern, mut at_name) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while at_name < name.len() {
        match pattern.get(at_pattern) {
            Some('*') => {
                backtrack = Some((at_pattern, at_name));
                at_pattern += 1;
            }
            Some('?') => {
                at_pattern += 1;
                at_name += 1;
            }
            Some(character) if *character == name[at_name] => {
                at_pattern += 1;
                at_name += 1;
            }
            _ => match backtrack {
                Some((star, start)) => {
                    at_pattern = star + 1;
                    at_name = start + 1;
                    backtrack = Some((star, start + 1));
                }
                None => return false,
            },
        }
    }
    pattern[at_pattern..]
        .iter()
        .all(|character| *character == '*')
}

/// A server path a command acts on. An empty one, which is what an unset shell variable
/// leaves, is refused rather than taken as the connection's own folder, which `.` names.
fn operand(connected: &Connected, path: &str) -> AppResult<String> {
    if path.trim().is_empty() {
        return Err(AppError::invalid(
            "A path is empty; write . for the connection's own folder",
        ));
    }
    Ok(connected.resolve(path))
}

/// `rm` and `mv` refuse a path whose last part is `.` or `..`, as POSIX `rm` does.
fn refuse_dots(paths: &[String]) -> AppResult<()> {
    for path in paths {
        let last = path.trim().trim_end_matches('/').rsplit('/').next();
        if matches!(last, Some("." | "..")) {
            return Err(AppError::invalid(format!(
                "{path} ends in . or ..; name the folder itself"
            ))
            .with_path(path.clone()));
        }
    }
    Ok(())
}

/// Server paths with `*` or `?` in their last part become the entries they match, as a shell
/// would expand them. Hidden names match only a pattern that starts with a dot.
async fn expand(connected: &Connected, paths: &[String]) -> AppResult<Vec<String>> {
    let mut expanded = Vec::with_capacity(paths.len());
    for path in paths {
        let resolved = operand(connected, path)?;
        let name = remote_path::file_name(&resolved);
        if !has_wildcards(name) {
            expanded.push(resolved);
            continue;
        }
        let folder = remote_path::parent(&resolved).unwrap_or_else(|| "/".into());
        let listing = connected.files().list_dir(&folder).await?;
        let mut found: Vec<FileEntry> = listing
            .entries
            .into_iter()
            .filter(|entry| name.starts_with('.') || !entry.name.starts_with('.'))
            .filter(|entry| matches_wildcards(name, &entry.name))
            .collect();
        if found.is_empty() {
            return Err(
                AppError::new(ErrorKind::NotFound, format!("Nothing matches {resolved}"))
                    .with_path(resolved),
            );
        }
        found.sort_by(|left, right| left.name.cmp(&right.name));
        expanded.extend(found.into_iter().map(|entry| entry.path));
    }
    Ok(expanded)
}

/// An absolute local path with `.` and `..` worked out, without following links.
fn absolute_local(path: &str) -> AppResult<PathBuf> {
    let absolute = std::path::absolute(Path::new(path))
        .map_err(|error| AppError::from(error).with_path(path))?;
    let mut clean = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other),
        }
    }
    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_as_a_shell_does() {
        assert!(matches_wildcards("*.log", "app.log"));
        assert!(matches_wildcards("*.log", ".log"));
        assert!(!matches_wildcards("*.log", "app.log.1"));
        assert!(matches_wildcards("app-??.tar*", "app-01.tar.gz"));
        assert!(!matches_wildcards("app-??.tar", "app-1.tar"));
        assert!(matches_wildcards("*a*b*", "xxaxxbxx"));
        assert!(matches_wildcards("*", ""));
        assert!(!matches_wildcards("?", ""));
        assert!(has_wildcards("*.txt") && !has_wildcards("plain.txt"));
    }

    #[test]
    fn items_with_one_name_are_found() {
        let named = |paths: &[&'static str]| {
            paths
                .iter()
                .map(|path| (remote_path::file_name(path), *path))
                .collect::<Vec<_>>()
        };
        let found = same_name(
            named(&["/in/a/report.csv", "/in/b/notes.txt", "/in/b/report.csv"]),
            "/archive",
        )
        .unwrap();
        assert_eq!(found.kind, ErrorKind::AlreadyExists);
        assert_eq!(
            found.message,
            "/in/a/report.csv and /in/b/report.csv have the same name, so one would replace the \
             other in /archive"
        );
        assert!(same_name(named(&["/in/a/report.csv", "/in/a/report"]), "/archive").is_none());
        assert!(same_name(named(&["/only.txt"]), "/archive").is_none());
    }

    #[test]
    fn paths_named_twice_are_kept_once() {
        let named = ["/in/p1.txt", "/in/p2.txt", "/in/p1.txt"].map(String::from);
        assert_eq!(distinct(named.to_vec()), ["/in/p1.txt", "/in/p2.txt"]);
    }

    #[test]
    fn paths_ending_in_dots_are_refused() {
        let refused = |path: &str| refuse_dots(&[path.to_string()]).is_err();
        for path in [".", "..", "./", "keep/..", "a/./", " .. ", "/srv/www/."] {
            assert!(refused(path), "{path}");
        }
        for path in ["/", "keep", ".hidden", "...", "a.b/c", "~", "../keep"] {
            assert!(!refused(path), "{path}");
        }
    }

    #[test]
    fn modes_are_octal() {
        assert_eq!(parse_mode("644").unwrap(), 0o644);
        assert_eq!(parse_mode("0755").unwrap(), 0o755);
        assert_eq!(parse_mode("0o4755").unwrap(), 0o4755);
        assert!(parse_mode("899").is_err());
        assert!(parse_mode("17777").is_err());
        assert!(parse_mode("").is_err());
    }

    #[test]
    fn local_paths_lose_dots_but_keep_their_names() {
        let base = std::env::current_dir().unwrap();
        assert_eq!(absolute_local(".").unwrap(), base);
        assert_eq!(
            absolute_local("a/./b/../c").unwrap(),
            base.join("a").join("c")
        );
    }
}
