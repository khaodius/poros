//! OneDrive's sharing links, made and removed through Microsoft Graph. A link is a
//! permission on the item with an address of its own, so removing the permission stops
//! the link for everyone.

use async_trait::async_trait;
use percent_encoding::utf8_percent_encode;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;

use super::{OneDriveFs, DRIVE_URL, PATH_SEGMENT};
use crate::cloud::api::{self, Refusal};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol::{self, RemoteFileSystem};
use crate::share::{
    Audience, Expiry, LinkKind, LinkSharing, ShareCapabilities, ShareLink, ShareOptions,
};
use crate::timestamp;

/// What kind of account a drive belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Account {
    /// A Microsoft account of one person: links are for anyone, and may take a password.
    Personal,
    /// A work or school account, whose links can be kept within the organization.
    Organization,
    /// Graph named a kind this code does not know, so every choice is offered and Graph
    /// decides.
    Unknown,
}

impl Account {
    pub(super) fn of(drive_type: &str) -> Self {
        match drive_type {
            "personal" => Account::Personal,
            "business" | "documentLibrary" => Account::Organization,
            _ => Account::Unknown,
        }
    }
}

/// A permission on an item, as Graph lists it and as `createLink` answers.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Permission {
    id: String,
    /// Present on permissions that are sharing links.
    #[serde(default)]
    link: Option<SharingLink>,
    #[serde(default)]
    expiration_date_time: Option<String>,
    #[serde(default)]
    has_password: bool,
    /// The folder above that the permission comes from, when it is not the item's own.
    #[serde(default)]
    inherited_from: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharingLink {
    /// `view`, `edit`, `embed` and a few narrower kinds.
    #[serde(rename = "type", default)]
    link_type: String,
    /// `anonymous`, `organization`, or `users` for a link only named people can open.
    #[serde(default)]
    scope: String,
    #[serde(default)]
    web_url: String,
}

#[derive(Debug, Deserialize)]
struct PermissionPage {
    #[serde(default)]
    value: Vec<Permission>,
    #[serde(rename = "@odata.nextLink", default)]
    next_link: Option<String>,
}

impl Permission {
    fn is_inherited(&self) -> bool {
        self.inherited_from
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .is_some_and(|ancestor| !ancestor.is_empty())
    }

    /// The link this permission is; `None` for one that names people, or that embeds the
    /// item in a page.
    fn link(&self) -> Option<ShareLink> {
        let link = self.link.as_ref()?;
        let audience = match link.scope.as_str() {
            "anonymous" => Audience::Anyone,
            "organization" => Audience::Organization,
            _ => return None,
        };
        if link.web_url.is_empty() || link.link_type == "embed" {
            return None;
        }
        let inherited = self.is_inherited();
        Some(ShareLink {
            id: self.id.clone(),
            url: link.web_url.clone(),
            kind: match link.link_type.as_str() {
                "edit" => LinkKind::Edit,
                _ => LinkKind::View,
            },
            audience,
            // Graph writes "no end" as the first second of year 1.
            expires_at: self
                .expiration_date_time
                .as_deref()
                .and_then(timestamp::parse_rfc3339)
                .filter(|seconds| *seconds > 0),
            has_password: self.has_password,
            revocable: !inherited,
            notes: match inherited {
                true => vec![
                    "This link comes from a folder above. Stop sharing that folder to remove it."
                        .to_string(),
                ],
                false => Vec::new(),
            },
        })
    }
}

/// The body of `createLink` for the link `options` ask for, made at `now`.
fn link_request(options: &ShareOptions, now: i64) -> AppResult<serde_json::Value> {
    let link_type = match options.kind {
        LinkKind::View => "view",
        LinkKind::Edit => "edit",
        LinkKind::Download => {
            return Err(AppError::invalid(
                "OneDrive has no download links; make a view link instead",
            ))
        }
    };
    let scope = match options.audience {
        Audience::Anyone => "anonymous",
        Audience::Organization => "organization",
        Audience::Invited => {
            return Err(AppError::invalid(
                "OneDrive links are for anyone or for your organization",
            ))
        }
    };
    let mut body = json!({ "type": link_type, "scope": scope });
    if let Some(lifetime_secs) = options.lifetime_secs {
        body["expirationDateTime"] = json!(timestamp::format_rfc3339(
            now.saturating_add_unsigned(lifetime_secs)
        ));
    }
    if let Some(password) = &options.password {
        body["password"] = json!(password);
    }
    Ok(body)
}

/// Graph's refusal to make a link, with what the account's kind suggests doing about it.
fn link_refusal(refusal: Refusal, options: &ShareOptions, account: Account) -> AppError {
    let said = refusal.error.message.trim_end_matches('.');
    let extras = options.lifetime_secs.is_some() || options.password.is_some();
    let advice = match (account, options.audience) {
        (Account::Organization | Account::Unknown, Audience::Anyone) => {
            " Your organization may not allow links that anyone can open: choose people in your organization instead, or ask an administrator."
        }
        (Account::Personal | Account::Unknown, _) if extras => {
            " On a personal account, links that expire or have a password need a Microsoft 365 subscription: try without them."
        }
        _ => "",
    };
    AppError::new(
        refusal.error.kind,
        format!("OneDrive did not make the link: {said}.{advice}"),
    )
}

