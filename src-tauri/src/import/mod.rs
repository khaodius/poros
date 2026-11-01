//! Saved sites from other file transfer and SSH clients: FileZilla, WinSCP, SmartFTP, Cyberduck
//! and Mountain Duck, PuTTY and KiTTY, MobaXterm, the OpenSSH client config, and the server
//! bookmarks of GNOME Files and KDE Dolphin. A scan reads them into entries the user picks from.
//! The passwords some of them carry stay in this process until the import, which hands them to
//! the system keychain; they never travel to the page. Mirrored in `src/lib/types.ts`.

mod cyberduck;
mod filezilla;
mod locations;
mod mobaxterm;
mod openssh;
mod putty;
mod registry;
mod smartftp;
mod urls;
mod winscp;
mod xml;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::cloud::s3::S3Options;
use crate::connections::{AuthType, ConnectionStore, SavedConnection};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol::Protocol;

pub use locations::{source_infos, SourceInfo};

/// Files bigger than this are not site lists.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// How many files a folder scan reads at most, and how deep it goes.
const MAX_FOLDER_FILES: usize = 5000;
const MAX_FOLDER_DEPTH: usize = 8;
/// Shown between folder names and the site name when folders go into connection names.
const FOLDER_SEPARATOR: &str = " / ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportSource {
    FileZilla,
    WinScp,
    SmartFtp,
    /// Cyberduck and Mountain Duck bookmarks (`.duck` files).
    Cyberduck,
    /// PuTTY, KiTTY and other PuTTY-derived clients.
    Putty,
    MobaXterm,
    /// `~/.ssh/config`.
    OpenSsh,
    /// Server bookmarks of GNOME Files (GTK bookmarks) and KDE Dolphin (places).
    FileManager,
    /// A plain list of `sftp://` and `ftp://` addresses, as SmartFTP exports its favorites.
    UrlList,
}

/// A site as read from another client, before it is saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Without an id; the name has no folder in it.
    pub connection: SavedConnection,
    /// The folders the site was in, outermost first.
    pub folder: Vec<String>,
    pub secret: Option<String>,
    /// Another candidate this one is reached through.
    pub jump: Option<usize>,
    /// What did not carry over, for the user.
    pub notes: Vec<String>,
}

impl Candidate {
    /// A password login to `host`; an empty name becomes the host, a missing port the
    /// protocol's own.
    pub fn new(
        protocol: Protocol,
        name: &str,
        host: &str,
        port: Option<u16>,
        username: &str,
    ) -> Self {
        let host = host.trim().to_string();
        let name = match name.trim() {
            "" => host.clone(),
            name => name.to_string(),
        };
        Self {
            connection: SavedConnection {
                id: String::new(),
                protocol,
                name,
                host,
                port: port
                    .filter(|port| *port != 0)
                    .unwrap_or_else(|| protocol.default_port()),
                username: username.trim().to_string(),
                auth_type: AuthType::Password,
                key_path: None,
                remote_path: None,
                save_secret: false,
                last_used: None,
                ftp_active: false,
                bypass_proxy: false,
                jump_connection_id: None,
                rsync_path: None,
                sync_arguments: None,
                s3: S3Options::default(),
            },
            folder: Vec::new(),
            secret: None,
            jump: None,
            notes: Vec::new(),
        }
    }

    /// Signs in with the private key at `path`, or keeps the current method for an empty one.
    pub fn use_key_file(&mut self, path: &str) {
        let path = path.trim();
        if !path.is_empty() {
            self.connection.auth_type = AuthType::PublicKey;
            self.connection.key_path = Some(path.to_string());
        }
    }

    /// The folder to open first. A WebDAV server's files are often only below a path, such
    /// as Nextcloud's `/remote.php/dav`, so for WebDAV it goes into the address, which makes it
    /// the top folder.
    pub fn set_remote_path(&mut self, path: &str) {
        let path = path.trim();
        if path.is_empty() || path == "/" {
            self.connection.remote_path = None;
        } else if self.connection.protocol.is_webdav() && !self.connection.host.contains('/') {
            self.connection.host = format!(
                "{}/{}",
                self.connection.host.trim_end_matches('/'),
                path.trim_matches('/')
            );
        } else {
            self.connection.remote_path = Some(path.to_string());
        }
    }

