//! Finds files and folders anywhere below a folder, by name, kind, size and date.
//!
//! Two engines answer the same request. On an SFTP server that runs commands and has GNU
//! find, the server searches itself (`find`). Everywhere else, and on this computer, folders
//! are listed one after another (`walker`). What is found is reported while the search runs,
//! and a search can be stopped at any moment.

mod criteria;
mod find;
mod pattern;
mod walker;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

pub use criteria::{Criteria, KindFilter, DEFAULT_LIMIT, MAX_LIMIT};
pub use pattern::{NamePattern, PatternKind};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::file_ops::{Failure, Location};
use crate::model::FileEntry;
use crate::protocol::RemoteFileSystem;
use crate::remote_path;
use crate::session::SessionManager;
use walker::Folders;

/// Entries reported at once at most, so the page stays responsive while thousands arrive.
const REPORT_ENTRIES: usize = 500;
const REPORT_INTERVAL: Duration = Duration::from_millis(120);
/// Folders that could not be read are counted beyond this, but no longer listed.
const MAX_UNREADABLE_LISTED: u64 = 500;
const MAX_ID_LENGTH: usize = 64;
const MAX_RECHECKED_PATHS: usize = 5_000;
const MAX_REMEMBERED_SERVERS: usize = 256;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    /// Chosen by the caller, to stop the search with.
    pub search_id: String,
    pub location: Location,
    /// Where the search starts.
    pub folder: String,
    /// Empty finds every name.
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub pattern_kind: PatternKind,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub kind: KindFilter,
    #[serde(default)]
    pub min_size: Option<u64>,
    #[serde(default)]
    pub max_size: Option<u64>,
    #[serde(default)]
    pub modified_after: Option<i64>,
    #[serde(default)]
    pub modified_before: Option<i64>,
    #[serde(default)]
    pub max_depth: Option<u32>,
    /// Whether hidden folders are searched too.
    #[serde(default)]
    pub hidden: bool,
    /// Whether links to folders are entered. A link that leads back up is never entered twice.
    #[serde(default)]
    pub follow_links: bool,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// What a search has to say while it runs.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchReport {
    /// What was found since the last report.
    pub entries: Vec<FileEntry>,
    /// How many folders were looked at so far.
    pub folders: u64,
    /// The folder being looked at.
    pub current: String,
    /// Folders that could not be read since the last report.
    pub unreadable: Vec<Failure>,
    /// The start folder as its listing names it, once, when that differs from the folder asked
    /// for: a link to a folder, `/var` that is `/private/var`, or a short or differently cased
    /// name on Windows. Results are below this name, not the one asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
}

pub type SearchSink = Arc<dyn Fn(SearchReport) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    /// The server's own `find`.
    Find,
    /// Listing folder after folder.
    Walker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    /// Everything was looked at.
    Finished,
    /// Stopped by the user.
    Stopped,
    /// Stopped at the most results a search may bring.
    Capped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSummary {
    pub engine: Engine,
    pub outcome: Outcome,
    pub found: u64,
    pub folders: u64,
    /// How many folders could not be read, also those no report listed.
    pub unreadable: u64,
}

/// What became of results the page still shows.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recheck {
    /// The items still there, as their folders list them now.
    pub entries: Vec<FileEntry>,
    /// The paths nothing is at any more.
    pub missing: Vec<String>,
}

/// Which engines a search may use; tests pick one to check each against the same server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Engines {
    #[default]
    Automatic,
    WalkerOnly,
}

pub struct SearchManager {
    sessions: Arc<SessionManager>,
    engines: Engines,
    running: Mutex<HashMap<String, CancellationToken>>,
    /// Whether each session's server has a `find` that can be used, once asked.
    servers_with_find: Mutex<HashMap<String, bool>>,
}

impl SearchManager {
    pub fn new(sessions: Arc<SessionManager>) -> Self {
        Self::with_engines(sessions, Engines::Automatic)
    }

    pub fn with_engines(sessions: Arc<SessionManager>, engines: Engines) -> Self {
        Self {
            sessions,
            engines,
            running: Mutex::new(HashMap::new()),
            servers_with_find: Mutex::new(HashMap::new()),
        }
    }

