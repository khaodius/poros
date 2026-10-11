//! Owners and groups for `-o`, `-g` and `--chown`. rsync matches them by name between the two
//! sides unless told to use numbers, so the account lists of both sides are read when needed.

use std::collections::HashMap;

use super::options::{parse_chown, SyncOptions};
use super::tree::Node;
use super::Place;
use crate::sftp::RemoteFs;

/// Larger account lists are not read.
const MAX_ACCOUNT_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// One side's users and groups, from its `/etc/passwd` and `/etc/group`.
#[derive(Debug, Clone, Default)]
pub struct Accounts {
    users: HashMap<String, u32>,
    groups: HashMap<String, u32>,
    user_names: HashMap<u32, String>,
    group_names: HashMap<u32, String>,
}

impl Accounts {
    pub fn parse(passwd: &str, group: &str) -> Self {
        let (users, user_names) = table(passwd);
        let (groups, group_names) = table(group);
        Self {
            users,
            groups,
            user_names,
            group_names,
        }
    }

    /// This computer's accounts. Windows has no numeric owners to set.
    pub fn local() -> Self {
        if cfg!(unix) {
            let read = |path| std::fs::read_to_string(path).unwrap_or_default();
            Self::parse(&read("/etc/passwd"), &read("/etc/group"))
        } else {
            Self::default()
        }
    }

    pub async fn remote(fs: &RemoteFs) -> Self {
        let read = |path| async move {
            fs.read_to_end(path, MAX_ACCOUNT_FILE_BYTES)
                .await
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default()
        };
        Self::parse(&read("/etc/passwd").await, &read("/etc/group").await)
    }
}

fn table(text: &str) -> (HashMap<String, u32>, HashMap<u32, String>) {
    let mut by_name = HashMap::new();
    let mut by_id = HashMap::new();
    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(':').collect();
        let (Some(name), Some(id)) = (fields.first(), fields.get(2)) else {
            continue;
        };
        let Ok(id) = id.trim().parse::<u32>() else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        by_name.entry(name.to_string()).or_insert(id);
        by_id.entry(id).or_insert_with(|| name.to_string());
    }
    (by_name, by_id)
}

/// Who owns what a synchronization creates or updates.
#[derive(Debug, Clone, Default)]
pub struct Owners {
    owner: bool,
    group: bool,
    by_number: bool,
    local: Accounts,
    remote: Accounts,
    /// `--chown` resolved on each side.
    chown_local: (Option<u32>, Option<u32>),
    chown_remote: (Option<u32>, Option<u32>),
}

impl Owners {
    /// Whether the options set any owner or group, so the account lists are needed.
    pub fn wanted(options: &SyncOptions) -> bool {
        options.owner || options.group || !options.chown.trim().is_empty()
    }

    /// Resolves `--chown` against the sides that receive files; a name either side lacks is
    /// an error, as it would be for rsync.
    pub fn new(
        options: &SyncOptions,
        local: Accounts,
        remote: Accounts,
        receivers: &[Place],
    ) -> Result<Self, String> {
        let (user, group) = parse_chown(&options.chown)?;
        let mut owners = Self {
            owner: options.owner,
            group: options.group,
            by_number: options.numeric_ids,
            local,
            remote,
            ..Self::default()
        };
        for &place in receivers {
            let (accounts, side) = match place {
                Place::Local => (&owners.local, "this computer"),
                Place::Remote => (&owners.remote, "the server"),
            };
            let resolve = |name: &Option<String>, table: &HashMap<String, u32>, kind: &str| {
                name.as_deref()
                    .map(|name| {
                        name.parse::<u32>()
                            .ok()
                            .or_else(|| table.get(name).copied())
                            .ok_or_else(|| format!("--chown: {side} has no {kind} \"{name}\""))
                    })
                    .transpose()
            };
            let resolved = (
                resolve(&user, &accounts.users, "user")?,
                resolve(&group, &accounts.groups, "group")?,
            );
            match place {
                Place::Local => owners.chown_local = resolved,
                Place::Remote => owners.chown_remote = resolved,
            }
        }
        Ok(owners)
    }

