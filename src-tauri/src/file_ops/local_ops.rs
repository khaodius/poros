//! Moving and copying on this computer's disks. Runs on a blocking thread.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use super::names;
use super::{summarize, Conflict, Method, Mode, MoveCopyRequest, OperationSummary, Progress};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::local;

const MAX_FREE_NAME_ATTEMPTS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dir,
    File,
    Link,
    Other,
}

impl Kind {
    fn of(metadata: &fs::Metadata) -> Self {
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            Self::Link
        } else if file_type.is_dir() {
            Self::Dir
        } else if file_type.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Destination {
    Fresh(PathBuf),
    Replace(PathBuf),
    Merge(PathBuf),
}

impl Destination {
    fn path(&self) -> &Path {
        match self {
            Self::Fresh(path) | Self::Replace(path) | Self::Merge(path) => path,
        }
    }
}

pub fn existing_names(directory: &str, names: &[String]) -> AppResult<Vec<String>> {
    Ok(names
        .iter()
        .filter(|name| Path::new(directory).join(name).symlink_metadata().is_ok())
        .cloned()
        .collect())
}

pub fn move_or_copy(
    request: &MoveCopyRequest,
    cancel: &CancellationToken,
    progress: &Progress,
) -> AppResult<OperationSummary> {
    let directory = PathBuf::from(&request.target_directory);
    if !directory.is_dir() {
        return Err(AppError::new(
            ErrorKind::NotFound,
            format!("{} is not a folder", request.target_directory),
        )
        .with_path(request.target_directory.clone()));
    }
    let work = Work { cancel, progress };
    progress.used(Method::Local);
    let results = request
        .sources
        .iter()
        .map(|source| {
            let placed = work
                .place(
                    Path::new(source),
                    &directory,
                    request.mode,
                    request.conflict,
                )
                .map(|placed| placed.map(|path| display(&path)));
            (source.clone(), placed)
        })
        .collect();
    summarize(progress, results, cancel)
}

struct Work<'a> {
    cancel: &'a CancellationToken,
    progress: &'a Progress,
}