    pub fn set_secret(&mut self, secret: Option<String>) {
        self.secret = secret.filter(|secret| !secret.is_empty());
    }

    pub fn note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    /// The same server, account and sign-in.
    fn same_login(&self, other: &Candidate) -> bool {
        same_server(&self.connection, &other.connection)
            && self.connection.auth_type == other.connection.auth_type
            && self.connection.key_path == other.connection.key_path
    }
}

/// The same server and account, whatever the names.
fn same_server(first: &SavedConnection, second: &SavedConnection) -> bool {
    first.protocol == second.protocol
        && first.host.trim().eq_ignore_ascii_case(second.host.trim())
        && first.port == second.port
        && first.username.trim() == second.username.trim()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub candidates: Vec<Candidate>,
    pub skipped: Vec<Skipped>,
    pub warnings: Vec<String>,
}

impl Parsed {
    pub fn skip(&mut self, name: &str, reason: impl Into<String>) {
        self.skipped.push(Skipped {
            name: name.trim().to_string(),
            reason: reason.into(),
        });
    }

    pub fn warn(&mut self, warning: impl Into<String>) {
        let warning = warning.into();
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    /// The candidate for a jump host: one already there with the same login and route, or
    /// `candidate` added.
    pub fn jump_host(&mut self, candidate: Candidate) -> usize {
        let existing = self.candidates.iter().position(|existing| {
            existing.same_login(&candidate) && existing.jump == candidate.jump
        });
        existing.unwrap_or_else(|| {
            self.candidates.push(candidate);
            self.candidates.len() - 1
        })
    }

    /// Whether following jump hosts from `from` leads to `target`.
    pub fn reaches(&self, from: usize, target: usize) -> bool {
        let mut current = Some(from);
        for _ in 0..=self.candidates.len() {
            match current {
                Some(index) if index == target => return true,
                Some(index) => current = self.candidates[index].jump,
                None => return false,
            }
        }
        true
    }

    /// Reaches `index` through `jump`, unless that would go round in a circle.
    pub fn link_jump(&mut self, index: usize, jump: usize) -> bool {
        if self.reaches(jump, index) {
            return false;
        }
        self.candidates[index].jump = Some(jump);
        true
    }

    fn is_empty(&self) -> bool {
        self.candidates.is_empty() && self.skipped.is_empty()
    }

    fn extend(&mut self, other: Parsed) {
        let offset = self.candidates.len();
        self.candidates
            .extend(other.candidates.into_iter().map(|mut candidate| {
                candidate.jump = candidate.jump.map(|jump| jump + offset);
                candidate
            }));
        self.skipped.extend(other.skipped);
        for warning in other.warnings {
            self.warn(warning);
        }
    }
}

/// One file or registry export to read.
#[derive(Debug, Clone)]
pub struct Input {
    /// Where it came from, for the user.
    pub origin: String,
    pub path: Option<PathBuf>,
    pub text: String,
    /// The folders it was found in below the folder that was scanned.
    pub folder: Vec<String>,
}

impl Input {
    pub fn from_text(origin: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            path: None,
            text: text.into(),
            folder: Vec::new(),
        }
    }

    /// The file's name without its extension.
    pub fn stem(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_stem)
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn file_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    }
}

/// Reads `path`, or every file inside it that `wanted` accepts when it is a folder.
pub fn read_inputs(path: &Path, wanted: &dyn Fn(&Path) -> bool) -> AppResult<Vec<Input>> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| AppError::from(error).with_path(path.display().to_string()))?;
    if !metadata.is_dir() {
        return Ok(vec![read_file(path, Vec::new())?]);
    }
    let mut inputs = Vec::new();
    walk(path, &mut Vec::new(), wanted, &mut inputs)?;
    Ok(inputs)
}

fn walk(
    folder: &Path,
    names: &mut Vec<String>,
    wanted: &dyn Fn(&Path) -> bool,
    inputs: &mut Vec<Input>,
) -> AppResult<()> {
    let mut entries: Vec<_> = std::fs::read_dir(folder)
        .map_err(|error| AppError::from(error).with_path(folder.display().to_string()))?
        .flatten()
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if inputs.len() >= MAX_FOLDER_FILES {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if names.len() < MAX_FOLDER_DEPTH {
                names.push(entry.file_name().to_string_lossy().into_owned());
                walk(&path, names, wanted, inputs)?;
                names.pop();
            }
        } else if wanted(&path) {
            if let Ok(input) = read_file(&path, names.clone()) {
                inputs.push(input);
            }
        }
    }
    Ok(())
}