fn permissions_url(item_id: &str) -> String {
    format!(
        "{DRIVE_URL}/items/{}/permissions",
        utf8_percent_encode(item_id, PATH_SEGMENT)
    )
}

fn top_folder() -> AppError {
    AppError::invalid("The top folder itself cannot be shared. Share a folder inside it.")
}

#[async_trait]
impl LinkSharing for OneDriveFs {
    fn share_capabilities(&self) -> ShareCapabilities {
        ShareCapabilities {
            kinds: vec![LinkKind::View, LinkKind::Edit],
            audiences: match self.account {
                Account::Personal => vec![Audience::Anyone],
                Account::Organization | Account::Unknown => {
                    vec![Audience::Anyone, Audience::Organization]
                }
            },
            expiry: Expiry::Optional,
            max_lifetime_secs: None,
            password: self.account != Account::Organization,
            revocable: true,
            folders: true,
            organization: None,
        }
    }

    async fn share_links(&self, path: &str) -> AppResult<Vec<ShareLink>> {
        let path = protocol::normalize(&self.resolve(path));
        if path == "/" {
            return Ok(Vec::new());
        }
        let item = self.existing(&path).await?;
        let mut links = Vec::new();
        let mut url = permissions_url(&item.id);
        loop {
            let page: PermissionPage = self.api.json(&|http| http.get(&url)).await?;
            links.extend(page.value.iter().filter_map(Permission::link));
            match page.next_link {
                Some(next) => url = next,
                None => return Ok(links),
            }
        }
    }

    async fn create_share_link(&self, path: &str, options: &ShareOptions) -> AppResult<ShareLink> {
        let path = protocol::normalize(&self.resolve(path));
        if path == "/" {
            return Err(top_folder());
        }
        let body = link_request(options, timestamp::now_seconds())?;
        let item = self.existing(&path).await?;
        let url = format!(
            "{DRIVE_URL}/items/{}/createLink",
            utf8_percent_encode(&item.id, PATH_SEGMENT)
        );
        let response = self
            .api
            .send_expecting(
                &|http| http.post(&url).json(&body),
                &[StatusCode::BAD_REQUEST, StatusCode::FORBIDDEN],
            )
            .await?;
        if !response.status().is_success() {
            return Err(link_refusal(
                api::refusal(response).await,
                options,
                self.account,
            ));
        }
        // Graph answers 200 instead of 201 when it hands back a link the item already had.
        let kept = response.status() == StatusCode::OK;
        let permission: Permission = api::decode(response).await?;
        let mut link = permission.link().ok_or_else(|| {
            AppError::new(
                ErrorKind::Cloud,
                "OneDrive made the link but gave no address for it",
            )
        })?;
        if kept {
            link.notes.insert(
                0,
                "This item already had a link of this kind, which was kept as it is. Stop sharing it first to make one with other settings."
                    .to_string(),
            );
        }
        Ok(link)
    }

    async fn remove_share_link(&self, path: &str, link_id: &str) -> AppResult<()> {
        let path = protocol::normalize(&self.resolve(path));
        if path == "/" {
            return Err(top_folder());
        }
        let item = self.existing(&path).await?;
        let url = format!(
            "{}/{}",
            permissions_url(&item.id),
            utf8_percent_encode(link_id, PATH_SEGMENT)
        );
        let response = self
            .api
            .send_expecting(&|http| http.delete(&url), &[StatusCode::NOT_FOUND])
            .await
            .map_err(|error| match error.kind {
                ErrorKind::PermissionDenied | ErrorKind::InvalidInput => AppError::new(
                    error.kind,
                    format!(
                        "OneDrive did not remove the link: {}. A link that comes from a folder above is removed by no longer sharing that folder.",
                        error.message.trim_end_matches('.')
                    ),
                ),
                _ => error,
            })?;
        // A link already removed, here or in the browser, is gone either way.
        drop(response);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(kind: LinkKind, audience: Audience) -> ShareOptions {
        ShareOptions {
            kind,
            audience,
            lifetime_secs: None,
            password: None,
        }
    }

    #[test]
    fn tells_account_kinds_apart() {
        assert_eq!(Account::of("personal"), Account::Personal);
        assert_eq!(Account::of("business"), Account::Organization);
        assert_eq!(Account::of("documentLibrary"), Account::Organization);
        assert_eq!(Account::of(""), Account::Unknown);
    }

    #[test]
    fn builds_the_create_link_request() {
        assert_eq!(
            link_request(&options(LinkKind::View, Audience::Anyone), 0).unwrap(),
            json!({ "type": "view", "scope": "anonymous" })
        );
        let limited = ShareOptions {
            lifetime_secs: Some(86_400),
            password: Some("ThisIsMyPrivatePassword".into()),
            ..options(LinkKind::Edit, Audience::Organization)
        };
        // 14 May 2024, 18:30 UTC, and a day later.
        assert_eq!(
            link_request(&limited, 1_715_711_400).unwrap(),
            json!({
                "type": "edit",
                "scope": "organization",
                "expirationDateTime": "2024-05-15T18:30:00Z",
                "password": "ThisIsMyPrivatePassword",
            })
        );
        assert!(link_request(&options(LinkKind::Download, Audience::Anyone), 0).is_err());
        assert!(link_request(&options(LinkKind::View, Audience::Invited), 0).is_err());
        assert_eq!(
            permissions_url("01ABC!123"),
            "https://graph.microsoft.com/v1.0/me/drive/items/01ABC%21123/permissions"
        );
    }

    #[test]
    fn reads_the_answer_to_create_link() {
        let permission: Permission = serde_json::from_str(
            r#"{
                "id": "123ABC",
                "roles": ["write"],
                "link": {
                    "type": "edit",
                    "scope": "anonymous",
                    "webUrl": "https://1drv.ms/A6913278E564460AA616C71B28AD6EB6",
                    "application": {"id": "1234", "displayName": "Sample Application"}
                },
                "hasPassword": true,
                "expirationDateTime": "2024-05-15T18:30:00Z"
            }"#,
        )
        .unwrap();
        let link = permission.link().unwrap();
        assert_eq!(link.id, "123ABC");
        assert_eq!(link.url, "https://1drv.ms/A6913278E564460AA616C71B28AD6EB6");
        assert_eq!(link.kind, LinkKind::Edit);
        assert_eq!(link.audience, Audience::Anyone);
        assert_eq!(link.expires_at, Some(1_715_797_800));
        assert!(link.has_password && link.revocable);
    }