impl Work<'_> {
    fn check_cancelled(&self) -> AppResult<()> {
        if self.cancel.is_cancelled() {
            Err(AppError::cancelled())
        } else {
            Ok(())
        }
    }

    fn place(
        &self,
        source: &Path,
        directory: &Path,
        mode: Mode,
        conflict: Conflict,
    ) -> AppResult<Option<PathBuf>> {
        self.check_cancelled()?;
        let metadata = source
            .symlink_metadata()
            .map_err(|error| with_path(error, source))?;
        let kind = Kind::of(&metadata);
        let name = source
            .file_name()
            .ok_or_else(|| AppError::invalid("Cannot move or copy a filesystem root"))?
            .to_string_lossy()
            .into_owned();
        if kind == Kind::Dir && directory.starts_with(source) {
            let verb = if mode == Mode::Move { "move" } else { "copy" };
            return Err(AppError::invalid(format!(
                "Cannot {verb} the folder {name} into itself"
            )));
        }
        let in_place = source.parent() == Some(directory);
        let destination = match (mode, in_place) {
            (Mode::Move, true) => return Ok(None),
            (Mode::Copy, true) => Some(Destination::Fresh(free_name(directory, &name, kind)?)),
            _ => destination(directory, &name, kind, conflict)?,
        };
        let Some(destination) = destination else {
            return Ok(None);
        };
        self.progress.working_on(&display(source));
        match mode {
            Mode::Move => self.move_to(source, kind, &destination)?,
            Mode::Copy => self.copy_to(source, kind, &destination)?,
        }
        Ok(Some(destination.path().to_path_buf()))
    }

    fn move_to(&self, source: &Path, kind: Kind, destination: &Destination) -> AppResult<()> {
        self.check_cancelled()?;
        let target = match destination {
            Destination::Merge(target) => return self.merge_into(source, target),
            Destination::Fresh(target) | Destination::Replace(target) => target,
        };
        // Replacing a file with a rename is a single step on every platform.
        match fs::rename(source, target) {
            Ok(()) => {
                self.progress.file_done();
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
                self.copy_to(source, kind, destination)?;
                local::delete(&[display(source)])
            }
            Err(error) => Err(with_path(error, source)),
        }
    }

    fn merge_into(&self, source: &Path, target: &Path) -> AppResult<()> {
        let mut all_moved = true;
        for child in fs::read_dir(source).map_err(|error| with_path(error, source))? {
            let child = child.map_err(|error| with_path(error, source))?.path();
            let moved = (|| {
                let metadata = child
                    .symlink_metadata()
                    .map_err(|error| with_path(error, &child))?;
                let kind = Kind::of(&metadata);
                let name = child.file_name().unwrap_or_default().to_string_lossy();
                let destination = destination(target, &name, kind, Conflict::Replace)?
                    .expect("replacing never skips");
                self.move_to(&child, kind, &destination)
            })();
            if let Err(error) = moved {
                self.check_cancelled()?;
                self.progress.fail(&display(&child), error);
                all_moved = false;
            }
        }
        if all_moved {
            fs::remove_dir(source).map_err(|error| with_path(error, source))?;
        }
        Ok(())
    }

    fn copy_to(&self, source: &Path, kind: Kind, destination: &Destination) -> AppResult<()> {
        self.check_cancelled()?;
        match (kind, destination) {
            (Kind::Dir, Destination::Fresh(target)) => self.copy_tree(source, target, false),
            (Kind::Dir, Destination::Merge(target)) => self.copy_tree(source, target, true),
            (Kind::File, _) => self.copy_file(source, destination),
            (Kind::Link, _) => self.copy_link(source, destination),
            (Kind::Dir, Destination::Replace(target)) => Err(AppError::new(
                ErrorKind::AlreadyExists,
                format!(
                    "{} is already a file there, so the folder cannot replace it",
                    target.file_name().unwrap_or_default().to_string_lossy()
                ),
            )),
            (Kind::Other, _) => Err(AppError::invalid(format!(
                "{} is not a file, folder or link",
                display(source)
            ))),
        }
    }

    fn copy_tree(&self, source: &Path, target: &Path, merge: bool) -> AppResult<()> {
        if !merge {
            fs::create_dir(target).map_err(|error| with_path(error, target))?;
        }
        for child in fs::read_dir(source).map_err(|error| with_path(error, source))? {
            self.check_cancelled()?;
            let child = child.map_err(|error| with_path(error, source))?.path();
            let copied = (|| {
                let metadata = child
                    .symlink_metadata()
                    .map_err(|error| with_path(error, &child))?;
                let kind = Kind::of(&metadata);
                let name = child.file_name().unwrap_or_default().to_string_lossy();
                let destination = if merge {
                    destination(target, &name, kind, Conflict::Replace)?
                        .expect("replacing never skips")
                } else {
                    Destination::Fresh(target.join(&*name))
                };
                self.copy_to(&child, kind, &destination)
            })();
            if let Err(error) = copied {
                self.check_cancelled()?;
                self.progress.fail(&display(&child), error);
            }
        }
        if !merge {
            let metadata = source
                .metadata()
                .map_err(|error| with_path(error, source))?;
            let _ = fs::set_permissions(target, metadata.permissions());
            if let Ok(modified) = metadata.modified() {
                let _ = fs::File::open(target).and_then(|folder| folder.set_modified(modified));
            }
        }
        Ok(())
    }

    /// A replaced file is written beside it and renamed over it, so it is never left half
    /// written.
    fn copy_file(&self, source: &Path, destination: &Destination) -> AppResult<()> {
        let target = destination.path();
        let writing = match destination {
            Destination::Replace(_) => target.with_file_name(names::temporary(
                &target.file_name().unwrap_or_default().to_string_lossy(),
            )),
            _ => target.to_path_buf(),
        };
        let copied = copy_file_contents(source, &writing).and_then(|bytes| {
            if writing != target {
                fs::rename(&writing, target)?;
            }
            Ok(bytes)
        });
        match copied {
            Ok(bytes) => {
                self.progress.add_bytes(bytes);
                self.progress.file_done();
                Ok(())
            }
            Err(error) => {
                let _ = fs::remove_file(&writing);
                Err(with_path(error, source))
            }
        }
    }

    fn copy_link(&self, source: &Path, destination: &Destination) -> AppResult<()> {
        let link_target = fs::read_link(source).map_err(|error| with_path(error, source))?;
        if let Destination::Replace(existing) = destination {
            fs::remove_file(existing).map_err(|error| with_path(error, existing))?;
        }
        make_link(&link_target, source, destination.path())
            .map_err(|error| with_path(error, destination.path()))?;
        self.progress.file_done();
        Ok(())
    }
}

