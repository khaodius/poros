//! Carrying out a plan: what each chosen item takes on either side, in rsync's order. What a
//! copy replaces goes first, then deletions unless they wait for the copies, then folders,
//! links and attribute changes; files go to the transfer queue. Once the queue is done with
//! them, delayed deletions, folder times and permissions, and the report follow.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::watch;

use super::chmod::Chmod;
use super::options::{AppendMode, SyncOptions};
use super::plan::{SyncAction, SyncDirection, SyncItem, SyncReason};
use super::tree::{Node, Tree};
use super::{plural, Place, StoredPlan, SyncReport, SyncRunSummary};
use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel};
use crate::format::format_size;
use crate::local;
use crate::remote_path;
use crate::sftp::RemoteFs;
use crate::transfer::{
    ensure_remote_folders, keep_local_backup, keep_remote_backup, Backup, Direction, ExistsAction,
    FileTransfer, JobId, JobOptions, TransferWatch,
};

/// Where `--backup` puts what is replaced or deleted.
struct Backups {
    suffix: String,
    /// `--backup-dir` on each side; `None` keeps backups beside the files.
    local_root: Option<String>,
    remote_root: Option<String>,
}

/// One run of a plan, shared with the work that follows the transfers.
pub(super) struct Run {
    pub id: String,
    pub stored: Arc<StoredPlan>,
    fs: Arc<RemoteFs>,
    events: Events,
    /// What every queued file is asked for; files that need more get their own copy.
    base: Arc<JobOptions>,
    backups: Option<Backups>,
    chmod: Chmod,
}