    /// Searches until everything was looked at, the search is stopped with [`Self::cancel`],
    /// or the most results allowed were found. Finds are handed to `sink` as they come.
    pub async fn run(&self, request: SearchRequest, sink: SearchSink) -> AppResult<SearchSummary> {
        self.run_until(request, sink, CancellationToken::new())
            .await
    }

    /// The same, also stopping when `stop` is cancelled, such as when nobody is left to tell
    /// what is found.
    pub async fn run_until(
        &self,
        request: SearchRequest,
        sink: SearchSink,
        stop: CancellationToken,
    ) -> AppResult<SearchSummary> {
        let criteria = Criteria::from_request(&request)?;
        let running = self.begin(&request.search_id, &stop)?;
        let mut output = Output::new(&sink, criteria.limit);
        let engine = self
            .search(&request, &criteria, &running.cancel, &mut output)
            .await?;
        output.report();
        let outcome = if output.is_full() {
            Outcome::Capped
        } else if running.cancel.is_cancelled() {
            Outcome::Stopped
        } else {
            Outcome::Finished
        };
        Ok(SearchSummary {
            engine,
            outcome,
            found: output.found as u64,
            folders: output.folders,
            unreadable: output.unreadable_count,
        })
    }

    /// Stops a search; what it found so far stays reported.
    pub fn cancel(&self, search_id: &str) {
        if let Some(cancel) = self.running.lock().unwrap().get(search_id) {
            cancel.cancel();
        }
    }

    fn begin(&self, search_id: &str, stop: &CancellationToken) -> AppResult<Running<'_>> {
        if search_id.is_empty() || search_id.len() > MAX_ID_LENGTH {
            return Err(AppError::invalid("A search needs a short name of its own"));
        }
        let mut running = self.running.lock().unwrap();
        if running.contains_key(search_id) {
            return Err(AppError::invalid("That search is already running"));
        }
        let cancel = stop.child_token();
        running.insert(search_id.to_string(), cancel.clone());
        Ok(Running {
            manager: self,
            search_id: search_id.to_string(),
            cancel,
        })
    }

    async fn search(
        &self,
        request: &SearchRequest,
        criteria: &Criteria,
        cancel: &CancellationToken,
        output: &mut Output<'_>,
    ) -> AppResult<Engine> {
        let session_id = match &request.location {
            Location::Local => {
                walker::walk(&Folders::Local, &request.folder, criteria, cancel, output).await?;
                return Ok(Engine::Walker);
            }
            Location::Remote { session_id } => session_id,
        };
        let session = self.sessions.get(session_id).await?;

        // Only a plain SFTP session has a server to run `find` on; a vault's names are
        // encrypted there.
        if let Ok(fs) = session.sftp() {
            if self.engines == Engines::Automatic && self.has_find(&session).await {
                match find::search(&session, fs, &request.folder, criteria, cancel, output).await {
                    Ok(()) => return Ok(Engine::Find),
                    Err(find::Stop::Failed(error)) => return Err(error),
                    Err(find::Stop::Unusable) => {}
                }
            }
        }

        // A channel of its own on SFTP, so browsing is not held up by the search.
        let own_channel = match session.sftp() {
            Ok(_) => session.open_channel().await.ok().map(Arc::new),
            Err(_) => None,
        };
        let folders = Folders::Remote(match &own_channel {
            Some(channel) => channel.clone() as Arc<dyn RemoteFileSystem>,
            None => session.files(),
        });
        let walked = walker::walk(&folders, &request.folder, criteria, cancel, output).await;
        if let Some(channel) = own_channel {
            channel.close();
        }
        walked?;
        // A dropped connection can look like one unreadable folder after another.
        if output.unreadable_count > 0 && !self.sessions.is_live(session_id).await {
            return Err(AppError::new(
                ErrorKind::Disconnected,
                "The connection was lost during the search",
            ));
        }
        Ok(Engine::Walker)
    }

    async fn has_find(&self, session: &crate::session::Session) -> bool {
        if let Some(known) = self.servers_with_find.lock().unwrap().get(&session.id) {
            return *known;
        }
        let usable = find::is_usable(session).await;
        let mut known = self.servers_with_find.lock().unwrap();
        if known.len() >= MAX_REMEMBERED_SERVERS {
            known.clear();
        }
        known.insert(session.id.clone(), usable);
        usable
    }

    /// Looks at the folders holding `paths` again: results a search view still shows after
    /// items were renamed, moved or deleted from it.
    pub async fn recheck(&self, location: &Location, paths: Vec<String>) -> AppResult<Recheck> {
        if paths.len() > MAX_RECHECKED_PATHS {
            return Err(AppError::invalid("Too many items to look at again"));
        }
        if paths
            .iter()
            .any(|path| path.is_empty() || path.contains('\0'))
        {
            return Err(AppError::invalid("That is not a path"));
        }
        let folders = match location {
            Location::Local => Folders::Local,
            Location::Remote { session_id } => {
                Folders::Remote(self.sessions.get(session_id).await?.files())
            }
        };
        let mut by_folder: BTreeMap<String, HashSet<String>> = BTreeMap::new();
        for path in paths {
            if let Some(folder) = parent_of(location, &path) {
                by_folder.entry(folder).or_default().insert(path);
            }
        }
        let folders = &folders;
        let listed: Vec<AppResult<Recheck>> = stream::iter(by_folder)
            .map(|(folder, mut wanted)| async move {
                match folders.list(&folder).await {
                    Ok(listing) => {
                        let entries: Vec<FileEntry> = listing
                            .entries
                            .into_iter()
                            .filter(|entry| wanted.remove(&entry.path))
                            .collect();
                        Ok(Recheck {
                            entries,
                            missing: wanted.into_iter().collect(),
                        })
                    }
                    Err(error) if error.kind == ErrorKind::NotFound => Ok(Recheck {
                        entries: Vec::new(),
                        missing: wanted.into_iter().collect(),
                    }),
                    Err(error) if error.is_connection_lost() => Err(error),
                    // No telling: the results stay as they are.
                    Err(_) => Ok(Recheck::default()),
                }
            })
            .buffered(folders.parallel_listings())
            .collect()
            .await;
        let mut recheck = Recheck::default();
        for folder in listed {
            let folder = folder?;
            recheck.entries.extend(folder.entries);
            recheck.missing.extend(folder.missing);
        }
        recheck.missing.sort();
        Ok(recheck)
    }
}

