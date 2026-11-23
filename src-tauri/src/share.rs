//! Share links: addresses that open a file or folder for people who have no account on the
//! server. S3 and Backblaze B2 sign an address that works for a limited time; Google Drive,
//! OneDrive and Box keep a link with the item until it is removed.
//!
//! A link is a credential: whoever holds it can open the file. Nothing here writes one to
//! the log, and `ShareLink` and `ShareOptions` hide it from debug output.

use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel};
use crate::protocol::{Protocol, RemoteFileSystem};
use crate::timestamp;

/// The longest a signed address can work, on S3 and on Backblaze B2 alike.
pub const MAX_SIGNED_LIFETIME_SECS: u64 = 7 * 24 * 60 * 60;
/// How long a link lasts where it must expire and no lifetime was chosen.
pub const DEFAULT_LIFETIME_SECS: u64 = 60 * 60;
/// Past this, a lifetime is a mistake rather than a wish.
const LONGEST_LIFETIME_SECS: u64 = 10 * 365 * 24 * 60 * 60;
const MAX_PASSWORD_LENGTH: usize = 256;
const MAX_LINK_ID_LENGTH: usize = 1024;

/// What a link lets its holder do. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkKind {
    /// Opens the item in the browser or the service's own viewer.
    View,
    /// Lets the holder change the item.
    Edit,
    /// Saves the file under its name instead of showing it.
    Download,
}

impl LinkKind {
    pub fn name(self) -> &'static str {
        match self {
            LinkKind::View => "view",
            LinkKind::Edit => "edit",
            LinkKind::Download => "download",
        }
    }
}

/// Who a link works for. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Audience {
    /// Whoever holds the link, signed in or not.
    #[default]
    Anyone,
    /// People signed in to the account's organization.
    Organization,
    /// People the item is already shared with.
    Invited,
}

impl Audience {
    fn description(self) -> &'static str {
        match self {
            Audience::Anyone => "anyone",
            Audience::Organization => "your organization",
            Audience::Invited => "invited people",
        }
    }
}

/// Whether links end by themselves. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Expiry {
    /// Links work until they are removed.
    Never,
    /// A link may be given an end.
    Optional,
    /// Every link has an end.
    Required,
}

/// What links a session can make. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareCapabilities {
    pub kinds: Vec<LinkKind>,
    pub audiences: Vec<Audience>,
    pub expiry: Expiry,
    /// The longest a link can last, where the service sets a limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_lifetime_secs: Option<u64>,
    pub password: bool,
    /// Links stay with the item, so they can be listed and removed.
    pub revocable: bool,
    pub folders: bool,
    /// The organization `Audience::Organization` stands for, when its name is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
}

/// The link the page or the command line asks for. Mirrored in `src/lib/types.ts`.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareOptions {
    pub kind: LinkKind,
    #[serde(default)]
    pub audience: Audience,
    #[serde(default)]
    pub lifetime_secs: Option<u64>,
    #[serde(default)]
    pub password: Option<String>,
}

impl fmt::Debug for ShareOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShareOptions")
            .field("kind", &self.kind)
            .field("audience", &self.audience)
            .field("lifetime_secs", &self.lifetime_secs)
            .field("password", &self.password.as_ref().map(|_| "(hidden)"))
            .finish()
    }
}

/// Mirrored in `src/lib/types.ts`.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareLink {
    /// Names the link when removing it; empty for a link that cannot be removed.
    pub id: String,
    pub url: String,
    pub kind: LinkKind,
    pub audience: Audience,
    /// Seconds since the Unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    pub has_password: bool,
    pub revocable: bool,
    /// What the user should know about this link, in whole sentences.
    pub notes: Vec<String>,
}

impl fmt::Debug for ShareLink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShareLink")
            .field("id", &self.id)
            .field("url", &"(hidden)")
            .field("kind", &self.kind)
            .field("audience", &self.audience)
            .field("expires_at", &self.expires_at)
            .field("has_password", &self.has_password)
            .field("revocable", &self.revocable)
            .field("notes", &self.notes)
            .finish()
    }
}

/// A file system whose service can hand out links to its files.
#[async_trait]
pub trait LinkSharing: Send + Sync {
    fn share_capabilities(&self) -> ShareCapabilities;