    #[test]
    fn lists_only_the_permissions_that_are_links() {
        let page: PermissionPage = serde_json::from_str(
            r#"{"value": [
                {"id": "1", "roles": ["owner"], "grantedToV2": {"user": {"displayName": "Ada"}}},
                {"id": "2", "roles": ["read"], "expirationDateTime": "0001-01-01T00:00:00Z",
                 "link": {"type": "view", "scope": "organization", "webUrl": "https://contoso-my.sharepoint.com/:t:/g/personal/ada/EZ"}},
                {"id": "3", "roles": ["read"],
                 "link": {"type": "view", "scope": "users", "webUrl": "https://contoso-my.sharepoint.com/:t:/g/personal/ada/EU"}},
                {"id": "4", "roles": ["read"], "inheritedFrom": {"driveId": "b!1", "id": "01PARENT", "path": "/drive/root:/Shared"},
                 "link": {"type": "view", "scope": "anonymous", "webUrl": "https://1drv.ms/f/abc"}},
                {"id": "5", "roles": ["read"], "inheritedFrom": {},
                 "link": {"type": "embed", "scope": "anonymous", "webUrl": "https://onedrive.live.com/embed?x"}}
            ], "@odata.nextLink": "https://graph.microsoft.com/v1.0/me/drive/items/01/permissions?$skiptoken=x"}"#,
        )
        .unwrap();
        assert!(page.next_link.is_some());
        let links: Vec<ShareLink> = page.value.iter().filter_map(Permission::link).collect();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].id, "2");
        assert_eq!(links[0].audience, Audience::Organization);
        assert_eq!(links[0].expires_at, None);
        assert!(links[0].revocable && !links[0].has_password);
        assert_eq!(links[1].id, "4");
        assert!(!links[1].revocable);
        assert_eq!(links[1].notes.len(), 1);
    }

    #[test]
    fn says_what_to_do_when_graph_refuses() {
        let denied = br#"{"error":{"code":"accessDenied","message":"Sharing has been disabled for this drive.","innererror":{"code":"sharingDisabled","request-id":"1"}}}"#;
        let refusal = api::refusal_from(StatusCode::FORBIDDEN, denied);
        assert!(refusal.has_reason(&["sharingDisabled"]));
        let work = link_refusal(
            refusal,
            &options(LinkKind::View, Audience::Anyone),
            Account::Organization,
        );
        assert_eq!(work.kind, ErrorKind::PermissionDenied);
        assert_eq!(
            work.message,
            "OneDrive did not make the link: Sharing has been disabled for this drive. Your organization may not allow links that anyone can open: choose people in your organization instead, or ask an administrator."
        );
        let limited = ShareOptions {
            lifetime_secs: Some(3600),
            ..options(LinkKind::View, Audience::Anyone)
        };
        let personal = link_refusal(
            api::refusal_from(
                StatusCode::BAD_REQUEST,
                br#"{"error":{"code":"invalidRequest","message":"Expiration is not available"}}"#,
            ),
            &limited,
            Account::Personal,
        );
        assert_eq!(personal.kind, ErrorKind::InvalidInput);
        assert!(personal
            .message
            .ends_with("need a Microsoft 365 subscription: try without them."));
        let plain = link_refusal(
            api::refusal_from(
                StatusCode::BAD_REQUEST,
                br#"{"error":{"code":"invalidRequest","message":"Nope."}}"#,
            ),
            &options(LinkKind::View, Audience::Anyone),
            Account::Personal,
        );
        assert_eq!(plain.message, "OneDrive did not make the link: Nope.");
    }
}