fn read_file(path: &Path, folder: Vec<String>) -> AppResult<Input> {
    let with_path =
        |error: std::io::Error| AppError::from(error).with_path(path.display().to_string());
    let size = std::fs::metadata(path).map_err(with_path)?.len();
    if size > MAX_FILE_BYTES {
        return Err(AppError::invalid(format!(
            "{} is too big to be a list of sites",
            path.display()
        )));
    }
    let bytes = std::fs::read(path).map_err(with_path)?;
    let text = crate::text::decode(&bytes)
        .map_err(|_| {
            AppError::invalid(format!(
                "{} is not a text file Poros can import from",
                path.display()
            ))
        })?
        .text;
    Ok(Input {
        origin: path.display().to_string(),
        path: Some(path.to_path_buf()),
        text,
        folder,
    })
}

/// Which client wrote `input`, from what is in it.
pub fn detect(input: &Input) -> Option<ImportSource> {
    let text = input.text.trim_start_matches('\u{feff}').trim_start();
    let file_name = input.file_name();
    if text.starts_with('<') {
        let lower = text.to_ascii_lowercase();
        return if lower.contains("<filezilla3") {
            Some(ImportSource::FileZilla)
        } else if lower.contains("<plist") {
            Some(ImportSource::Cyberduck)
        } else if lower.contains("<xbel") {
            Some(ImportSource::FileManager)
        } else if lower.contains("<host>") {
            Some(ImportSource::SmartFtp)
        } else {
            None
        };
    }
    if registry::is_export(text) {
        return if text.contains("\\WinSCP 2\\Sessions\\") {
            Some(ImportSource::WinScp)
        } else if text.contains("\\Sessions\\") {
            Some(ImportSource::Putty)
        } else {
            None
        };
    }
    if text.contains("[Sessions\\") {
        return Some(ImportSource::WinScp);
    }
    if text.contains("[Bookmarks") || file_name.ends_with(".mxtsessions") {
        return Some(ImportSource::MobaXterm);
    }
    if file_name == "bookmarks" && text.contains("://") {
        return Some(ImportSource::FileManager);
    }
    if openssh::looks_like_config(text) {
        return Some(ImportSource::OpenSsh);
    }
    if putty::looks_like_session_file(text) {
        return Some(ImportSource::Putty);
    }
    if text.contains("://") {
        return Some(ImportSource::UrlList);
    }
    None
}

fn parse_input(source: ImportSource, input: &Input) -> AppResult<Parsed> {
    let mut parsed = match source {
        ImportSource::FileZilla => filezilla::parse(&input.text)?,
        ImportSource::WinScp => winscp::parse(&input.text),
        ImportSource::SmartFtp => smartftp::parse(input)?,
        ImportSource::Cyberduck => cyberduck::parse(input)?,
        ImportSource::Putty => putty::parse(input),
        ImportSource::MobaXterm => mobaxterm::parse(&input.text),
        ImportSource::OpenSsh => openssh::parse(input),
        ImportSource::FileManager | ImportSource::UrlList => urls::parse(&input.text)?,
    };
    if !input.folder.is_empty() {
        for candidate in &mut parsed.candidates {
            let mut folder = input.folder.clone();
            folder.append(&mut candidate.folder);
            candidate.folder = folder;
        }
    }
    Ok(parsed)
}

/// What one scan found.
pub struct Scanned {
    pub source: ImportSource,
    pub origins: Vec<String>,
    pub parsed: Parsed,
}