impl Run {
    pub fn new(stored: Arc<StoredPlan>, fs: Arc<RemoteFs>, events: Events) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let options = &stored.options;
        let base = Arc::new(base_job_options(options, &id));
        let backups = options.backup.then(|| {
            let folder = options.backup_dir.trim();
            let (local_root, remote_root) = if folder.is_empty() {
                (None, None)
            } else {
                let local_root = if Path::new(folder).is_absolute() {
                    folder.to_string()
                } else {
                    local_path(&stored.local.root, folder)
                };
                let remote_root = if folder.starts_with('/') {
                    folder.to_string()
                } else {
                    remote_path::join(&stored.remote.root, folder)
                };
                (Some(local_root), Some(remote_root))
            };
            Backups {
                suffix: options.backup_suffix().to_string(),
                local_root,
                remote_root,
            }
        });
        let chmod = options.chmod_rules();
        Self {
            id,
            stored,
            fs,
            events,
            base,
            backups,
            chmod,
        }
    }

    fn options(&self) -> &SyncOptions {
        &self.stored.options
    }

    fn tree(&self, place: Place) -> &Tree {
        match place {
            Place::Local => &self.stored.local,
            Place::Remote => &self.stored.remote,
        }
    }

    fn path(&self, place: Place, relative: &str) -> String {
        match place {
            Place::Local => local_path(&self.stored.local.root, relative),
            Place::Remote if relative.is_empty() => self.stored.remote.root.clone(),
            Place::Remote => remote_path::join(&self.stored.remote.root, relative),
        }
    }

    /// Where the backup of `relative` goes on `place`.
    fn backup_path(&self, backups: &Backups, place: Place, relative: &str) -> String {
        let kept = match place {
            Place::Local => backups
                .local_root
                .as_deref()
                .map(|root| local_path(root, relative)),
            Place::Remote => backups
                .remote_root
                .as_deref()
                .map(|root| remote_path::join(root, relative)),
        };
        let path = kept.unwrap_or_else(|| self.path(place, relative));
        format!("{path}{}", backups.suffix)
    }

    /// The folder a queued file's backup goes into, `None` for beside it.
    fn backup_folder(&self, backups: &Backups, place: Place, relative: &str) -> Option<String> {
        let parent = relative.rsplit_once('/').map_or("", |(parent, _)| parent);
        match place {
            Place::Local => backups
                .local_root
                .as_deref()
                .map(|root| local_path(root, parent)),
            Place::Remote => backups.remote_root.as_deref().map(|root| {
                if parent.is_empty() {
                    root.to_string()
                } else {
                    remote_path::join(root, parent)
                }
            }),
        }
    }

    /// Whether a file or folder inside a folder being copied is copied too.
    fn copies(&self, source: &Tree, relative: &str, node: &Node) -> bool {
        let plan_options = &self.stored.plan_options;
        if !node.is_dir {
            return plan_options.size_allowed(node);
        }
        !plan_options.prune_empty_dirs
            || source
                .inside(relative)
                .any(|(_, inner)| !inner.is_dir && plan_options.size_allowed(inner))
    }

    fn transfer(
        &self,
        to: Place,
        relative: &str,
        node: &Node,
        existing: Option<&Node>,
        reason: SyncReason,
    ) -> FileTransfer {
        let from = to.other();
        let parent = relative.rsplit_once('/').map_or("", |(parent, _)| parent);
        FileTransfer {
            direction: match to {
                Place::Remote => Direction::Upload,
                Place::Local => Direction::Download,
            },
            name: relative.rsplit('/').next().unwrap_or(relative).to_string(),
            source: self.path(from, relative),
            target: self.path(to, relative),
            target_directory: self.path(to, parent),
            size: node.size,
            resolution: if reason == SyncReason::Append {
                ExistsAction::Resume
            } else {
                ExistsAction::Overwrite
            },
            options: Some(self.job_options(to, relative, node, existing, reason)),
        }
    }

    fn job_options(
        &self,
        to: Place,
        relative: &str,
        node: &Node,
        existing: Option<&Node>,
        reason: SyncReason,
    ) -> Arc<JobOptions> {
        let options = self.options();
        let (owner, group) = self.stored.owners.ids(node, to);
        // rsync applies --chmod to the permissions the source sends, which only reach an
        // existing file with --perms.
        let chmod = if !options.chmod.trim().is_empty() && (options.perms || existing.is_none()) {
            options.chmod.clone()
        } else {
            String::new()
        };
        let backup = match (&self.backups, existing) {
            (Some(backups), Some(_)) => Some(Backup {
                folder: self.backup_folder(backups, to, relative),
                suffix: backups.suffix.clone(),
            }),
            _ => None,
        };
        let appending = reason == SyncReason::Append;
        if owner.is_none() && group.is_none() && chmod.is_empty() && backup.is_none() && !appending
        {
            return self.base.clone();
        }
        Arc::new(JobOptions {
            owner,
            group,
            chmod,
            backup,
            // Appending sends the rest of the file as it is.
            delta_transfers: if appending {
                Some(false)
            } else {
                self.base.delta_transfers
            },
            ..(*self.base).clone()
        })
    }

    /// Deletes each entry, moving its files to their backups first when asked to.
    async fn delete(&self, entries: &[(Place, String)], failures: &mut Vec<String>) -> u64 {
        let mut deleted = 0;
        for (place, relative) in entries {
            let result = match &self.backups {
                Some(backups) => self.back_up_and_delete(backups, *place, relative).await,
                None => self.remove(*place, relative).await,
            };
            match result {
                Ok(()) => deleted += 1,
                Err(error) => failures.push(format!(
                    "{}: {}",
                    self.path(*place, relative),
                    error.message
                )),
            }
        }
        deleted
    }

    async fn remove(&self, place: Place, relative: &str) -> AppResult<()> {
        let path = self.path(place, relative);
        match place {
            Place::Remote => self.fs.delete(&[path]).await,
            Place::Local => tokio::task::spawn_blocking(move || local::delete(&[path])).await?,
        }
    }

    async fn back_up_and_delete(
        &self,
        backups: &Backups,
        place: Place,
        relative: &str,
    ) -> AppResult<()> {
        let tree = self.tree(place);
        let is_dir = tree.nodes.get(relative).is_some_and(|node| node.is_dir);
        let files: Vec<&str> = if is_dir {
            tree.inside(relative)
                .filter(|(_, node)| !node.is_dir)
                .map(|(path, _)| path.as_str())
                .collect()
        } else {
            vec![relative]
        };
        for file in files {
            let from = self.path(place, file);
            let to = self.backup_path(backups, place, file);
            match place {
                Place::Remote => keep_remote_backup(&self.fs, &from, &to).await?,
                Place::Local => {
                    tokio::task::spawn_blocking(move || {
                        keep_local_backup(Path::new(&from), Path::new(&to))
                            .map_err(|error| AppError::from(error).with_path(from))
                    })
                    .await??
                }
            }
        }
        // Only folders are left in a folder whose files were kept.
        if is_dir {
            self.remove(place, relative).await?;
        }
        Ok(())
    }

    async fn create_folders(&self, folders: &[(Place, String)], failures: &mut Vec<String>) -> u64 {
        let mut created = 0;
        let mut local_folders = Vec::new();
        for (place, relative) in folders {
            let path = self.path(*place, relative);
            match place {
                Place::Remote => match self.fs.ensure_dir(&path).await {
                    Ok(()) => created += 1,
                    Err(error) => failures.push(format!("{path}: {}", error.message)),
                },
                Place::Local => local_folders.push(path),
            }
        }
        let count = local_folders.len() as u64;
        let local_failures = tokio::task::spawn_blocking(move || {
            local_folders
                .iter()
                .filter_map(|folder| {
                    std::fs::create_dir_all(folder)
                        .err()
                        .map(|error| format!("{folder}: {error}"))
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        created += count - local_failures.len() as u64;
        failures.extend(local_failures);
        created
    }

    async fn create_links(
        &self,
        links: &[(Place, String, String)],
        failures: &mut Vec<String>,
    ) -> u64 {
        let mut created = 0;
        for (place, relative, text) in links {
            let path = self.path(*place, relative);
            let result = match place {
                Place::Remote => self.fs.make_symlink(&path, text).await,
                Place::Local => {
                    let (path, text) = (path.clone(), text.clone());
                    tokio::task::spawn_blocking(move || {
                        make_local_link(&text, Path::new(&path))
                            .map_err(|error| AppError::from(error).with_path(path))
                    })
                    .await
                    .map_err(AppError::from)
                    .and_then(|result| result)
                }
            };
            match result {
                Ok(()) => created += 1,
                Err(error) => failures.push(format!("{path}: {}", error.message)),
            }
        }
        created
    }

    /// Sets the time, permissions and owner of files whose contents already match.
    async fn update_attributes(
        &self,
        entries: &[(Place, String)],
        failures: &mut Vec<String>,
    ) -> u64 {
        let options = self.options();
        let mut updated = 0;
        for (to, relative) in entries {
            let Some(source) = self.tree(to.other()).nodes.get(relative) else {
                continue;
            };
            let change = Attributes {
                modified: source.modified.filter(|_| options.times),
                permissions: source
                    .permissions
                    .filter(|_| options.perms)
                    .map(|mode| self.chmod.apply(mode, false)),
                chmod_current: false,
                owner: self.stored.owners.ids(source, *to),
            };
            let path = self.path(*to, relative);
            match self.set_attributes(*to, &path, false, &change).await {
                Ok(()) => updated += 1,
                Err(error) => failures.push(format!("{path}: {}", error.message)),
            }
        }
        updated
    }

    async fn set_attributes(
        &self,
        place: Place,
        path: &str,
        is_dir: bool,
        change: &Attributes,
    ) -> AppResult<()> {
        match place {
            Place::Remote => {
                let permissions = match change.permissions {
                    Some(mode) => Some(mode),
                    None if change.chmod_current => self
                        .fs
                        .stat(path)
                        .await?
                        .and_then(|stat| stat.permissions)
                        .map(|mode| self.chmod.apply(mode, is_dir)),
                    None => None,
                };
                self.fs
                    .set_attributes(path, change.modified, permissions)
                    .await?;
                let (uid, gid) = change.owner;
                if uid.is_some() || gid.is_some() {
                    let current = self.fs.lstat_entry(path).await?;
                    let uid = uid.or(current.and_then(|stat| stat.uid));
                    let gid = gid.or(current.and_then(|stat| stat.gid));
                    if let (Some(uid), Some(gid)) = (uid, gid) {
                        let same = current
                            .is_some_and(|stat| stat.uid == Some(uid) && stat.gid == Some(gid));
                        if !same {
                            self.fs.set_owner(path, uid, gid).await?;
                        }
                    }
                }
                Ok(())
            }
            Place::Local => {
                let path = path.to_string();
                let change = change.clone();
                let chmod = self.chmod.clone();
                tokio::task::spawn_blocking(move || {
                    set_local_attributes(Path::new(&path), is_dir, &change, &chmod)
                        .map_err(|error| AppError::from(error).with_path(path))
                })
                .await?
            }
        }
    }

    /// Times, permissions and owners of folders, set once their contents stop changing.
    fn folder_fixes(&self, work: &Work) -> Vec<FolderFix> {
        let options = self.options();
        let times = options.times && !options.omit_dir_times;
        let created: BTreeSet<&(Place, String)> = work.folders.iter().collect();
        let mut candidates: BTreeSet<(Place, String)> = work.folders.iter().cloned().collect();
        if self.stored.direction != SyncDirection::Both {
            candidates.extend(work.touched.iter().cloned());
            let to = match self.stored.direction {
                SyncDirection::Upload => Place::Remote,
                _ => Place::Local,
            };
            let (source, target) = (self.tree(to.other()), self.tree(to));
            let tolerance = self.stored.plan_options.time_tolerance_secs;
            for (relative, node) in &source.nodes {
                let Some(existing) = target.nodes.get(relative).filter(|node| node.is_dir) else {
                    continue;
                };
                if !node.is_dir {
                    continue;
                }
                let time_differs = match (node.modified, existing.modified) {
                    (Some(source_time), Some(target_time)) => {
                        (source_time - target_time).abs() > tolerance
                    }
                    _ => false,
                };
                let permissions_differ = options.perms
                    && node.permissions.is_some()
                    && node.permissions.map(|mode| self.chmod.apply(mode, true))
                        != existing.permissions.map(|mode| mode & 0o7777);
                let (uid, gid) = self.stored.owners.ids(node, to);
                let owner_differs = uid.is_some_and(|uid| existing.uid != Some(uid))
                    || gid.is_some_and(|gid| existing.gid != Some(gid));
                if (times && time_differs) || permissions_differ || owner_differs {
                    candidates.insert((to, relative.clone()));
                }
            }
        }
        candidates
            .into_iter()
            .filter_map(|(to, relative)| {
                let node = self.tree(to.other()).nodes.get(&relative)?;
                if !node.is_dir {
                    return None;
                }
                let change = Attributes {
                    modified: node.modified.filter(|_| times),
                    permissions: node
                        .permissions
                        .filter(|_| options.perms)
                        .map(|mode| self.chmod.apply(mode, true)),
                    chmod_current: !options.perms
                        && !self.chmod.is_empty()
                        && created.contains(&(to, relative.clone())),
                    owner: self.stored.owners.ids(node, to),
                };
                change.changes_anything().then_some(FolderFix {
                    place: to,
                    relative,
                    change,
                })
            })
            .collect()
    }

    async fn fix_folders(&self, fixes: &[FolderFix], failures: &mut Vec<String>) {
        // The deepest first, so setting a folder's time is not undone inside it.
        for fix in fixes.iter().rev() {
            let path = self.path(fix.place, &fix.relative);
            if let Err(error) = self
                .set_attributes(fix.place, &path, true, &fix.change)
                .await
            {
                failures.push(format!("{path}: {}", error.message));
            }
        }
    }

    /// `--remove-source-files` for files that were already the same on the target.
    async fn remove_sources(
        &self,
        place: Place,
        paths: &[String],
        failures: &mut Vec<String>,
    ) -> u64 {
        let mut removed = 0;
        for relative in paths {
            let path = self.path(place, relative);
            let result = match place {
                Place::Remote => self.fs.remove_file(&path).await,
                Place::Local => {
                    let local = path.clone();
                    tokio::task::spawn_blocking(move || {
                        std::fs::remove_file(&local)
                            .map_err(|error| AppError::from(error).with_path(local))
                    })
                    .await
                    .map_err(AppError::from)
                    .and_then(|result| result)
                }
            };
            match result {
                Ok(()) => removed += 1,
                Err(error) => failures.push(format!("{path}: {}", error.message)),
            }
        }
        removed
    }

    fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.events
            .log(level, Some(&self.stored.session_id), message);
    }

    /// rsync's `--stats`, for what can be counted here.
    fn stats(
        &self,
        transferred: u64,
        transferred_bytes: u64,
        created: u64,
        deleted: u64,
    ) -> Vec<String> {
        let source = match self.stored.direction {
            SyncDirection::Download => &self.stored.remote,
            _ => &self.stored.local,
        };
        let (mut files, mut folders, mut links, mut size) = (0u64, 0u64, 0u64, 0u64);
        for node in source.nodes.values() {
            if node.is_dir {
                folders += 1;
            } else if node.link.is_some() {
                links += 1;
            } else {
                files += 1;
                size += node.size;
            }
        }
        vec![
            format!(
                "Number of files: {} (reg: {files}, dir: {folders}, link: {links})",
                files + folders + links
            ),
            format!("Number of created files: {created}"),
            format!("Number of deleted files: {deleted}"),
            format!("Number of regular files transferred: {transferred}"),
            format!("Total file size: {} ({size} bytes)", format_size(size)),
            format!(
                "Total transferred file size: {} ({transferred_bytes} bytes)",
                format_size(transferred_bytes)
            ),
        ]
    }
}

/// What each queued file is asked for, from the options.
fn base_job_options(options: &SyncOptions, run_id: &str) -> JobOptions {
    JobOptions {
        preserve_timestamps: Some(options.times),
        preserve_permissions: Some(options.perms),
        temporary_files: options.inplace.then_some(false),
        delta_transfers: options.whole_file.then_some(false),
        flush_to_disk: options.fsync.then_some(true),
        verify_checksums: (options.append == AppendMode::Verify).then_some(true),
        log_each_file: options.verbose.then_some(true),
        rsync_path: Some(options.rsync_path.trim().to_string()).filter(|path| !path.is_empty()),
        block_size: options.block_size.and_then(|size| u32::try_from(size).ok()),
        executability: options.executability && !options.perms,
        chmod: String::new(),
        owner: None,
        group: None,
        backup: None,
        remove_source: options.remove_source_files,
        compress: options.compress,
        rate_limit: options.bwlimit_kib.saturating_mul(1024),
        limit_group: run_id.to_string(),
    }
}

#[derive(Debug, Clone, Default)]
struct Attributes {
    modified: Option<i64>,
    permissions: Option<u32>,
    /// Applies the `--chmod` rules to the permissions the entry has now.
    chmod_current: bool,
    owner: (Option<u32>, Option<u32>),
}

impl Attributes {
    fn changes_anything(&self) -> bool {
        self.modified.is_some()
            || self.permissions.is_some()
            || self.chmod_current
            || self.owner != (None, None)
    }
}

struct FolderFix {
    place: Place,
    relative: String,
    change: Attributes,
}

/// What carrying out the chosen items takes, gathered before anything changes.
#[derive(Default)]
pub(super) struct Work {
    /// What copies replace, removed first: entries of another type, and changed links.
    replaced: Vec<(Place, String)>,
    deletions: Vec<(Place, String)>,
    folders: Vec<(Place, String)>,
    links: Vec<(Place, String, String)>,
    attributes: Vec<(Place, String)>,
    files: Vec<FileTransfer>,
    /// Folders whose contents change, so their times are set again afterwards.
    touched: BTreeSet<(Place, String)>,
    /// The changes in rsync's `--itemize-changes` form.
    itemized: Vec<String>,
}

impl Work {
    pub fn add(&mut self, run: &Run, item: &SyncItem, action: SyncAction) {
        let line = match action {
            // A settled conflict is itemized as the copy it became.
            SyncAction::Upload if item.itemized.is_empty() => "<f.........".to_string(),
            SyncAction::Download if item.itemized.is_empty() => ">f.........".to_string(),
            _ => item.itemized.clone(),
        };
        if !line.is_empty() {
            let suffix = if item.is_dir { "/" } else { "" };
            self.itemized
                .push(format!("{line:<11} {}{suffix}", item.path));
        }
        let (to, deletes) = match action {
            SyncAction::Upload => (Place::Remote, false),
            SyncAction::Download => (Place::Local, false),
            SyncAction::DeleteRemote => (Place::Remote, true),
            SyncAction::DeleteLocal => (Place::Local, true),
            SyncAction::Conflict => return,
        };
        self.touch(to, &item.path);
        if deletes {
            self.deletions.push((to, item.path.clone()));
            return;
        }
        let source = run.tree(to.other());
        let Some(node) = source.nodes.get(&item.path) else {
            return;
        };
        let target = run.tree(to).nodes.get(&item.path);
        if item.reason == SyncReason::Attributes {
            self.attributes.push((to, item.path.clone()));
            return;
        }
        let replaces = item.reason == SyncReason::TypeDiffers || node.link.is_some();
        if target.is_some() && replaces {
            self.replaced.push((to, item.path.clone()));
        }
        let existing = target.filter(|_| !replaces);
        let mut entries = vec![(&item.path, node)];
        if node.is_dir {
            entries.extend(
                source
                    .inside(&item.path)
                    .filter(|(relative, inner)| run.copies(source, relative, inner)),
            );
        }
        for (relative, node) in entries {
            if node.is_dir {
                self.folders.push((to, relative.clone()));
            } else if let Some(text) = &node.link {
                self.links.push((to, relative.clone(), text.clone()));
            } else {
                let existing = existing.filter(|_| relative == &item.path);
                self.files
                    .push(run.transfer(to, relative, node, existing, item.reason));
            }
        }
    }

    fn touch(&mut self, place: Place, relative: &str) {
        if let Some((parent, _)) = relative.rsplit_once('/') {
            self.touched.insert((place, parent.to_string()));
        }
    }
}

/// What a run does before and after the queue, and what it reports.
pub(super) struct Outcome {
    pub summary: SyncRunSummary,
    /// Work left for after the transfers; `None` when the run is already over.
    pub follow_up: Option<FollowUp>,
}

pub(super) struct FollowUp {
    run: Arc<Run>,
    ids: Vec<JobId>,
    delete_later: Vec<(Place, String)>,
    fixes: Vec<FolderFix>,
    summary: SyncRunSummary,
    lines: Vec<String>,
}

/// Does everything up to queueing the files, which `queue` does.
pub(super) async fn start<Q, F>(run: Arc<Run>, mut work: Work, queue: Q) -> AppResult<Outcome>
where
    Q: FnOnce(Vec<FileTransfer>) -> F,
    F: std::future::Future<Output = AppResult<Vec<JobId>>>,
{
    let options = run.options().clone();
    let stored = run.stored.clone();
    let mut summary = SyncRunSummary {
        run_id: run.id.clone(),
        dry_run: options.dry_run,
        ..SyncRunSummary::default()
    };
    let mut lines = vec![format!(
        "Synchronizing {} with {}: {}",
        stored.local.root, stored.remote.root, stored.arguments
    )];
    if options.itemize {
        for line in &work.itemized {
            run.log(LogLevel::Info, line.clone());
        }
    }
    lines.append(&mut work.itemized);
    let source_place = match stored.direction {
        SyncDirection::Download => Place::Remote,
        _ => Place::Local,
    };
    let removed_unchanged: Vec<String> = if options.remove_source_files {
        stored
            .unchanged_files
            .iter()
            .chain(work.attributes.iter().map(|(_, path)| path))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    if options.dry_run {
        summary.queued_files = work.files.len() as u64;
        summary.queued_bytes = work.files.iter().map(|file| file.size).sum();
        summary.deleted = work.deletions.len() as u64;
        summary.created_folders = work.folders.len() as u64;
        summary.created_links = work.links.len() as u64;
        summary.updated = work.attributes.len() as u64;
        summary.removed_sources = removed_unchanged.len() as u64;
        let message = format!(
            "Dry run of {} with {}: {} would be copied ({}), {} deleted, {} created; nothing was changed",
            stored.local.root,
            stored.remote.root,
            plural(summary.queued_files, "file"),
            format_size(summary.queued_bytes),
            plural(summary.deleted, "item"),
            plural(summary.created_folders, "folder"),
        );
        run.log(LogLevel::Info, message.clone());
        lines.push(message);
        write_log(&run, &lines).await;
        return Ok(Outcome {
            summary,
            follow_up: None,
        });
    }

    let mut failures = Vec::new();
    // A target folder that is not there yet; the plan checked that its parent is, unless
    // --mkpath creates that too.
    if stored.local.missing {
        let root = stored.local.root.clone();
        tokio::task::spawn_blocking(move || std::fs::create_dir_all(&root))
            .await?
            .map_err(|error| AppError::from(error).with_path(stored.local.root.clone()))?;
    }
    if stored.remote.missing {
        ensure_remote_folders(&run.fs, &stored.remote.root).await?;
    }

    run.delete(&work.replaced, &mut failures).await;
    let delete_later = if options.delete_timing.after_copying() {
        summary.deleting_later = work.deletions.len() as u64;
        std::mem::take(&mut work.deletions)
    } else {
        summary.deleted = run.delete(&work.deletions, &mut failures).await;
        Vec::new()
    };
    summary.created_folders = run.create_folders(&work.folders, &mut failures).await;
    summary.created_links = run.create_links(&work.links, &mut failures).await;
    summary.updated = run.update_attributes(&work.attributes, &mut failures).await;
    let fixes = run.folder_fixes(&work);
    if !removed_unchanged.is_empty() {
        summary.removed_sources = run
            .remove_sources(source_place, &removed_unchanged, &mut failures)
            .await;
    }

    let files = std::mem::take(&mut work.files);
    summary.queued_files = files.len() as u64;
    summary.queued_bytes = files.iter().map(|file| file.size).sum();
    let ids = if files.is_empty() {
        Vec::new()
    } else {
        queue(files).await?
    };

    let mut message = format!(
        "Synchronizing {} with {}: {} queued ({})",
        stored.local.root,
        stored.remote.root,
        plural(summary.queued_files, "file"),
        format_size(summary.queued_bytes)
    );
    if summary.deleted > 0 {
        message.push_str(&format!(", {} deleted", plural(summary.deleted, "item")));
    }
    if summary.deleting_later > 0 {
        message.push_str(&format!(
            ", {} to delete once copying is done",
            plural(summary.deleting_later, "item")
        ));
    }
    if summary.created_folders > 0 {
        message.push_str(&format!(
            ", {} created",
            plural(summary.created_folders, "folder")
        ));
    }
    if summary.created_links > 0 {
        message.push_str(&format!(
            ", {} created",
            plural(summary.created_links, "link")
        ));
    }
    if summary.updated > 0 {
        message.push_str(&format!(
            ", {} updated",
            plural(summary.updated, "attribute change")
        ));
    }
    if summary.removed_sources > 0 {
        message.push_str(&format!(
            ", {} removed from the source",
            plural(summary.removed_sources, "unchanged file")
        ));
    }
    run.log(LogLevel::Info, message.clone());
    lines.push(message);
    for failure in &failures {
        run.log(LogLevel::Error, failure.clone());
    }
    lines.extend(failures.iter().cloned());
    summary.failures = failures;
    Ok(Outcome {
        summary: summary.clone(),
        follow_up: Some(FollowUp {
            run,
            ids,
            delete_later,
            fixes,
            summary,
            lines,
        }),
    })
}

impl FollowUp {
    /// Waits for the queued files, then finishes the run and reports it.
    pub async fn finish(self, watch: TransferWatch, report: watch::Sender<Option<SyncReport>>) {
        let FollowUp {
            run,
            ids,
            delete_later,
            fixes,
            summary,
            mut lines,
        } = self;
        let outcome = watch.wait(&ids).await;
        let mut failures = Vec::new();
        let deleted_later = run.delete(&delete_later, &mut failures).await;
        run.fix_folders(&fixes, &mut failures).await;
        for failure in &failures {
            run.log(LogLevel::Error, failure.clone());
        }
        lines.extend(failures.iter().cloned());

        let deleted = summary.deleted + deleted_later;
        // Jobs that left the queue were done before completed ones were cleared.
        let copied = outcome.done + outcome.removed;
        let mut message = format!(
            "Synchronized {} with {}: {} copied",
            run.stored.local.root,
            run.stored.remote.root,
            plural(copied, "file")
        );
        if deleted > 0 {
            message.push_str(&format!(", {} deleted", plural(deleted, "item")));
        }
        if outcome.failed > 0 {
            message.push_str(&format!(", {} failed", plural(outcome.failed, "file")));
        }
        let level = if outcome.failed > 0 || !failures.is_empty() {
            LogLevel::Warn
        } else {
            LogLevel::Info
        };
        if !ids.is_empty() || deleted_later > 0 || !fixes.is_empty() {
            run.log(level, message.clone());
        }
        lines.push(message);
        if run.options().stats {
            let created = summary.created_folders + summary.created_links + summary.queued_files;
            for line in run.stats(outcome.done, outcome.bytes, created, deleted) {
                run.log(LogLevel::Info, line.clone());
                lines.push(line);
            }
        }
        write_log(&run, &lines).await;
        let mut all_failures = summary.failures;
        all_failures.extend(failures);
        let _ = report.send(Some(SyncReport {
            copied,
            failed: outcome.failed,
            bytes: outcome.bytes,
            deleted,
            failures: all_failures,
        }));
    }
}

/// Appends the run's lines to `--log-file`, stamped as rsync stamps them.
async fn write_log(run: &Run, lines: &[String]) {
    let path = run.options().log_file.trim().to_string();
    if path.is_empty() {
        return;
    }
    let lines = lines.to_vec();
    let written = tokio::task::spawn_blocking({
        let path = path.clone();
        move || -> std::io::Result<()> {
            use std::io::Write;
            let stamp = chrono::Local::now().format("%Y/%m/%d %H:%M:%S");
            let process = std::process::id();
            let mut file = std::io::BufWriter::new(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)?,
            );
            for line in lines {
                writeln!(file, "{stamp} [{process}] {line}")?;
            }
            file.flush()
        }
    })
    .await;
    if let Ok(Err(error)) = written {
        run.log(
            LogLevel::Warn,
            format!("Could not write the log file {path}: {error}"),
        );
    }
}

fn local_path(root: &str, relative: &str) -> String {
    if relative.is_empty() {
        return root.to_string();
    }
    super::hash::local_path(root, relative)
        .to_string_lossy()
        .into_owned()
}

fn system_time(seconds: i64) -> Option<SystemTime> {
    if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(seconds as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
    }
}

fn set_local_attributes(
    path: &Path,
    is_dir: bool,
    change: &Attributes,
    chmod: &Chmod,
) -> std::io::Result<()> {
    if let Some(time) = change.modified.and_then(system_time) {
        let opened = if is_dir {
            std::fs::File::open(path)
        } else {
            std::fs::OpenOptions::new().write(true).open(path)
        };
        match opened.and_then(|file| file.set_modified(time)) {
            // Windows opens folders only with flags the standard library does not pass.
            Err(_) if is_dir && cfg!(windows) => {}
            other => other?,
        }
    }
    set_local_mode_and_owner(path, is_dir, change, chmod)
}

#[cfg(unix)]
fn set_local_mode_and_owner(
    path: &Path,
    is_dir: bool,
    change: &Attributes,
    chmod: &Chmod,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = match change.permissions {
        Some(mode) => Some(mode),
        None if change.chmod_current => {
            let current = std::fs::metadata(path)?.permissions().mode() & 0o7777;
            Some(chmod.apply(current, is_dir))
        }
        None => None,
    };
    if let Some(mode) = mode {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    }
    let (uid, gid) = change.owner;
    if uid.is_some() || gid.is_some() {
        std::os::unix::fs::chown(path, uid, gid)?;
    }
    Ok(())
}

/// Windows files have no Unix permissions or owners to set.
#[cfg(not(unix))]
fn set_local_mode_and_owner(
    _path: &Path,
    _is_dir: bool,
    _change: &Attributes,
    _chmod: &Chmod,
) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn make_local_link(target: &str, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_local_link(target: &str, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target.replace('/', "\\"), link)
}

#[cfg(not(any(unix, windows)))]
fn make_local_link(_target: &str, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "links cannot be made here",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_options_follow_the_sync_options() {
        let options = SyncOptions {
            perms: true,
            inplace: true,
            whole_file: true,
            fsync: true,
            append: AppendMode::Verify,
            rsync_path: " sudo rsync ".into(),
            block_size: Some(4096),
            executability: true,
            remove_source_files: true,
            compress: true,
            bwlimit_kib: 100,
            ..SyncOptions::default()
        };
        let job = base_job_options(&options, "run");
        assert_eq!(job.preserve_timestamps, Some(true));
        assert_eq!(job.preserve_permissions, Some(true));
        assert_eq!(job.temporary_files, Some(false));
        assert_eq!(job.delta_transfers, Some(false));
        assert_eq!(job.flush_to_disk, Some(true));
        assert_eq!(job.verify_checksums, Some(true));
        assert_eq!(job.rsync_path.as_deref(), Some("sudo rsync"));
        assert_eq!(job.block_size, Some(4096));
        // -E only matters without -p.
        assert!(!job.executability);
        assert!(job.remove_source && job.compress);
        assert_eq!((job.rate_limit, job.limit_group.as_str()), (102_400, "run"));

        let defaults = base_job_options(&SyncOptions::default(), "run");
        assert_eq!(defaults.temporary_files, None);
        assert_eq!(defaults.delta_transfers, None);
        assert_eq!(defaults.rsync_path, None);
        assert_eq!(defaults.rate_limit, 0);
    }

    #[test]
    fn times_before_the_epoch() {
        assert_eq!(
            system_time(10),
            UNIX_EPOCH.checked_add(Duration::from_secs(10))
        );
        assert_eq!(
            system_time(-10),
            UNIX_EPOCH.checked_sub(Duration::from_secs(10))
        );
    }
}