    /// The links `path` already has. Services that sign an address on request keep none.
    async fn share_links(&self, _path: &str) -> AppResult<Vec<ShareLink>> {
        Ok(Vec::new())
    }

    /// `options` have been checked against `share_capabilities`, with any lifetime inside
    /// the service's limits.
    async fn create_share_link(&self, path: &str, options: &ShareOptions) -> AppResult<ShareLink>;

    async fn remove_share_link(&self, _path: &str, _link_id: &str) -> AppResult<()> {
        Err(not_revocable())
    }
}

fn not_revocable() -> AppError {
    AppError::unsupported("These links cannot be removed; they stop working when they expire")
}

fn sharing(files: &dyn RemoteFileSystem) -> AppResult<&dyn LinkSharing> {
    files.link_sharing().ok_or_else(|| {
        AppError::unsupported(match files.protocol() {
            Protocol::Cryptomator => {
                "Files in a vault cannot be shared with a link: the server holds them encrypted"
                    .to_string()
            }
            protocol => format!("{} has no share links", protocol.display_name()),
        })
    })
}

/// What links the session can make; `None` when it cannot make any.
pub fn capabilities(files: &dyn RemoteFileSystem) -> Option<ShareCapabilities> {
    files.link_sharing().map(LinkSharing::share_capabilities)
}

/// Fails, saying why, unless the session's links can be removed.
pub fn check_revocable(files: &dyn RemoteFileSystem) -> AppResult<()> {
    match sharing(files)?.share_capabilities().revocable {
        true => Ok(()),
        false => Err(not_revocable()),
    }
}

/// "7 days", "90 minutes": the largest unit that leaves no remainder.
pub fn lifetime_text(seconds: u64) -> String {
    let (count, unit) = match seconds {
        seconds if seconds % 86_400 == 0 => (seconds / 86_400, "day"),
        seconds if seconds % 3_600 == 0 => (seconds / 3_600, "hour"),
        seconds if seconds % 60 == 0 => (seconds / 60, "minute"),
        seconds => (seconds, "second"),
    };
    match count {
        1 => format!("1 {unit}"),
        count => format!("{count} {unit}s"),
    }
}

/// The options as the service will take them, and what had to change as notes for the user.
fn prepared(
    capabilities: &ShareCapabilities,
    service: &str,
    options: &ShareOptions,
) -> AppResult<(ShareOptions, Vec<String>)> {
    if !capabilities.kinds.contains(&options.kind) {
        let offered: Vec<&str> = capabilities.kinds.iter().map(|kind| kind.name()).collect();
        return Err(AppError::invalid(format!(
            "{service} cannot make {} links; choose {}",
            options.kind.name(),
            offered.join(" or ")
        )));
    }
    if !capabilities.audiences.contains(&options.audience) {
        return Err(AppError::invalid(format!(
            "{service} cannot make links for {} on this account",
            options.audience.description()
        )));
    }
    let mut notes = Vec::new();
    let lifetime_secs = match (capabilities.expiry, options.lifetime_secs) {
        (Expiry::Never, Some(_)) => {
            return Err(AppError::invalid(format!(
                "{service} links do not expire; remove the link to stop sharing"
            )))
        }
        (Expiry::Never, None) | (Expiry::Optional, None) => None,
        (Expiry::Required, None) => Some(DEFAULT_LIFETIME_SECS),
        (_, Some(asked)) => {
            let longest = capabilities
                .max_lifetime_secs
                .unwrap_or(LONGEST_LIFETIME_SECS);
            let lifetime = asked.clamp(1, longest);
            if asked > longest {
                notes.push(format!(
                    "{service} links last at most {}, so this one ends then.",
                    lifetime_text(longest)
                ));
            } else if asked == 0 {
                notes.push("The shortest a link can last is 1 second.".to_string());
            }
            Some(lifetime)
        }
    };
    let password = options
        .password
        .as_deref()
        .filter(|password| !password.is_empty());
    if let Some(password) = password {
        if !capabilities.password {
            return Err(AppError::invalid(format!(
                "{service} links cannot have a password on this account"
            )));
        }
        if password.chars().count() > MAX_PASSWORD_LENGTH || password.chars().any(char::is_control)
        {
            return Err(AppError::invalid(format!(
                "A link's password is at most {MAX_PASSWORD_LENGTH} characters of plain text"
            )));
        }
    }
    let prepared = ShareOptions {
        kind: options.kind,
        audience: options.audience,
        lifetime_secs,
        password: password.map(str::to_string),
    };
    Ok((prepared, notes))
}