/// Reads `source` from where its client keeps it on this computer, or from `paths`, which may
/// be files or folders. Without a source, each file's contents say which client wrote it.
pub fn scan(source: Option<ImportSource>, paths: &[PathBuf]) -> AppResult<Scanned> {
    let inputs = if paths.is_empty() {
        let source =
            source.ok_or_else(|| AppError::invalid("Choose a client or a file to import from"))?;
        locations::read(source)?
    } else {
        let mut inputs = Vec::new();
        for path in paths {
            inputs.extend(read_inputs(path, &|file| wanted_file(source, file))?);
        }
        inputs
    };
    if inputs.is_empty() {
        return Err(AppError::invalid("There was nothing to import there"));
    }

    let mut parsed = Parsed::default();
    let mut detected = None;
    let mut origins = Vec::new();
    let mut unknown = Vec::new();
    for input in &inputs {
        let Some(input_source) = source.or_else(|| detect(input)) else {
            unknown.push(input.origin.clone());
            continue;
        };
        let (input_source, result) = match (parse_input(input_source, input), detect(input)) {
            // A file chosen for one app that another app wrote.
            (Ok(found), Some(other)) if found.is_empty() && other != input_source => {
                (other, parse_input(other, input))
            }
            (Err(_), Some(other)) if other != input_source => (other, parse_input(other, input)),
            (result, _) => (input_source, result),
        };
        detected.get_or_insert(input_source);
        match result {
            Ok(found) => parsed.extend(found),
            Err(error) if inputs.len() == 1 => return Err(error),
            Err(error) => parsed.warn(format!("{}: {}", input.origin, error.message)),
        }
        if !origins.contains(&input.origin) {
            origins.push(input.origin.clone());
        }
    }
    let Some(source) = detected else {
        return Err(AppError::invalid(match unknown.as_slice() {
            [one] => format!("Poros does not recognize the sites in {one}"),
            _ => "Poros does not recognize the sites in these files".to_string(),
        }));
    };
    for origin in unknown {
        parsed.warn(format!(
            "{origin} was left out: Poros does not recognize it"
        ));
    }
    Ok(Scanned {
        source,
        origins,
        parsed,
    })
}

/// The files of a folder that hold sites of `source`.
fn wanted_file(source: Option<ImportSource>, path: &Path) -> bool {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match source {
        Some(ImportSource::Cyberduck) => extension == "duck",
        Some(ImportSource::SmartFtp) => extension == "xml" || extension == "txt",
        // PuTTY's own session files on Linux are named after the session, dots and all.
        Some(ImportSource::Putty) => true,
        _ => matches!(
            extension.as_str(),
            "xml" | "ini" | "reg" | "duck" | "mxtsessions" | "txt" | "xbel" | ""
        ),
    }
}

/// An entry of a scan, as the page shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportEntry {
    pub connection: SavedConnection,
    pub folder: Vec<String>,
    /// A password or secret key came with it.
    pub has_password: bool,
    /// The entry this one is reached through.
    pub jump: Option<usize>,
    /// A saved connection to the same server and account.
    pub existing_id: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportScan {
    pub scan_id: String,
    pub source: ImportSource,
    pub origins: Vec<String>,
    pub entries: Vec<ImportEntry>,
    pub skipped: Vec<Skipped>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub scan_id: String,
    /// Entry indexes; the jump hosts they go through come along.
    pub selected: Vec<usize>,
    /// Keeps the passwords that came with the sites in the system keychain.
    pub save_passwords: bool,
    /// Puts the folders a site was in before its name.
    pub folder_names: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub imported: usize,
    /// Imported without the password that came with them, since the keychain refused it.
    pub without_passwords: usize,
    pub failures: Vec<String>,
}

/// Scans waiting for the user to choose what to import, with the passwords that came along.
#[derive(Default)]
pub struct ImportScans(Mutex<HashMap<String, Vec<Candidate>>>);

impl ImportScans {
    pub fn keep(&self, scanned: Scanned, existing: &[SavedConnection]) -> ImportScan {
        let scan_id = uuid::Uuid::new_v4().to_string();
        let entries = scanned
            .parsed
            .candidates
            .iter()
            .map(|candidate| ImportEntry {
                connection: candidate.connection.clone(),
                folder: candidate.folder.clone(),
                has_password: candidate.secret.is_some(),
                jump: candidate.jump,
                existing_id: existing
                    .iter()
                    .find(|saved| same_server(saved, &candidate.connection))
                    .map(|saved| saved.id.clone()),
                notes: candidate.notes.clone(),
            })
            .collect();
        let mut scans = self.0.lock().unwrap();
        // Only the dialog's latest scan is ever imported.
        scans.clear();
        scans.insert(scan_id.clone(), scanned.parsed.candidates);
        ImportScan {
            scan_id,
            source: scanned.source,
            origins: scanned.origins,
            entries,
            skipped: scanned.parsed.skipped,
            warnings: scanned.parsed.warnings,
        }
    }

    pub fn discard(&self, scan_id: &str) {
        self.0.lock().unwrap().remove(scan_id);
    }