fn parent_of(location: &Location, path: &str) -> Option<String> {
    match location {
        Location::Local => Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .filter(|parent| !parent.is_empty()),
        Location::Remote { .. } => remote_path::parent(path),
    }
}

/// Forgets the search when it ends, however it ends.
struct Running<'a> {
    manager: &'a SearchManager,
    search_id: String,
    cancel: CancellationToken,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.manager.running.lock().unwrap().remove(&self.search_id);
    }
}

/// Collects what an engine finds and reports it in batches.
pub(crate) struct Output<'a> {
    sink: &'a SearchSink,
    limit: usize,
    pending: SearchReport,
    found: usize,
    folders: u64,
    unreadable_count: u64,
    /// Whether there is anything to say since the last report.
    changed: bool,
    last_report: Instant,
}

impl<'a> Output<'a> {
    fn new(sink: &'a SearchSink, limit: usize) -> Self {
        Self {
            sink,
            limit,
            pending: SearchReport::default(),
            found: 0,
            folders: 0,
            unreadable_count: 0,
            changed: false,
            last_report: Instant::now(),
        }
    }

    /// Takes a find, unless the search already has as many as it may.
    fn offer(&mut self, entry: FileEntry) {
        if self.is_full() {
            return;
        }
        self.found += 1;
        self.pending.entries.push(entry);
        self.changed = true;
        if self.pending.entries.len() >= REPORT_ENTRIES {
            self.report();
        }
    }

    fn is_full(&self) -> bool {
        self.found >= self.limit
    }

    /// Notes that a folder is being looked at.
    fn visit(&mut self, folder: &str) {
        self.folders += 1;
        self.pending.current.clear();
        self.pending.current.push_str(folder);
        self.changed = true;
    }