/// Copies contents and permissions, then the modified time.
fn copy_file_contents(source: &Path, target: &Path) -> io::Result<u64> {
    let bytes = fs::copy(source, target)?;
    if let Ok(modified) = source.metadata().and_then(|metadata| metadata.modified()) {
        let _ = fs::File::options()
            .write(true)
            .open(target)
            .and_then(|file| file.set_modified(modified));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn make_link(link_target: &Path, _source: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(link_target, link)
}

#[cfg(windows)]
fn make_link(link_target: &Path, source: &Path, link: &Path) -> io::Result<()> {
    if fs::metadata(source).is_ok_and(|metadata| metadata.is_dir()) {
        std::os::windows::fs::symlink_dir(link_target, link)
    } else {
        std::os::windows::fs::symlink_file(link_target, link)
    }
}

fn destination(
    directory: &Path,
    name: &str,
    kind: Kind,
    conflict: Conflict,
) -> AppResult<Option<Destination>> {
    let path = directory.join(name);
    let Ok(existing) = path.symlink_metadata() else {
        return Ok(Some(Destination::Fresh(path)));
    };
    let existing_is_dir = Kind::of(&existing) == Kind::Dir;
    match conflict {
        Conflict::Skip => Ok(None),
        Conflict::KeepBoth => Ok(Some(Destination::Fresh(free_name(directory, name, kind)?))),
        Conflict::Replace => {
            match (kind == Kind::Dir, existing_is_dir) {
                (true, true) => Ok(Some(Destination::Merge(path))),
                (false, false) => Ok(Some(Destination::Replace(path))),
                (incoming_is_dir, _) => {
                    let (existing, incoming) = if incoming_is_dir {
                        ("a file", "folder")
                    } else {
                        ("a folder", "file")
                    };
                    Err(AppError::new(
                    ErrorKind::AlreadyExists,
                    format!("{name} is already {existing} there, so the {incoming} cannot replace it"),
                )
                .with_path(display(&path)))
                }
            }
        }
    }
}

fn free_name(directory: &Path, name: &str, kind: Kind) -> AppResult<PathBuf> {
    names::numbered(name, kind == Kind::Dir)
        .take(MAX_FREE_NAME_ATTEMPTS)
        .map(|candidate| directory.join(candidate))
        .find(|path| path.symlink_metadata().is_err())
        .ok_or_else(|| {
            AppError::new(
                ErrorKind::AlreadyExists,
                format!("No free name for a copy of {name}"),
            )
        })
}

fn with_path(error: io::Error, path: &Path) -> AppError {
    AppError::from(error).with_path(display(path))
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        mode: Mode,
        sources: &[&Path],
        target: &Path,
        conflict: Conflict,
    ) -> MoveCopyRequest {
        MoveCopyRequest {
            operation_id: "test".into(),
            location: super::super::Location::Local,
            mode,
            sources: sources.iter().map(|path| display(path)).collect(),
            target_directory: display(target),
            conflict,
        }
    }

    fn run(request: &MoveCopyRequest) -> OperationSummary {
        move_or_copy(request, &CancellationToken::new(), &Progress::default()).unwrap()
    }

    #[test]
    fn copies_trees_and_keeps_both_in_place() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("site/css")).unwrap();
        fs::write(root.join("site/index.html"), b"<p>hi</p>").unwrap();
        fs::write(root.join("site/css/main.css"), b"p{}").unwrap();
        fs::create_dir(root.join("backup")).unwrap();

        let summary = run(&request(
            Mode::Copy,
            &[&root.join("site")],
            &root.join("backup"),
            Conflict::KeepBoth,
        ));
        assert_eq!(summary.placed, [display(&root.join("backup/site"))]);
        assert_eq!(
            fs::read(root.join("backup/site/css/main.css")).unwrap(),
            b"p{}"
        );
        assert!(root.join("site/index.html").exists());

        let summary = run(&request(
            Mode::Copy,
            &[&root.join("site")],
            root,
            Conflict::KeepBoth,
        ));
        assert_eq!(summary.placed, [display(&root.join("site (2)"))]);
        assert_eq!(
            fs::read(root.join("site (2)/index.html")).unwrap(),
            b"<p>hi</p>"
        );
    }

    #[test]
    fn moves_with_replace_merge_and_skip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("new/docs")).unwrap();
        fs::write(root.join("new/docs/a.txt"), b"new a").unwrap();
        fs::write(root.join("new/docs/b.txt"), b"new b").unwrap();
        fs::create_dir_all(root.join("live/docs")).unwrap();
        fs::write(root.join("live/docs/a.txt"), b"old a").unwrap();
        fs::write(root.join("live/docs/keep.txt"), b"keep").unwrap();

        let skipped = run(&request(
            Mode::Move,
            &[&root.join("new/docs")],
            &root.join("live"),
            Conflict::Skip,
        ));
        assert_eq!(skipped.skipped, 1);
        assert!(root.join("new/docs").exists());

        let summary = run(&request(
            Mode::Move,
            &[&root.join("new/docs")],
            &root.join("live"),
            Conflict::Replace,
        ));
        assert!(summary.failures.is_empty());
        assert_eq!(fs::read(root.join("live/docs/a.txt")).unwrap(), b"new a");
        assert_eq!(fs::read(root.join("live/docs/b.txt")).unwrap(), b"new b");
        assert_eq!(fs::read(root.join("live/docs/keep.txt")).unwrap(), b"keep");
        assert!(!root.join("new/docs").exists());
    }

    #[test]
    fn refuses_to_copy_a_folder_into_itself() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        let summary = run(&request(
            Mode::Copy,
            &[&root.join("a")],
            &root.join("a/b"),
            Conflict::KeepBoth,
        ));
        assert_eq!(summary.failures.len(), 1);
        assert!(summary.failures[0].message.contains("into itself"));
        assert!(!root.join("a/b/a").exists());
    }

    #[test]
    fn a_folder_cannot_replace_a_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("src/item")).unwrap();
        fs::create_dir(root.join("dst")).unwrap();
        fs::write(root.join("dst/item"), b"file").unwrap();
        let summary = run(&request(
            Mode::Copy,
            &[&root.join("src/item")],
            &root.join("dst"),
            Conflict::Replace,
        ));
        assert_eq!(summary.failures.len(), 1);
        assert_eq!(fs::read(root.join("dst/item")).unwrap(), b"file");
    }
}