    /// The owner and group `node` should have once copied to `to`.
    pub fn ids(&self, node: &Node, to: Place) -> (Option<u32>, Option<u32>) {
        let (chown_user, chown_group) = match to {
            Place::Local => self.chown_local,
            Place::Remote => self.chown_remote,
        };
        let (from_accounts, to_accounts) = match to {
            Place::Local => (&self.remote, &self.local),
            Place::Remote => (&self.local, &self.remote),
        };
        let user = chown_user.or_else(|| {
            node.uid
                .filter(|_| self.owner)
                .map(|id| self.map(id, &from_accounts.user_names, &to_accounts.users))
        });
        let group = chown_group.or_else(|| {
            node.gid
                .filter(|_| self.group)
                .map(|id| self.map(id, &from_accounts.group_names, &to_accounts.groups))
        });
        (user, group)
    }

    /// The same name's number on the other side, or the number itself when either side does
    /// not know it. Root is always 0.
    fn map(&self, id: u32, names: &HashMap<u32, String>, ids: &HashMap<String, u32>) -> u32 {
        if self.by_number || id == 0 {
            return id;
        }
        names
            .get(&id)
            .and_then(|name| ids.get(name))
            .copied()
            .unwrap_or(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL_PASSWD: &str = "root:x:0:0::/root:/bin/sh\n# comment\nme:x:1000:1000::/home/me:/bin/sh\nweb:x:33:33::/var/www:/usr/sbin/nologin\n";
    const LOCAL_GROUP: &str = "root:x:0:\nme:x:1000:\nwww-data:x:33:\n";
    const REMOTE_PASSWD: &str = "root:x:0:0::/root:/bin/sh\nme:x:501:20::/Users/me:/bin/zsh\n";
    const REMOTE_GROUP: &str = "staff:x:20:\nwww-data:x:82:\nme:x:501:\n";

    fn node(uid: u32, gid: u32) -> Node {
        Node {
            uid: Some(uid),
            gid: Some(gid),
            ..Node::default()
        }
    }

    fn options(owner: bool, group: bool, chown: &str) -> SyncOptions {
        SyncOptions {
            owner,
            group,
            chown: chown.into(),
            ..SyncOptions::default()
        }
    }

    fn owners(options: &SyncOptions, receivers: &[Place]) -> Result<Owners, String> {
        Owners::new(
            options,
            Accounts::parse(LOCAL_PASSWD, LOCAL_GROUP),
            Accounts::parse(REMOTE_PASSWD, REMOTE_GROUP),
            receivers,
        )
    }

    #[test]
    fn owners_match_by_name_unless_numbers_are_asked_for() {
        let mapped = owners(&options(true, true, ""), &[Place::Remote]).unwrap();
        // "me" is 1000 here and 501 there; group 1000 is "me" too.
        assert_eq!(
            mapped.ids(&node(1000, 1000), Place::Remote),
            (Some(501), Some(501))
        );
        // Group 33 is www-data on both sides.
        assert_eq!(
            mapped.ids(&node(33, 33), Place::Remote),
            (Some(33), Some(82))
        );
        // Root stays root; unknown numbers stay as they are.
        assert_eq!(
            mapped.ids(&node(0, 4242), Place::Remote),
            (Some(0), Some(4242))
        );

        let numeric = owners(
            &SyncOptions {
                numeric_ids: true,
                ..options(true, false, "")
            },
            &[Place::Remote],
        )
        .unwrap();
        assert_eq!(
            numeric.ids(&node(1000, 1000), Place::Remote),
            (Some(1000), None)
        );
        let nothing = owners(&options(false, false, ""), &[Place::Remote]).unwrap();
        assert_eq!(nothing.ids(&node(1000, 1000), Place::Remote), (None, None));
    }

    #[test]
    fn chown_names_resolve_on_the_receiving_side() {
        let chown = owners(&options(false, false, "me:www-data"), &[Place::Remote]).unwrap();
        assert_eq!(chown.ids(&node(7, 7), Place::Remote), (Some(501), Some(82)));
        let local = owners(&options(true, false, ":www-data"), &[Place::Local]).unwrap();
        assert_eq!(
            local.ids(&node(501, 20), Place::Local),
            (Some(1000), Some(33))
        );
        assert!(owners(&options(false, false, "nobody"), &[Place::Remote])
            .unwrap_err()
            .contains("the server has no user \"nobody\""));
        assert_eq!(
            owners(&options(false, false, "123:456"), &[Place::Local])
                .unwrap()
                .ids(&node(1, 1), Place::Local),
            (Some(123), Some(456))
        );
    }
}