    /// Notes the name the start folder's listing gives it, when not the one asked for.
    fn started_in(&mut self, folder: &str) {
        self.pending.start = Some(folder.to_string());
        self.changed = true;
    }

    fn unreadable(&mut self, failure: Failure) {
        self.unreadable_count += 1;
        if self.unreadable_count <= MAX_UNREADABLE_LISTED {
            self.pending.unreadable.push(failure);
        }
        self.changed = true;
    }

    fn report_if_due(&mut self) {
        if self.last_report.elapsed() >= REPORT_INTERVAL {
            self.report();
        }
    }

    fn report(&mut self) {
        if !self.changed {
            return;
        }
        let report = SearchReport {
            entries: std::mem::take(&mut self.pending.entries),
            folders: self.folders,
            current: self.pending.current.clone(),
            unreadable: std::mem::take(&mut self.pending.unreadable),
            start: self.pending.start.take(),
        };
        (self.sink)(report);
        self.changed = false;
        self.last_report = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EntryKind;
    use criteria::tests::entry;

    fn collecting_sink() -> (SearchSink, Arc<Mutex<Vec<SearchReport>>>) {
        let reports = Arc::new(Mutex::new(Vec::new()));
        let sink_reports = reports.clone();
        let sink: SearchSink = Arc::new(move |report| sink_reports.lock().unwrap().push(report));
        (sink, reports)
    }

    #[test]
    fn finds_are_reported_in_batches_up_to_the_limit() {
        let (sink, reports) = collecting_sink();
        let mut output = Output::new(&sink, REPORT_ENTRIES + 10);
        output.visit("/srv");
        for index in 0..REPORT_ENTRIES + 50 {
            output.offer(entry(&format!("file-{index}"), EntryKind::File, 1, None));
        }
        assert!(output.is_full());
        output.report();
        output.report();
        let reports = reports.lock().unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].entries.len(), REPORT_ENTRIES);
        assert_eq!(reports[1].entries.len(), 10);
        assert_eq!(reports[1].folders, 1);
        assert_eq!(reports[1].current, "/srv");
    }

    #[test]
    fn unreadable_folders_are_listed_up_to_a_point_and_counted_beyond() {
        let (sink, reports) = collecting_sink();
        let mut output = Output::new(&sink, 10);
        for index in 0..MAX_UNREADABLE_LISTED + 5 {
            output.unreadable(Failure {
                path: format!("/srv/{index}"),
                message: "Permission denied".to_string(),
            });
        }
        output.report();
        assert_eq!(output.unreadable_count, MAX_UNREADABLE_LISTED + 5);
        let listed: usize = reports
            .lock()
            .unwrap()
            .iter()
            .map(|report| report.unreadable.len())
            .sum();
        assert_eq!(listed as u64, MAX_UNREADABLE_LISTED);
    }

    #[test]
    fn a_search_can_be_stopped_only_while_it_runs() {
        let manager = SearchManager::new(Arc::new(SessionManager::new(
            std::env::temp_dir().join("poros-search-test-known-hosts"),
            crate::events::Events::default(),
        )));
        let stop = CancellationToken::new();
        let running = manager.begin("one", &stop).unwrap();
        assert!(manager.begin("one", &stop).is_err());
        assert!(manager.begin("", &stop).is_err());
        manager.cancel("one");
        assert!(running.cancel.is_cancelled());
        drop(running);
        manager.cancel("one");
        let again = manager.begin("one", &stop).unwrap();
        assert!(!again.cancel.is_cancelled());
        // Nobody left to tell what is found stops it too.
        stop.cancel();
        assert!(again.cancel.is_cancelled());
    }

    #[test]
    fn parents_follow_the_kind_of_path() {
        let remote = Location::Remote {
            session_id: "session".to_string(),
        };
        assert_eq!(
            parent_of(&remote, "/srv/a/b.txt").as_deref(),
            Some("/srv/a")
        );
        assert_eq!(parent_of(&remote, "/"), None);
        let local = std::env::temp_dir().join("file.txt");
        let parent = parent_of(&Location::Local, &local.to_string_lossy()).unwrap();
        assert_eq!(Path::new(&parent), std::env::temp_dir());
    }
}