    pub fn import(
        &self,
        request: &ImportRequest,
        store: &ConnectionStore,
    ) -> AppResult<ImportOutcome> {
        let candidates = self
            .0
            .lock()
            .unwrap()
            .remove(&request.scan_id)
            .ok_or_else(|| AppError::invalid("Scan the sites again; this list is out of date"))?;
        let existing = store.list()?;
        let plan = plan_import(&candidates, request, &existing);
        let total = plan.len();
        let connections: Vec<SavedConnection> = plan
            .iter()
            .map(|(connection, _)| connection.clone())
            .collect();
        let results = store.save_all(plan)?;
        let mut failures = Vec::new();
        let mut keychain_refused = Vec::new();
        for (connection, result) in connections.into_iter().zip(results) {
            match result {
                Ok(_) => {}
                Err(error) if error.kind == ErrorKind::Keychain => keychain_refused.push((
                    SavedConnection {
                        save_secret: false,
                        ..connection
                    },
                    None,
                )),
                Err(error) => failures.push(error.message),
            }
        }
        // Without the keychain the sites are still worth having; Poros asks for the password.
        let mut without_passwords = 0;
        if !keychain_refused.is_empty() {
            for result in store.save_all(keychain_refused)? {
                match result {
                    Ok(_) => without_passwords += 1,
                    Err(error) => failures.push(error.message),
                }
            }
        }
        Ok(ImportOutcome {
            imported: total - failures.len(),
            without_passwords,
            failures,
        })
    }
}

/// The connections to save, jump hosts before the sites that go through them, with the ids
/// that link them.
fn plan_import(
    candidates: &[Candidate],
    request: &ImportRequest,
    existing: &[SavedConnection],
) -> Vec<(SavedConnection, Option<String>)> {
    let selected: HashSet<usize> = request
        .selected
        .iter()
        .copied()
        .filter(|index| *index < candidates.len())
        .collect();
    let mut ids: HashMap<usize, String> = HashMap::new();
    let mut plan = Vec::new();
    let mut order: Vec<usize> = selected.iter().copied().collect();
    order.sort_unstable();
    for index in order {
        add_to_plan(
            index, candidates, request, &selected, existing, &mut ids, &mut plan, 0,
        );
    }
    plan
}