fn checked_path(path: &str) -> AppResult<&str> {
    if path.trim().is_empty() || path.contains('\0') {
        return Err(AppError::invalid("Choose a file to share"));
    }
    Ok(path)
}

/// The links `path` has now, for services that keep them.
pub async fn links(files: &dyn RemoteFileSystem, path: &str) -> AppResult<Vec<ShareLink>> {
    sharing(files)?.share_links(checked_path(path)?).await
}

/// Makes a link to `path`. The log says that a link was made, never the link.
pub async fn create(
    files: &dyn RemoteFileSystem,
    events: &Events,
    session_id: &str,
    path: &str,
    options: &ShareOptions,
) -> AppResult<ShareLink> {
    let sharing = sharing(files)?;
    let path = checked_path(path)?;
    let service = files.protocol().display_name();
    let (options, mut notes) = prepared(&sharing.share_capabilities(), service, options)?;
    let mut link = sharing.create_share_link(path, &options).await?;
    notes.append(&mut link.notes);
    link.notes = notes;
    let ending = match link.expires_at {
        Some(expires_at) => format!(", valid until {}", timestamp::format_rfc3339(expires_at)),
        None => String::new(),
    };
    events.log(
        LogLevel::Info,
        Some(session_id),
        format!(
            "Made a {} link to {}{ending}",
            link.kind.name(),
            files.resolve(path)
        ),
    );
    Ok(link)
}

/// Removes one of the links `path` has, which stops it working for everyone.
pub async fn remove(
    files: &dyn RemoteFileSystem,
    events: &Events,
    session_id: &str,
    path: &str,
    link_id: &str,
) -> AppResult<()> {
    let sharing = sharing(files)?;
    let path = checked_path(path)?;
    let well_formed = !link_id.is_empty()
        && link_id.len() <= MAX_LINK_ID_LENGTH
        && !link_id.chars().any(char::is_control);
    if !well_formed {
        return Err(AppError::invalid("That is not a link of this item"));
    }
    sharing.remove_share_link(path, link_id).await?;
    events.log(
        LogLevel::Info,
        Some(session_id),
        format!("Stopped sharing {} with a link", files.resolve(path)),
    );
    Ok(())
}

