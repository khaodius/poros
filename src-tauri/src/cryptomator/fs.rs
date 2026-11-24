//! An unlocked vault as a file system: the decrypted tree, served through any server's own
//! `RemoteFileSystem`. Paths are the server's: the vault's folder and everything below it show
//! decrypted, and nothing outside it can be reached.
//!
//! On the server, the folder with id `I` keeps its entries in `d/XX/YYYY...` (a hash of `I`).
//! A file there is `<encrypted name>.c9r`. A folder is a directory `<encrypted name>.c9r` that
//! holds `dir.c9r` with the folder's own id; the top folder's id is empty. A name whose
//! encrypted form is too long is a directory `<hash>.c9s` holding the full name in `name.c9s`
//! and either `contents.c9r` (a file) or `dir.c9r` (a folder).

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{self, StreamExt, TryStreamExt};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use super::config::{SignedConfig, VaultConfig, CONFIG_FILE, MASTERKEY_FILE};
use super::content::{ContentCryptor, CLEARTEXT_CHUNK};
use super::masterkey::Masterkey;
use super::names::{
    shortened_name, NameCryptor, CONTENTS_FILE, DIRECTORY_FILE, DIRECTORY_ID_BACKUP,
    ENCRYPTED_SUFFIX, FULL_NAME_FILE, ROOT_DIRECTORY_ID, SHORTENED_SUFFIX, SYMLINK_FILE,
};
use super::stream::{NothingToRead, VaultReader, VaultWriter};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{DirListing, EntryKind, FileEntry};
use crate::protocol::{
    self, Capabilities, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use crate::remote_path;

/// The most `vault.cryptomator` and the masterkey file are read of.
const KEY_FILE_LIMIT: u64 = 64 * 1024;
const DIRECTORY_ID_LIMIT: u64 = 1024;
/// As Cryptomator: no name is this long.
const FULL_NAME_LIMIT: u64 = 10 * 1024;
const SYMLINK_LIMIT: u64 = 64 * 1024;
/// How long a folder's id is trusted before it is read again, since another program may move
/// or replace folders in the same vault.
const DIRECTORY_ID_LIFETIME: Duration = Duration::from_secs(60);
/// Requests in flight at once while looking into several nodes of one folder.
const LOOKUPS_AT_ONCE: usize = 8;
/// How much of a file is held in memory while it is copied over a connection that cannot read
/// and write at once.
const COPY_PIECE: u64 = 4 * 1024 * 1024;

pub(super) struct Keys {
    pub names: NameCryptor,
    pub content: ContentCryptor,
}

/// What every connection to one unlocked vault shares.
struct Vault {
    /// The vault's folder on the server.
    root: String,
    name: String,
    config: VaultConfig,
    /// Gone once the vault is locked.
    keys: RwLock<Option<Arc<Keys>>>,
    directory_ids: DirectoryIds,
    /// Held while a folder is created, so two transfer workers that need the same new folder
    /// do not each give it an id of its own, which would hide one worker's files.
    creating: tokio::sync::Mutex<()>,
}

/// Folder ids read recently, by the folder's path inside the vault.
#[derive(Default)]
struct DirectoryIds(Mutex<HashMap<String, (String, Instant)>>);

impl DirectoryIds {
    fn get(&self, path: &str) -> Option<String> {
        let mut ids = self.0.lock().unwrap();
        match ids.get(path) {
            Some((id, read)) if read.elapsed() < DIRECTORY_ID_LIFETIME => Some(id.clone()),
            Some(_) => {
                ids.remove(path);
                None
            }
            None => None,
        }
    }

    fn insert(&self, path: &str, id: &str) {
        self.0
            .lock()
            .unwrap()
            .insert(path.to_string(), (id.to_string(), Instant::now()));
    }

    /// Forgets the folder at `names`, everything inside it and every folder on the way to it.
    fn forget(&self, names: &[String]) {
        let path = names.join("/");
        let inside = format!("{path}/");
        self.0.lock().unwrap().retain(|known, _| {
            let leads_here = path
                .strip_prefix(known.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'));
            !leads_here && !known.starts_with(&inside)
        });
    }
}

/// A folder of the vault: its id, and where on the server its entries are kept.
struct Folder {
    id: String,
    storage: String,
}

/// How a name is stored in its folder's storage.
struct Node {
    /// `<encrypted name>.c9r`, or `<hash>.c9s` when that is too long.
    stored_name: String,
    /// The encrypted name a shortened node stands for.
    full_name: Option<String>,
}

/// What the server holds for a name.
enum Found {
    Nothing,
    File {
        /// The encrypted file.
        stored: String,
        /// What to remove to delete it: the file, or the shortened node around it.
        node: String,
        stored_size: u64,
        modified: Option<i64>,
    },
    /// A folder or, rarely, a symbolic link; which one shows when it is opened.
    Folder {
        node: String,
        modified: Option<i64>,
    },
    Link {
        node: String,
    },
}

pub struct VaultFs {
    vault: Arc<Vault>,
    /// The server's own files, which a reconnect replaces.
    server: RwLock<Arc<dyn RemoteFileSystem>>,
}

impl VaultFs {
    /// Opens the vault in the folder `root` on `server` with its password.
    pub async fn unlock(
        server: Arc<dyn RemoteFileSystem>,
        root: &str,
        password: String,
    ) -> AppResult<Arc<Self>> {
        let password = Zeroizing::new(password);
        let root = protocol::normalize(&server.resolve(root));
        let config_path = remote_path::join(&root, CONFIG_FILE);
        let Some(token) = read_small(&server, &config_path, KEY_FILE_LIMIT).await? else {
            return Err(not_a_vault(&server, &root).await);
        };
        let signed = SignedConfig::parse(&token)?;
        let masterkey_path = remote_path::join(&root, signed.masterkey_file()?);
        let masterkey_file = read_small(&server, &masterkey_path, KEY_FILE_LIMIT)
            .await?
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::NotFound,
                    format!(
                        "The vault's key file, {}, is missing",
                        remote_path::file_name(&masterkey_path)
                    ),
                )
                .with_path(masterkey_path.clone())
            })?;
        let masterkey =
            tokio::task::spawn_blocking(move || Masterkey::unlock(&masterkey_file, &password))
                .await??;
        let config = signed.verify(&masterkey)?;
        Ok(Self::assemble(server, root, config, &masterkey))
    }

    /// Makes the folder `name` in `parent` on `server` a new, empty vault that `password`
    /// opens, and opens it.
    pub async fn create(
        server: Arc<dyn RemoteFileSystem>,
        parent: &str,
        name: &str,
        password: String,
    ) -> AppResult<Arc<Self>> {
        let password = Zeroizing::new(password);
        if password.is_empty() {
            return Err(AppError::invalid("A vault needs a password"));
        }
        protocol::validate_name(name)?;
        let parent = protocol::normalize(&server.resolve(parent));
        let root = server.make_dir(&parent, name).await?;
        let filled = Self::fill_new_vault(&server, &root, password).await;
        if filled.is_err() {
            // The folder was made just now, so nothing but a half-written vault is in it.
            let _ = server.delete(std::slice::from_ref(&root)).await;
        }
        filled
    }

    async fn fill_new_vault(
        server: &Arc<dyn RemoteFileSystem>,
        root: &str,
        password: Zeroizing<String>,
    ) -> AppResult<Arc<Self>> {
        let masterkey = Masterkey::generate()?;
        let (masterkey, masterkey_file) = tokio::task::spawn_blocking(move || {
            masterkey.lock(&password).map(|file| (masterkey, file))
        })
        .await??;
        let config = VaultConfig::new();
        write_small(
            server,
            &remote_path::join(root, MASTERKEY_FILE),
            masterkey_file,
        )
        .await?;
        write_small(
            server,
            &remote_path::join(root, CONFIG_FILE),
            config.sign(&masterkey).into_bytes(),
        )
        .await?;
        let vault = Self::assemble(server.clone(), root.to_string(), config, &masterkey);
        let keys = vault.keys()?;
        vault
            .create_storage(&keys, server, ROOT_DIRECTORY_ID)
            .await?;
        Ok(vault)
    }

    fn assemble(
        server: Arc<dyn RemoteFileSystem>,
        root: String,
        config: VaultConfig,
        masterkey: &Masterkey,
    ) -> Arc<Self> {
        let keys = Keys {
            names: NameCryptor::new(masterkey),
            content: ContentCryptor::new(masterkey, config.cipher_combo),
        };
        Arc::new(Self {
            vault: Arc::new(Vault {
                name: remote_path::file_name(&root).to_string(),
                root,
                config,
                keys: RwLock::new(Some(Arc::new(keys))),
                directory_ids: DirectoryIds::default(),
                creating: tokio::sync::Mutex::new(()),
            }),
            server: RwLock::new(server),
        })
    }

    /// The same unlocked vault reached through another connection to its server, as each
    /// transfer worker has.
    pub fn through(&self, server: Arc<dyn RemoteFileSystem>) -> Arc<Self> {
        Arc::new(Self {
            vault: self.vault.clone(),
            server: RwLock::new(server),
        })
    }

    /// Moves the vault onto a new connection to its server, after a reconnect.
    pub fn reattach(&self, server: Arc<dyn RemoteFileSystem>) {
        *self.server.write().unwrap() = server;
    }

    /// Forgets the vault's keys. Every connection to the vault stops working; reads and writes
    /// under way keep the keys until they end.
    pub fn lock(&self) {
        *self.vault.keys.write().unwrap() = None;
    }

    pub fn is_locked(&self) -> bool {
        self.vault.keys.read().unwrap().is_none()
    }

    /// Fails as everything in the vault does once it is locked.
    pub fn check_unlocked(&self) -> AppResult<()> {
        self.keys().map(drop)
    }

    /// The vault's folder on the server.
    pub fn root(&self) -> &str {
        &self.vault.root
    }

    pub fn name(&self) -> &str {
        &self.vault.name
    }

    /// The protocol of the server the vault's files are on.
    pub fn server_protocol(&self) -> Protocol {
        self.server().protocol()
    }

    fn keys(&self) -> AppResult<Arc<Keys>> {
        self.vault.keys.read().unwrap().clone().ok_or_else(|| {
            AppError::new(
                ErrorKind::SessionNotFound,
                format!("The vault {} is locked", self.vault.name),
            )
        })
    }

    fn server(&self) -> Arc<dyn RemoteFileSystem> {
        self.server.read().unwrap().clone()
    }

    /// The names from the vault's folder down to `path`, composed (NFC) as they are stored.
    fn relative(&self, path: &str) -> AppResult<Vec<String>> {
        let path = protocol::normalize(&self.resolve(path));
        let root = self.vault.root.as_str();
        let inside = if root == "/" {
            Some(&path[1..])
        } else if path == root {
            Some("")
        } else {
            path.strip_prefix(root)
                .and_then(|rest| rest.strip_prefix('/'))
        };
        match inside {
            Some(inside) => Ok(inside
                .split('/')
                .filter(|name| !name.is_empty())
                .map(|name| name.nfc().collect())
                .collect()),
            None => Err(AppError::invalid(format!(
                "{path} is outside the vault {}",
                self.vault.name
            ))
            .with_path(path)),
        }
    }

    /// The path, as the server would spell it, of the entry at `names` inside the vault.
    fn path_of(&self, names: &[String]) -> String {
        names.iter().fold(self.vault.root.clone(), |path, name| {
            remote_path::join(&path, name)
        })
    }

    fn storage_path(&self, keys: &Keys, directory_id: &str) -> String {
        remote_path::join(&self.vault.root, &keys.names.directory_path(directory_id))
    }

    fn node(&self, keys: &Keys, directory_id: &str, name: &str) -> Node {
        let full_name = format!(
            "{}{ENCRYPTED_SUFFIX}",
            keys.names.encrypt(directory_id, name)
        );
        if full_name.len() > self.vault.config.shortening_threshold {
            Node {
                stored_name: shortened_name(&full_name),
                full_name: Some(full_name),
            }
        } else {
            Node {
                stored_name: full_name,
                full_name: None,
            }
        }
    }

    /// The folder at `names`, reading the id of each folder on the way that is not known.
    async fn folder(
        &self,
        keys: &Keys,
        server: &Arc<dyn RemoteFileSystem>,
        names: &[String],
    ) -> AppResult<Folder> {
        let mut id = ROOT_DIRECTORY_ID.to_string();
        for depth in 0..names.len() {
            let path = names[..=depth].join("/");
            id = match self.vault.directory_ids.get(&path) {
                Some(known) => known,
                None => {
                    let read = self
                        .read_folder_id(keys, server, &id, &names[..=depth])
                        .await?;
                    self.vault.directory_ids.insert(&path, &read);
                    read
                }
            };
        }
        Ok(Folder {
            storage: self.storage_path(keys, &id),
            id,
        })
    }

    /// The id of the folder at `names`, whose parent has `parent_id`.
    async fn read_folder_id(
        &self,
        keys: &Keys,
        server: &Arc<dyn RemoteFileSystem>,
        parent_id: &str,
        names: &[String],
    ) -> AppResult<String> {
        let name = names.last().map(String::as_str).unwrap_or_default();
        let node = self.node(keys, parent_id, name);
        let node_path = remote_path::join(&self.storage_path(keys, parent_id), &node.stored_name);
        // Nearly always the folder is there, so its id is read without asking first what the
        // node is. Servers answer in their own ways when it is a file or missing, so what the
        // node is decides what went wrong.
        let read = read_directory_id(server, &node_path).await;
        if let Ok(Some(id)) = read {
            return Ok(id);
        }
        let path = self.path_of(names);
        match self.find(server, &node, &node_path).await? {
            Found::Nothing => return Err(not_found(&path)),
            Found::File { .. } => return Err(not_a_folder(&path)),
            Found::Folder { .. } | Found::Link { .. } => read?,
        };
        let link = remote_path::join(&node_path, SYMLINK_FILE);
        Err(match read_small(server, &link, SYMLINK_LIMIT).await? {
            Some(stored) => {
                let target = decrypt_whole(keys, &stored)
                    .ok()
                    .and_then(|target| String::from_utf8(target).ok())
                    .unwrap_or_default();
                AppError::unsupported(format!(
                    "{name} is a symbolic link to {target}, which Poros does not follow inside \
                     a vault"
                ))
                .with_path(path)
            }
            None => AppError::integrity(format!(
                "The folder {name} is damaged in the vault: its {DIRECTORY_FILE} is missing"
            ))
            .with_path(path),
        })
    }

    /// What the server holds at a node.
    async fn find(
        &self,
        server: &Arc<dyn RemoteFileSystem>,
        node: &Node,
        node_path: &str,
    ) -> AppResult<Found> {
        if node.full_name.is_none() {
            return Ok(match server.stat(node_path).await? {
                None => Found::Nothing,
                Some(stat) if stat.is_dir => Found::Folder {
                    node: node_path.to_string(),
                    modified: stat.modified,
                },
                Some(stat) => Found::File {
                    stored: node_path.to_string(),
                    node: node_path.to_string(),
                    stored_size: stat.size,
                    modified: stat.modified,
                },
            });
        }
        let children = match server.list_dir(node_path).await {
            Ok(listing) => listing.entries,
            Err(error) if error.kind == ErrorKind::NotFound => return Ok(Found::Nothing),
            Err(error) => return Err(error),
        };
        Ok(found_in_shortened_node(node_path, &children, None))
    }

    /// Creates the storage of the folder with `directory_id`, and in it the copy of the id
    /// Cryptomator keeps for repairs.
    async fn create_storage(
        &self,
        keys: &Keys,
        server: &Arc<dyn RemoteFileSystem>,
        directory_id: &str,
    ) -> AppResult<String> {
        let mut storage = self.vault.root.clone();
        for part in keys.names.directory_path(directory_id).split('/') {
            storage = remote_path::join(&storage, part);
            server.ensure_dir(&storage).await?;
        }
        write_small(
            server,
            &remote_path::join(&storage, DIRECTORY_ID_BACKUP),
            encrypt_whole(keys, directory_id.as_bytes())?,
        )
        .await?;
        Ok(storage)
    }

    /// Creates the folder `name` in the folder at `parent`. With `exclusive`, a folder already
    /// there is an error. The caller holds the vault's `creating` lock.
    async fn create_folder(
        &self,
        keys: &Keys,
        server: &Arc<dyn RemoteFileSystem>,
        parent: &[String],
        name: &str,
        exclusive: bool,
    ) -> AppResult<()> {
        let mut names = parent.to_vec();
        names.push(name.to_string());
        let path = self.path_of(&names);
        let folder = self.folder(keys, server, parent).await?;
        let node = self.node(keys, &folder.id, name);
        let node_path = remote_path::join(&folder.storage, &node.stored_name);
        match self.find(server, &node, &node_path).await? {
            Found::Nothing => {}
            Found::File { .. } | Found::Link { .. } => {
                return Err(if exclusive {
                    protocol::already_exists(&path)
                } else {
                    protocol::not_a_folder(&path)
                })
            }
            Found::Folder { .. } => {
                if let Some(id) = read_directory_id(server, &node_path).await? {
                    self.vault.directory_ids.insert(&names.join("/"), &id);
                    return match exclusive {
                        true => Err(protocol::already_exists(&path)),
                        false => Ok(()),
                    };
                }
                let link = remote_path::join(&node_path, SYMLINK_FILE);
                if server.stat(&link).await?.is_some() {
                    return Err(protocol::already_exists(&path));
                }
                // A node with nothing in it is what an interrupted creation leaves behind.
            }
        }

        let id = uuid::Uuid::new_v4().to_string();
        let created = async {
            server.ensure_dir(&node_path).await?;
            if let Some(full_name) = &node.full_name {
                let name_file = remote_path::join(&node_path, FULL_NAME_FILE);
                write_small(server, &name_file, full_name.clone().into_bytes()).await?;
            }
            let id_file = remote_path::join(&node_path, DIRECTORY_FILE);
            write_small(server, &id_file, id.clone().into_bytes()).await?;
            self.create_storage(keys, server, &id).await
        }
        .await;
        if let Err(error) = created {
            // Without its id a node is not a folder, so nothing can be in it yet.
            let _ = server.delete(std::slice::from_ref(&node_path)).await;
            return Err(error);
        }
        self.vault.directory_ids.insert(&names.join("/"), &id);
        Ok(())
    }

    /// Lists the storage of the folder at `names`. An id remembered from earlier may belong to
    /// a folder another program has since replaced, so a missing storage is looked up afresh
    /// once.
    async fn list_storage(
        &self,
        keys: &Keys,
        server: &Arc<dyn RemoteFileSystem>,
        names: &[String],
    ) -> AppResult<(Folder, DirListing)> {
        let folder = self.folder(keys, server, names).await?;
        match server.list_dir(&folder.storage).await {
            Ok(listing) => Ok((folder, listing)),
            Err(error) if error.kind == ErrorKind::NotFound && !names.is_empty() => {
                self.vault.directory_ids.forget(names);
                let folder = self.folder(keys, server, names).await?;
                let listing = server.list_dir(&folder.storage).await.map_err(|error| {
                    if error.kind == ErrorKind::NotFound {
                        AppError::integrity(format!(
                            "The folder {} is damaged in the vault: its contents are missing",
                            names.last().map(String::as_str).unwrap_or_default()
                        ))
                        .with_path(self.path_of(names))
                    } else {
                        error
                    }
                })?;
                Ok((folder, listing))
            }
            Err(error) => Err(error),
        }
    }

    /// The entry a shortened node stands for; `None` when it is not one this vault can read.
    async fn shortened_entry(
        &self,
        keys: &Keys,
        server: &Arc<dyn RemoteFileSystem>,
        folder: &Folder,
        directory: &str,
        stored_name: String,
        node_modified: Option<i64>,
    ) -> AppResult<Option<FileEntry>> {
        let node_path = remote_path::join(&folder.storage, &stored_name);
        let children = match server.list_dir(&node_path).await {
            Ok(listing) => listing.entries,
            Err(error) if error.kind == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let name_file = remote_path::join(&node_path, FULL_NAME_FILE);
        let Some(full_name) = read_small(server, &name_file, FULL_NAME_LIMIT).await? else {
            return Ok(None);
        };
        let Some(name) = String::from_utf8(full_name)
            .ok()
            .filter(|full_name| shortened_name(full_name) == stored_name)
            .and_then(|full_name| {
                let encrypted = full_name.strip_suffix(ENCRYPTED_SUFFIX)?;
                keys.names.decrypt(&folder.id, encrypted).ok()
            })
            .filter(|name| protocol::validate_name(name).is_ok())
        else {
            return Ok(None);
        };
        let path = remote_path::join(directory, &name);
        Ok(
            match found_in_shortened_node(&node_path, &children, node_modified) {
                Found::Nothing => None,
                Found::File {
                    stored_size,
                    modified,
                    ..
                } => Some(entry(
                    path,
                    name,
                    EntryKind::File,
                    keys.content.cleartext_size(stored_size),
                    modified,
                )),
                Found::Folder { modified, .. } => {
                    Some(entry(path, name, EntryKind::Dir, 0, modified))
                }
                Found::Link { .. } => Some(entry(path, name, EntryKind::Symlink, 0, node_modified)),
            },
        )
    }

    /// The storage of the folder with `directory_id` and of every folder below it, parents
    /// before their children.
    async fn storages_below(
        &self,
        keys: &Keys,
        server: &Arc<dyn RemoteFileSystem>,
        directory_id: String,
    ) -> AppResult<Vec<String>> {
        // The top folder's id never belongs to a folder inside the vault, and a damaged vault
        // may name one folder twice.
        let mut seen: HashSet<String> = HashSet::from([ROOT_DIRECTORY_ID.to_string()]);
        let mut storages = Vec::new();
        let mut ids = vec![directory_id];
        let mut next = 0;
        while next < ids.len() {
            let id = ids[next].clone();
            next += 1;
            if !seen.insert(id.clone()) {
                continue;
            }
            let storage = self.storage_path(keys, &id);
            let listing = match server.list_dir(&storage).await {
                Ok(listing) => listing,
                Err(error) if error.kind == ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let mut nodes = Vec::new();
            for stored in &listing.entries {
                if stored.is_dir_like() && is_node_name(&stored.name) {
                    nodes.push(remote_path::join(&storage, &stored.name));
                }
            }
            let found: Vec<Option<String>> = stream::iter(nodes)
                .map(|node| async move { read_directory_id(server, &node).await })
                .buffer_unordered(LOOKUPS_AT_ONCE)
                .try_collect()
                .await?;
            ids.extend(found.into_iter().flatten());
            storages.push(storage);
        }
        Ok(storages)
    }

    /// Removes a folder's node and the storage of the folder and of everything in it. The
    /// deepest storages go first, so a failure part way leaves folders that still show and can
    /// be deleted again.
    async fn delete_folder(
        &self,
        keys: &Keys,
        server: &Arc<dyn RemoteFileSystem>,
        node: &str,
    ) -> AppResult<()> {
        if let Some(id) = read_directory_id(server, node).await? {
            let storages = self.storages_below(keys, server, id).await?;
            for storage in storages.iter().rev() {
                match server.delete(std::slice::from_ref(storage)).await {
                    Err(error) if error.kind != ErrorKind::NotFound => return Err(error),
                    _ => {}
                }
            }
        }
        server.delete(&[node.to_string()]).await
    }

    /// Gives the node of a file or folder another name in the same folder. A rename that
    /// fails part way leaves the entry under its old name, as long as the server still answers.
    async fn rename_node(
        &self,
        server: &Arc<dyn RemoteFileSystem>,
        found: Found,
        old: &Node,
        new: &Node,
        new_node_path: &str,
    ) -> AppResult<()> {
        let name_file = remote_path::join(new_node_path, FULL_NAME_FILE);
        let full_name = new.full_name.clone().map(String::into_bytes);
        match found {
            Found::Nothing => unreachable!("the caller found the entry"),
            Found::Folder { node, .. } | Found::Link { node } => {
                server.rename(&node, &new.stored_name).await?;
                let named = match (full_name, &old.full_name) {
                    (Some(full_name), _) => write_small(server, &name_file, full_name).await,
                    (None, Some(_)) => server.delete(&[name_file]).await,
                    (None, None) => Ok(()),
                };
                if named.is_err() {
                    rename_back(server, new_node_path, &node, old).await;
                }
                named
            }
            Found::File { node, .. } if old.full_name.is_some() && full_name.is_some() => {
                server.rename(&node, &new.stored_name).await?;
                let named = write_small(server, &name_file, full_name.unwrap_or_default()).await;
                if named.is_err() {
                    rename_back(server, new_node_path, &node, old).await;
                }
                named
            }
            Found::File { node, .. } if old.full_name.is_none() && full_name.is_none() => {
                server.rename(&node, &new.stored_name).await.map(drop)
            }
            // Between a plain and a shortened name the encrypted file changes folders, which
            // not every protocol can do in place, so it is copied as it is and the old one
            // removed.
            Found::File {
                stored,
                node,
                stored_size,
                modified,
            } => {
                let copied = async {
                    let target = match full_name {
                        Some(full_name) => {
                            server.ensure_dir(new_node_path).await?;
                            write_small(server, &name_file, full_name).await?;
                            remote_path::join(new_node_path, CONTENTS_FILE)
                        }
                        None => new_node_path.to_string(),
                    };
                    copy_stored(server, &stored, &target, stored_size, modified).await
                }
                .await;
                if let Err(error) = copied {
                    // A part copy would show as a second, damaged file.
                    let _ = server.delete(&[new_node_path.to_string()]).await;
                    return Err(error);
                }
                // Should the old file stay, both copies are whole, so nothing is lost.
                server.delete(&[node]).await
            }
        }
    }
}

/// Moves a node renamed to `new_node_path` back to `old_node_path` after what had to follow
/// the rename failed, and gives it back the name file it had: without one that matches its
/// name, a shortened node is not shown at all. Best effort, as the connection may be gone.
async fn rename_back(
    server: &Arc<dyn RemoteFileSystem>,
    new_node_path: &str,
    old_node_path: &str,
    old: &Node,
) {
    if server
        .rename(new_node_path, &old.stored_name)
        .await
        .is_err()
    {
        return;
    }
    let name_file = remote_path::join(old_node_path, FULL_NAME_FILE);
    let _ = match &old.full_name {
        Some(full_name) => write_small(server, &name_file, full_name.clone().into_bytes()).await,
        // A plain node needs none; one the failed write may have left is removed.
        None => server.delete(&[name_file]).await,
    };
}

#[async_trait]
impl RemoteFileSystem for VaultFs {
    fn protocol(&self) -> Protocol {
        Protocol::Cryptomator
    }

    fn transport(&self) -> Protocol {
        self.server_protocol()
    }

    fn connection_identity(&self) -> usize {
        self.server().connection_identity()
    }

    fn home(&self) -> &str {
        &self.vault.root
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            ranged_reads: self.server().capabilities().ranged_reads,
            // Continuing a file would mean trusting a last chunk the interruption may have cut.
            resumable_writes: false,
        }
    }

    fn coarse_times(&self) -> bool {
        self.server().coarse_times()
    }

    async fn canonicalize(&self, path: &str) -> AppResult<String> {
        let keys = self.keys()?;
        let names = self.relative(path)?;
        self.folder(&keys, &self.server(), &names).await?;
        Ok(self.path_of(&names))
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        let keys = self.keys()?;
        let server = self.server();
        let names = self.relative(path)?;
        let directory = self.path_of(&names);
        let (folder, listing) = self.list_storage(&keys, &server, &names).await?;

        let mut entries = Vec::with_capacity(listing.entries.len());
        let mut shortened: Vec<(String, Option<i64>)> = Vec::new();
        for stored in &listing.entries {
            if stored.name.ends_with(SHORTENED_SUFFIX) {
                if stored.is_dir_like() {
                    shortened.push((stored.name.clone(), stored.modified));
                }
                continue;
            }
            let Some(name) = stored
                .name
                .strip_suffix(ENCRYPTED_SUFFIX)
                .filter(|_| stored.name != DIRECTORY_ID_BACKUP)
                .and_then(|encrypted| keys.names.decrypt(&folder.id, encrypted).ok())
                .filter(|name| protocol::validate_name(name).is_ok())
            else {
                continue;
            };
            let path = remote_path::join(&directory, &name);
            if stored.is_dir_like() {
                entries.push(entry(path, name, EntryKind::Dir, 0, stored.modified));
            } else if matches!(stored.kind, EntryKind::File | EntryKind::Symlink) {
                let size = keys.content.cleartext_size(stored.size);
                entries.push(entry(path, name, EntryKind::File, size, stored.modified));
            }
        }
        let read: Vec<Option<FileEntry>> = stream::iter(shortened)
            .map(|(stored_name, modified)| {
                self.shortened_entry(&keys, &server, &folder, &directory, stored_name, modified)
            })
            .buffer_unordered(LOOKUPS_AT_ONCE)
            .try_collect()
            .await?;
        entries.extend(read.into_iter().flatten());

        Ok(DirListing {
            parent: remote_path::parent(&directory),
            path: directory,
            entries,
        })
    }

    async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        protocol::validate_name(name)?;
        let keys = self.keys()?;
        let server = self.server();
        let names = self.relative(parent)?;
        let name: String = name.nfc().collect();
        let _creating = self.vault.creating.lock().await;
        self.create_folder(&keys, &server, &names, &name, true)
            .await?;
        Ok(remote_path::join(&self.path_of(&names), &name))
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        protocol::validate_name(new_name)?;
        let keys = self.keys()?;
        let server = self.server();
        let names = self.relative(path)?;
        let Some((name, parent)) = names.split_last() else {
            return Err(AppError::invalid(
                "Lock the vault to rename its folder on the server",
            ));
        };
        let new_name: String = new_name.nfc().collect();
        let renamed = remote_path::join(&self.path_of(parent), &new_name);
        if *name == new_name {
            return Ok(renamed);
        }
        let folder = self.folder(&keys, &server, parent).await?;
        let old = self.node(&keys, &folder.id, name);
        let new = self.node(&keys, &folder.id, &new_name);
        let old_node_path = remote_path::join(&folder.storage, &old.stored_name);
        let new_node_path = remote_path::join(&folder.storage, &new.stored_name);
        let found = match self.find(&server, &old, &old_node_path).await? {
            Found::Nothing => return Err(not_found(&self.path_of(&names))),
            found => found,
        };
        if !matches!(
            self.find(&server, &new, &new_node_path).await?,
            Found::Nothing
        ) {
            return Err(protocol::already_exists(&renamed));
        }
        let result = self
            .rename_node(&server, found, &old, &new, &new_node_path)
            .await;
        // Even a rename that failed may have left a folder under its new name.
        self.vault.directory_ids.forget(&names);
        result?;
        Ok(renamed)
    }

    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        let keys = self.keys()?;
        let server = self.server();
        let resolved: Vec<String> = paths.iter().map(|path| self.resolve(path)).collect();
        for path in protocol::outermost(&resolved) {
            let names = self.relative(&path)?;
            let Some((name, parent)) = names.split_last() else {
                return Err(AppError::invalid(
                    "Lock the vault and delete its folder on the server to remove the whole vault",
                ));
            };
            let folder = self.folder(&keys, &server, parent).await?;
            let node = self.node(&keys, &folder.id, name);
            let node_path = remote_path::join(&folder.storage, &node.stored_name);
            match self.find(&server, &node, &node_path).await? {
                Found::Nothing => return Err(not_found(&path)),
                Found::File { node, .. } | Found::Link { node } => server.delete(&[node]).await?,
                Found::Folder { node, .. } => {
                    let deleted = self.delete_folder(&keys, &server, &node).await;
                    self.vault.directory_ids.forget(&names);
                    deleted?;
                }
            }
        }
        Ok(())
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        let keys = self.keys()?;
        let server = self.server();
        let names = self.relative(path)?;
        let folder_stat = |modified| RemoteStat {
            is_dir: true,
            size: 0,
            modified,
            permissions: None,
        };
        let Some((name, parent)) = names.split_last() else {
            return Ok(Some(folder_stat(None)));
        };
        let folder = match self.folder(&keys, &server, parent).await {
            Ok(folder) => folder,
            // Nothing is below a folder that is missing, nor below a file.
            Err(error) if matches!(error.kind, ErrorKind::NotFound | ErrorKind::InvalidInput) => {
                return Ok(None)
            }
            Err(error) => return Err(error),
        };
        let node = self.node(&keys, &folder.id, name);
        let node_path = remote_path::join(&folder.storage, &node.stored_name);
        Ok(match self.find(&server, &node, &node_path).await? {
            Found::Nothing => None,
            Found::File {
                stored_size,
                modified,
                ..
            } => Some(RemoteStat {
                is_dir: false,
                size: keys.content.cleartext_size(stored_size),
                modified,
                permissions: None,
            }),
            Found::Folder { modified, .. } => Some(folder_stat(modified)),
            Found::Link { .. } => Some(folder_stat(None)),
        })
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        let keys = self.keys()?;
        let server = self.server();
        let names = self.relative(path)?;
        if names.is_empty() || self.vault.directory_ids.get(&names.join("/")).is_some() {
            return Ok(());
        }
        let _creating = self.vault.creating.lock().await;
        for depth in 0..names.len() {
            match self.folder(&keys, &server, &names[..=depth]).await {
                Ok(_) => {}
                Err(error) if error.kind == ErrorKind::NotFound => {
                    self.create_folder(&keys, &server, &names[..depth], &names[depth], false)
                        .await?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Only the time is kept: the encrypted file's own time is the file's time, as in
    /// Cryptomator.
    async fn set_attributes(
        &self,
        path: &str,
        modified: Option<i64>,
        _permissions: Option<u32>,
    ) -> AppResult<()> {
        let keys = self.keys()?;
        let server = self.server();
        let names = self.relative(path)?;
        let Some((name, parent)) = names.split_last() else {
            return Ok(());
        };
        if modified.is_none() {
            return Ok(());
        }
        let folder = self.folder(&keys, &server, parent).await?;
        let node = self.node(&keys, &folder.id, name);
        let node_path = remote_path::join(&folder.storage, &node.stored_name);
        let target = match self.find(&server, &node, &node_path).await? {
            Found::Nothing => return Err(not_found(&self.path_of(&names))),
            Found::File { stored, .. } => stored,
            Found::Folder { node, .. } | Found::Link { node } => node,
        };
        server.set_attributes(&target, modified, None).await
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        let keys = self.keys()?;
        let server = self.server();
        let names = self.relative(path)?;
        let path = self.path_of(&names);
        let Some((name, parent)) = names.split_last() else {
            return Err(is_a_folder(&path));
        };
        let folder = self.folder(&keys, &server, parent).await?;
        let node = self.node(&keys, &folder.id, name);
        let node_path = remote_path::join(&folder.storage, &node.stored_name);
        // A plain node is read without asking what it is first: a folder there fails to open.
        let stored = match &node.full_name {
            None => node_path,
            Some(_) => match self.find(&server, &node, &node_path).await? {
                Found::File { stored, .. } => stored,
                Found::Nothing => return Err(not_found(&path)),
                Found::Folder { .. } | Found::Link { .. } => return Err(is_a_folder(&path)),
            },
        };

        let header_length = keys.content.header_length() as u64;
        let read_header = || async {
            let stored_header = read_small(&server, &stored, header_length)
                .await
                .map_err(|error| in_vault_terms(error, &path))?
                .ok_or_else(|| not_found(&path))?;
            keys.content
                .decrypt_header(&stored_header)
                .map_err(|error| error.with_path(path.clone()))
        };
        if range.start >= range.end {
            // Nothing to read, but the header still shows the file is one of this vault's.
            read_header().await?;
            return Ok(Box::new(NothingToRead));
        }
        let chunk = CLEARTEXT_CHUNK as u64;
        let encrypted_chunk = keys.content.encrypted_chunk() as u64;
        let first_chunk = range.start / chunk;
        let stored_end =
            header_length.saturating_add(range.end.div_ceil(chunk).saturating_mul(encrypted_chunk));
        let (header, stored_start) = match first_chunk {
            // From the start, the header arrives with the contents in one request.
            0 => (None, 0),
            _ => (
                Some(read_header().await?),
                header_length + first_chunk * encrypted_chunk,
            ),
        };
        let source = server
            .clone()
            .open_read(&stored, stored_start..stored_end)
            .await
            .map_err(|error| in_vault_terms(error, &path))?;
        Ok(Box::new(VaultReader::new(
            source,
            keys.clone(),
            path,
            header,
            first_chunk,
            (range.start % chunk) as usize,
            range.end - range.start,
        )))
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        if request.offset > 0 {
            return Err(AppError::unsupported(
                "A file in a vault cannot be continued where an upload stopped",
            ));
        }
        let keys = self.keys()?;
        let server = self.server();
        let names = self.relative(&request.path)?;
        let path = self.path_of(&names);
        let Some((name, parent)) = names.split_last() else {
            return Err(is_a_folder(&path));
        };
        let folder = self.folder(&keys, &server, parent).await?;
        let node = self.node(&keys, &folder.id, name);
        let node_path = remote_path::join(&folder.storage, &node.stored_name);
        // Storage that keeps objects by name would take a file beside a folder of the same
        // name, so there what is at the name is looked at first. SFTP and FTP servers refuse
        // to write a file where a folder is, which saves every upload a round trip.
        let refuses_itself = node.full_name.is_none() && !server.protocol().is_http();
        let stored = if refuses_itself {
            node_path
        } else {
            match self.find(&server, &node, &node_path).await? {
                Found::Folder { .. } | Found::Link { .. } => return Err(is_a_folder(&path)),
                Found::File { stored, .. } => stored,
                Found::Nothing => match &node.full_name {
                    None => node_path,
                    Some(full_name) => {
                        server.ensure_dir(&node_path).await?;
                        let name_file = remote_path::join(&node_path, FULL_NAME_FILE);
                        write_small(&server, &name_file, full_name.clone().into_bytes()).await?;
                        remote_path::join(&node_path, CONTENTS_FILE)
                    }
                },
            }
        };
        let target = server
            .clone()
            .open_write(WriteRequest {
                path: stored,
                offset: 0,
                size: keys.content.encrypted_size(request.size),
                modified: request.modified,
                permissions: None,
            })
            .await
            .map_err(|error| in_vault_terms(error, &path))?;
        Ok(Box::new(VaultWriter::new(target, keys, path)?))
    }
}

fn entry(
    path: String,
    name: String,
    kind: EntryKind,
    size: u64,
    modified: Option<i64>,
) -> FileEntry {
    FileEntry {
        hidden: name.starts_with('.'),
        name,
        path,
        kind,
        link_target: None,
        size,
        modified,
        permissions: None,
        owner: None,
        group: None,
    }
}

fn is_node_name(stored_name: &str) -> bool {
    stored_name.ends_with(ENCRYPTED_SUFFIX) || stored_name.ends_with(SHORTENED_SUFFIX)
}

/// What a shortened node holds, from the listing of the node itself.
fn found_in_shortened_node(
    node_path: &str,
    children: &[FileEntry],
    node_modified: Option<i64>,
) -> Found {
    let child = |name: &str| children.iter().find(|child| child.name == name);
    let node = node_path.to_string();
    if let Some(contents) = child(CONTENTS_FILE) {
        Found::File {
            stored: remote_path::join(node_path, CONTENTS_FILE),
            node,
            stored_size: contents.size,
            modified: contents.modified,
        }
    } else if child(DIRECTORY_FILE).is_some() {
        Found::Folder {
            node,
            modified: node_modified,
        }
    } else if child(SYMLINK_FILE).is_some() {
        Found::Link { node }
    } else {
        Found::Nothing
    }
}

/// The id in a folder node's `dir.c9r`; `None` when the node has none.
async fn read_directory_id(
    server: &Arc<dyn RemoteFileSystem>,
    node_path: &str,
) -> AppResult<Option<String>> {
    let id_file = remote_path::join(node_path, DIRECTORY_FILE);
    let Some(id) = read_small(server, &id_file, DIRECTORY_ID_LIMIT).await? else {
        return Ok(None);
    };
    String::from_utf8(id).map(Some).map_err(|_| {
        AppError::integrity(format!("{DIRECTORY_FILE} does not hold a folder's id"))
            .with_path(id_file)
    })
}

/// Up to `limit` bytes of a file on the server; `None` when there is no such file.
async fn read_small(
    server: &Arc<dyn RemoteFileSystem>,
    path: &str,
    limit: u64,
) -> AppResult<Option<Vec<u8>>> {
    let mut stream = match server.clone().open_read(path, 0..limit).await {
        Ok(stream) => stream,
        Err(error) if error.kind == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut contents = Vec::new();
    loop {
        match stream.next_chunk().await {
            Ok(Some(chunk)) => contents.extend_from_slice(&chunk),
            Ok(None) => break,
            Err(error) if error.kind == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    stream.finish().await?;
    contents.truncate(limit as usize);
    Ok(Some(contents))
}

async fn write_small(
    server: &Arc<dyn RemoteFileSystem>,
    path: &str,
    contents: Vec<u8>,
) -> AppResult<()> {
    let mut stream = server
        .clone()
        .open_write(WriteRequest {
            path: path.to_string(),
            offset: 0,
            size: contents.len() as u64,
            modified: None,
            permissions: None,
        })
        .await?;
    if !contents.is_empty() {
        stream.write(Bytes::from(contents)).await?;
    }
    stream.finish().await
}

/// Copies an encrypted file as it is: contents are not bound to the name they are stored
/// under.
async fn copy_stored(
    server: &Arc<dyn RemoteFileSystem>,
    source: &str,
    target: &str,
    size: u64,
    modified: Option<i64>,
) -> AppResult<()> {
    if server.protocol().is_ftp() {
        return copy_in_turns(server, source, target, size, modified).await;
    }
    let mut reader = server.clone().open_read(source, 0..size).await?;
    let mut writer = server
        .clone()
        .open_write(WriteRequest {
            path: target.to_string(),
            offset: 0,
            size,
            modified,
            permissions: None,
        })
        .await?;
    while let Some(chunk) = reader.next_chunk().await? {
        writer.write(chunk).await?;
    }
    reader.finish().await?;
    writer.finish().await
}

/// Copies a file over an FTP connection, which cannot read and write at once: a piece is read
/// whole, then written where the last one ended.
async fn copy_in_turns(
    server: &Arc<dyn RemoteFileSystem>,
    source: &str,
    target: &str,
    size: u64,
    modified: Option<i64>,
) -> AppResult<()> {
    let mut copied = 0;
    loop {
        let end = size.min(copied + COPY_PIECE);
        let mut piece = Vec::with_capacity((end - copied) as usize);
        let mut reader = server.clone().open_read(source, copied..end).await?;
        while let Some(chunk) = reader.next_chunk().await? {
            piece.extend_from_slice(&chunk);
        }
        reader.finish().await?;
        if piece.len() as u64 != end - copied {
            return Err(AppError::new(
                ErrorKind::Io,
                format!(
                    "{} changed while it was being renamed",
                    remote_path::file_name(source)
                ),
            ));
        }
        let mut writer = server
            .clone()
            .open_write(WriteRequest {
                path: target.to_string(),
                offset: copied,
                size,
                modified,
                permissions: None,
            })
            .await?;
        writer.write(Bytes::from(piece)).await?;
        writer.finish().await?;
        copied = end;
        if copied >= size {
            return Ok(());
        }
    }
}

/// Small contents in the form of an encrypted file: a header, then chunks. Where Cryptomator
/// writes such a file in one go, as it does a folder's copy of its id, nothing at all is still
/// one empty chunk.
fn encrypt_whole(keys: &Keys, cleartext: &[u8]) -> AppResult<Vec<u8>> {
    let (header, mut stored) = keys.content.new_header()?;
    if cleartext.is_empty() {
        keys.content
            .encrypt_chunk(&header, 0, cleartext, &mut stored)?;
    }
    for (chunk_number, chunk) in cleartext.chunks(CLEARTEXT_CHUNK).enumerate() {
        keys.content
            .encrypt_chunk(&header, chunk_number as u64, chunk, &mut stored)?;
    }
    Ok(stored)
}

fn decrypt_whole(keys: &Keys, stored: &[u8]) -> AppResult<Vec<u8>> {
    let header_length = keys.content.header_length().min(stored.len());
    let (stored_header, chunks) = stored.split_at(header_length);
    let header = keys.content.decrypt_header(stored_header)?;
    let mut cleartext = Vec::new();
    for (chunk_number, chunk) in chunks.chunks(keys.content.encrypted_chunk()).enumerate() {
        keys.content
            .decrypt_chunk(&header, chunk_number as u64, chunk, &mut cleartext)?;
    }
    Ok(cleartext)
}

/// Why the folder at `root` did not open as a vault.
async fn not_a_vault(server: &Arc<dyn RemoteFileSystem>, root: &str) -> AppError {
    let name = remote_path::file_name(root);
    let older_key = remote_path::join(root, MASTERKEY_FILE);
    if matches!(server.stat(&older_key).await, Ok(Some(_))) {
        return AppError::unsupported(format!(
            "{name} is a vault in an older format. Open it once in Cryptomator 1.6 or later to \
             upgrade it."
        ))
        .with_path(root);
    }
    AppError::new(
        ErrorKind::NotFound,
        format!("{name} is not a Cryptomator vault: it has no {CONFIG_FILE}"),
    )
    .with_path(root)
}

/// A server's error about a stored file, told of the file the vault shows instead.
pub(super) fn in_vault_terms(error: AppError, path: &str) -> AppError {
    match error.kind {
        ErrorKind::NotFound => not_found(path),
        _ => error.with_path(path),
    }
}

fn not_found(path: &str) -> AppError {
    AppError::new(
        ErrorKind::NotFound,
        format!("{} does not exist", remote_path::file_name(path)),
    )
    .with_path(path)
}

fn not_a_folder(path: &str) -> AppError {
    AppError::invalid(format!("{} is not a folder", remote_path::file_name(path))).with_path(path)
}

fn is_a_folder(path: &str) -> AppError {
    AppError::invalid(format!("{} is a folder", remote_path::file_name(path))).with_path(path)
}