#[allow(clippy::too_many_arguments)]
fn add_to_plan(
    index: usize,
    candidates: &[Candidate],
    request: &ImportRequest,
    selected: &HashSet<usize>,
    existing: &[SavedConnection],
    ids: &mut HashMap<usize, String>,
    plan: &mut Vec<(SavedConnection, Option<String>)>,
    depth: usize,
) -> Option<String> {
    if let Some(id) = ids.get(&index) {
        return Some(id.clone());
    }
    let candidate = &candidates[index];
    // A jump host the user left out is used as already saved, when it is.
    if depth > 0 && !selected.contains(&index) {
        if let Some(saved) = existing
            .iter()
            .find(|saved| same_server(saved, &candidate.connection))
        {
            ids.insert(index, saved.id.clone());
            return Some(saved.id.clone());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    ids.insert(index, id.clone());
    let jump_connection_id = candidate.jump.and_then(|jump| {
        add_to_plan(
            jump,
            candidates,
            request,
            selected,
            existing,
            ids,
            plan,
            depth + 1,
        )
    });
    let mut connection = candidate.connection.clone();
    connection.id = id.clone();
    connection.jump_connection_id = jump_connection_id;
    if request.folder_names && !candidate.folder.is_empty() {
        connection.name = format!(
            "{}{FOLDER_SEPARATOR}{}",
            candidate.folder.join(FOLDER_SEPARATOR),
            connection.name
        );
    }
    let secret = candidate
        .secret
        .clone()
        .filter(|_| request.save_passwords && connection.auth_type != AuthType::Agent);
    connection.save_secret = secret.is_some();
    plan.push((connection, secret));
    Some(id)
}

/// `%XX` escapes decoded as bytes; the bytes read as UTF-8, or else one character each.
pub fn percent_decode(text: &str) -> String {
    let bytes: Vec<u8> = percent_encoding::percent_decode_str(text).collect();
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    match std::str::from_utf8(bytes) {
        Ok(decoded) => decoded.to_string(),
        Err(_) => bytes.iter().map(|&byte| char::from(byte)).collect(),
    }
}

pub fn parse_port(text: &str) -> Option<u16> {
    text.trim().parse::<u16>().ok().filter(|port| *port != 0)
}

/// `user@host` split into its parts; the user is empty without an `@`.
pub fn split_user_host(text: &str) -> (&str, &str) {
    match text.rsplit_once('@') {
        Some((user, host)) => (user, host),
        None => ("", text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::tests::MemorySecrets;

    fn candidate(host: &str) -> Candidate {
        Candidate::new(Protocol::Sftp, "", host, None, "alice")
    }

    #[test]
    fn a_new_candidate_takes_the_host_and_default_port() {
        let site = Candidate::new(
            Protocol::FtpsImplicit,
            " ",
            " ftp.example.com ",
            None,
            "bob",
        );
        assert_eq!(site.connection.name, "ftp.example.com");
        assert_eq!(site.connection.port, 990);
        let site = Candidate::new(Protocol::Sftp, "x", "h", Some(0), "");
        assert_eq!(site.connection.port, 22);
    }

    #[test]
    fn jump_hosts_are_shared_and_never_circular() {
        let mut parsed = Parsed::default();
        parsed.candidates.push(candidate("target"));
        let bastion = parsed.jump_host(candidate("bastion"));
        assert_eq!(parsed.jump_host(candidate("bastion")), bastion);
        assert!(parsed.link_jump(0, bastion));
        assert!(!parsed.link_jump(bastion, 0));
        assert_eq!(parsed.candidates[bastion].jump, None);
    }

    #[test]
    fn imports_selected_sites_with_their_jump_hosts_and_passwords() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(
            temp_dir.path().join("connections.json"),
            Box::<MemorySecrets>::default(),
        );
        let already = store
            .save(candidate("saved.example").connection, None)
            .unwrap();

        let mut parsed = Parsed::default();
        let mut web = candidate("web");
        web.folder = vec!["Work".into(), "Acme".into()];
        web.set_secret(Some("hunter2".into()));
        parsed.candidates.push(web);
        let bastion = parsed.jump_host(candidate("bastion"));
        parsed.link_jump(0, bastion);
        parsed.candidates.push(candidate("left-out"));
        let saved_jump = parsed.jump_host(candidate("saved.example"));
        let mut through_saved = candidate("inner");
        through_saved.jump = Some(saved_jump);
        parsed.candidates.push(through_saved);

        let scans = ImportScans::default();
        let scan = scans.keep(
            Scanned {
                source: ImportSource::FileZilla,
                origins: vec![],
                parsed,
            },
            &store.list().unwrap(),
        );
        assert!(scan.entries[0].has_password);
        assert_eq!(
            scan.entries[saved_jump].existing_id.as_deref(),
            Some(already.id.as_str())
        );

        let request = ImportRequest {
            scan_id: scan.scan_id.clone(),
            selected: vec![0, 4],
            save_passwords: true,
            folder_names: true,
        };
        let outcome = scans.import(&request, &store).unwrap();
        assert_eq!(outcome.imported, 3);
        assert!(outcome.failures.is_empty());

        let saved = store.list().unwrap();
        let find = |host: &str| saved.iter().find(|saved| saved.host == host).unwrap();
        let web = find("web");
        assert_eq!(web.name, "Work / Acme / web");
        assert!(web.save_secret);
        assert_eq!(store.secret(&web.id).unwrap().as_deref(), Some("hunter2"));
        assert_eq!(
            web.jump_connection_id.as_deref(),
            Some(find("bastion").id.as_str())
        );
        assert_eq!(
            find("inner").jump_connection_id.as_deref(),
            Some(already.id.as_str())
        );
        assert!(saved.iter().all(|saved| saved.host != "left-out"));
        assert_eq!(saved.len(), 4);

        // A scan is imported once.
        assert!(scans.import(&request, &store).is_err());
    }

    /// A keychain that cannot be reached, as on a Linux desktop without a secret service.
    struct NoKeychain;

    impl crate::connections::SecretStore for NoKeychain {
        fn get(&self, _account: &str) -> AppResult<Option<String>> {
            Ok(None)
        }
        fn set(&self, _account: &str, _secret: &str) -> AppResult<()> {
            Err(AppError::new(ErrorKind::Keychain, "no keychain"))
        }
        fn delete(&self, _account: &str) -> AppResult<()> {
            Err(AppError::new(ErrorKind::Keychain, "no keychain"))
        }
    }

    #[test]
    fn imports_sites_without_their_passwords_when_the_keychain_is_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(
            temp_dir.path().join("connections.json"),
            Box::new(NoKeychain),
        );
        let mut parsed = Parsed::default();
        let mut web = candidate("web");
        web.set_secret(Some("hunter2".into()));
        parsed.candidates.push(web);
        parsed.candidates.push(candidate("db"));
        let scans = ImportScans::default();
        let scan = scans.keep(
            Scanned {
                source: ImportSource::WinScp,
                origins: vec![],
                parsed,
            },
            &[],
        );
        let outcome = scans
            .import(
                &ImportRequest {
                    scan_id: scan.scan_id,
                    selected: vec![0, 1],
                    save_passwords: true,
                    folder_names: false,
                },
                &store,
            )
            .unwrap();
        assert_eq!((outcome.imported, outcome.without_passwords), (2, 1));
        assert!(outcome.failures.is_empty());
        let saved = store.list().unwrap();
        assert_eq!(saved.len(), 2);
        assert!(saved.iter().all(|connection| !connection.save_secret));
    }

    #[test]
    fn passwords_stay_behind_when_the_user_says_so() {
        let mut site = candidate("web");
        site.set_secret(Some("hunter2".into()));
        let request = ImportRequest {
            scan_id: String::new(),
            selected: vec![0],
            save_passwords: false,
            folder_names: false,
        };
        let plan = plan_import(&[site], &request, &[]);
        assert_eq!(plan.len(), 1);
        assert!(!plan[0].0.save_secret);
        assert_eq!(plan[0].1, None);
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("My%20Site"), "My Site");
        assert_eq!(percent_decode("C:%5Ckeys%5Cid.ppk"), "C:\\keys\\id.ppk");
        assert_eq!(percent_decode("%EF%BB%BFJos%C3%A9"), "José");
        assert_eq!(percent_decode("Jos%E9"), "José");
    }

    #[test]
    fn reads_a_file_chosen_for_the_wrong_app_as_what_it_is() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("sites.xml");
        std::fs::write(
            &file,
            "<FileZilla3><Servers><Server><Host>h</Host><Protocol>1</Protocol></Server></Servers></FileZilla3>",
        )
        .unwrap();
        let scanned = scan(Some(ImportSource::WinScp), &[file]).unwrap();
        assert_eq!(scanned.source, ImportSource::FileZilla);
        assert_eq!(scanned.parsed.candidates.len(), 1);

        let unknown = folder.path().join("notes.txt");
        std::fs::write(&unknown, "nothing to see").unwrap();
        assert!(scan(None, &[unknown]).is_err());
    }

    #[test]
    fn detects_formats_from_their_contents() {
        let detect_text = |text: &str| detect(&Input::from_text("t", text));
        assert_eq!(
            detect_text("<?xml version=\"1.0\"?>\n<FileZilla3><Servers/></FileZilla3>"),
            Some(ImportSource::FileZilla)
        );
        assert_eq!(
            detect_text("<?xml?><plist version=\"1.0\"><dict/></plist>"),
            Some(ImportSource::Cyberduck)
        );
        assert_eq!(
            detect_text("Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\Software\\Martin Prikryl\\WinSCP 2\\Sessions\\a]"),
            Some(ImportSource::WinScp)
        );
        assert_eq!(
            detect_text("Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\a]"),
            Some(ImportSource::Putty)
        );
        assert_eq!(
            detect_text("[Sessions\\a]\nHostName=x"),
            Some(ImportSource::WinScp)
        );
        assert_eq!(
            detect_text("[Bookmarks]\nSubRep=\nImgNum=42\n"),
            Some(ImportSource::MobaXterm)
        );
        assert_eq!(
            detect_text("# mine\nHost web\n  HostName example.com\n"),
            Some(ImportSource::OpenSsh)
        );
        assert_eq!(
            detect_text("HostName=example.com\nProtocol=ssh\nPortNumber=22\n"),
            Some(ImportSource::Putty)
        );
        assert_eq!(
            detect_text("sftp://alice@example.com/\n"),
            Some(ImportSource::UrlList)
        );
        assert_eq!(detect_text("nothing here"), None);
    }
}