/// A link's address, checked before it is handed to the system's browser: only web
/// addresses, so nothing a server sent can start another program.
pub fn web_address(address: &str) -> AppResult<reqwest::Url> {
    let refused = || AppError::invalid("Only web links can be opened in the browser");
    let url = reqwest::Url::parse(address.trim()).map_err(|_| refused())?;
    let is_web = matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|host| !host.is_empty());
    if is_web {
        Ok(url)
    } else {
        Err(refused())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn signed() -> ShareCapabilities {
        ShareCapabilities {
            kinds: vec![LinkKind::View, LinkKind::Download],
            audiences: vec![Audience::Anyone],
            expiry: Expiry::Required,
            max_lifetime_secs: Some(MAX_SIGNED_LIFETIME_SECS),
            password: false,
            revocable: false,
            folders: false,
            organization: None,
        }
    }

    fn kept() -> ShareCapabilities {
        ShareCapabilities {
            kinds: vec![LinkKind::View, LinkKind::Edit],
            audiences: vec![Audience::Anyone, Audience::Organization],
            expiry: Expiry::Optional,
            max_lifetime_secs: None,
            password: true,
            revocable: true,
            folders: true,
            organization: None,
        }
    }

    fn options(kind: LinkKind, lifetime_secs: Option<u64>) -> ShareOptions {
        ShareOptions {
            kind,
            audience: Audience::Anyone,
            lifetime_secs,
            password: None,
        }
    }

    #[test]
    fn clamps_a_lifetime_to_what_the_service_allows_and_says_so() {
        let (within, notes) =
            prepared(&signed(), "S3", &options(LinkKind::View, Some(3600))).unwrap();
        assert_eq!(within.lifetime_secs, Some(3600));
        assert!(notes.is_empty());

        let (longest, notes) =
            prepared(&signed(), "S3", &options(LinkKind::View, Some(604_800))).unwrap();
        assert_eq!(longest.lifetime_secs, Some(604_800));
        assert!(notes.is_empty());

        let (clamped, notes) =
            prepared(&signed(), "S3", &options(LinkKind::View, Some(604_801))).unwrap();
        assert_eq!(clamped.lifetime_secs, Some(604_800));
        assert_eq!(
            notes,
            ["S3 links last at most 7 days, so this one ends then."]
        );

        let (shortest, notes) =
            prepared(&signed(), "S3", &options(LinkKind::View, Some(0))).unwrap();
        assert_eq!(shortest.lifetime_secs, Some(1));
        assert_eq!(notes, ["The shortest a link can last is 1 second."]);

        let (far, notes) = prepared(
            &kept(),
            "OneDrive",
            &options(LinkKind::View, Some(u64::MAX)),
        )
        .unwrap();
        assert_eq!(far.lifetime_secs, Some(LONGEST_LIFETIME_SECS));
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn fills_in_a_lifetime_only_where_links_must_expire() {
        let (required, _) = prepared(&signed(), "S3", &options(LinkKind::View, None)).unwrap();
        assert_eq!(required.lifetime_secs, Some(DEFAULT_LIFETIME_SECS));
        let (optional, _) = prepared(&kept(), "OneDrive", &options(LinkKind::View, None)).unwrap();
        assert_eq!(optional.lifetime_secs, None);
        let never = ShareCapabilities {
            expiry: Expiry::Never,
            ..kept()
        };
        let refused =
            prepared(&never, "Google Drive", &options(LinkKind::View, Some(60))).unwrap_err();
        assert_eq!(refused.kind, ErrorKind::InvalidInput);
        assert!(refused.message.contains("do not expire"));
    }

    #[test]
    fn refuses_choices_the_service_does_not_offer() {
        let edit = prepared(&signed(), "S3", &options(LinkKind::Edit, None)).unwrap_err();
        assert_eq!(
            edit.message,
            "S3 cannot make edit links; choose view or download"
        );
        let organization = ShareOptions {
            audience: Audience::Organization,
            ..options(LinkKind::View, None)
        };
        let audience = prepared(&signed(), "S3", &organization).unwrap_err();
        assert!(audience.message.contains("your organization"));
        let with_password = ShareOptions {
            password: Some("open sesame".into()),
            ..options(LinkKind::View, None)
        };
        assert!(prepared(&signed(), "S3", &with_password).is_err());
        let (allowed, _) = prepared(&kept(), "OneDrive", &with_password).unwrap();
        assert_eq!(allowed.password.as_deref(), Some("open sesame"));
        let empty = ShareOptions {
            password: Some(String::new()),
            ..options(LinkKind::View, None)
        };
        let (none, _) = prepared(&signed(), "S3", &empty).unwrap();
        assert_eq!(none.password, None);
    }

    #[test]
    fn names_lifetimes_in_their_largest_whole_unit() {
        assert_eq!(lifetime_text(604_800), "7 days");
        assert_eq!(lifetime_text(86_400), "1 day");
        assert_eq!(lifetime_text(5_400), "90 minutes");
        assert_eq!(lifetime_text(3_600), "1 hour");
        assert_eq!(lifetime_text(61), "61 seconds");
    }

    #[test]
    fn debug_output_hides_the_link_and_the_password() {
        let link = ShareLink {
            id: "permission".into(),
            url: "https://example.com/file?X-Amz-Signature=abcdef".into(),
            kind: LinkKind::View,
            audience: Audience::Anyone,
            expires_at: None,
            has_password: false,
            revocable: false,
            notes: Vec::new(),
        };
        let shown = format!("{link:?}");
        assert!(!shown.contains("abcdef"));
        assert!(!shown.contains("example.com"));
        let options = ShareOptions {
            password: Some("open sesame".into()),
            ..options(LinkKind::View, None)
        };
        assert!(!format!("{options:?}").contains("sesame"));
    }

    #[test]
    fn opens_only_web_addresses_in_the_browser() {
        assert!(web_address("https://drive.google.com/file/d/abc/view").is_ok());
        assert!(web_address("http://127.0.0.1:9000/bucket/key?X-Amz-Expires=60").is_ok());
        for refused in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ms-msdt:/id",
            "/usr/bin/true",
            "",
            "https://",
        ] {
            assert!(web_address(refused).is_err(), "{refused}");
        }
    }
}
