//! Box's shared links. An item has at most one, kept as its `shared_link`: setting it
//! makes or changes the link, and clearing it stops the link for everyone.

use async_trait::async_trait;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;

use super::{BoxFs, BoxItem, API_URL, ROOT_ID};
use crate::cloud::api::{self, Refusal};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol::{self, RemoteFileSystem};
use crate::share::{
    Audience, Expiry, LinkKind, LinkSharing, ShareCapabilities, ShareLink, ShareOptions,
};
use crate::timestamp;

/// An item has one link, so one name tells it apart.
const LINK_ID: &str = "shared_link";

#[derive(Debug, Deserialize)]
struct SharedItem {
    #[serde(default)]
    shared_link: Option<SharedLink>,
}

#[derive(Debug, Deserialize)]
struct SharedLink {
    url: String,
    /// `open`, `company` or `collaborators`, as it was asked for.
    #[serde(default)]
    access: Option<String>,
    /// What the account's and the folders' settings leave of `access`.
    #[serde(default)]
    effective_access: Option<String>,
    #[serde(default)]
    unshared_at: Option<String>,
    #[serde(default)]
    is_password_enabled: bool,
}

fn audience_of(access: &str) -> Option<Audience> {
    match access {
        "open" => Some(Audience::Anyone),
        "company" => Some(Audience::Organization),
        "collaborators" => Some(Audience::Invited),
        _ => None,
    }
}

fn access_of(audience: Audience) -> &'static str {
    match audience {
        Audience::Anyone => "open",
        Audience::Organization => "company",
        Audience::Invited => "collaborators",
    }
}

impl SharedLink {
    fn link(&self) -> ShareLink {
        let asked = self.access.as_deref().and_then(audience_of);
        let effective = self.effective_access.as_deref().and_then(audience_of);
        let mut notes = Vec::new();
        if let (Some(asked), Some(effective)) = (asked, effective) {
            if asked != effective {
                notes.push(
                    "The settings of the account or of a folder above narrow who this link works for."
                        .to_string(),
                );
            }
        }
        ShareLink {
            id: LINK_ID.to_string(),
            url: self.url.clone(),
            kind: LinkKind::View,
            audience: effective.or(asked).unwrap_or(Audience::Invited),
            expires_at: self
                .unshared_at
                .as_deref()
                .and_then(timestamp::parse_rfc3339),
            has_password: self.is_password_enabled,
            revocable: true,
            notes,
        }
    }
}

/// The body that gives an item the link `options` ask for, made at `now`. Box keeps the
/// value of a field left out, so a link without an end says so: otherwise an end the
/// item's link already has would stay. A password left out stays too, which keeps a
/// protected link protected.
fn link_request(options: &ShareOptions, now: i64) -> AppResult<serde_json::Value> {
    if options.kind != LinkKind::View {
        return Err(AppError::invalid(
            "Box links open the item for viewing; make a view link",
        ));
    }
    let unshared_at = options
        .lifetime_secs
        .map(|lifetime_secs| timestamp::format_rfc3339(now.saturating_add_unsigned(lifetime_secs)));
    let mut link = json!({
        "access": access_of(options.audience),
        "unshared_at": unshared_at,
    });
    if let Some(password) = &options.password {
        if options.audience != Audience::Anyone {
            return Err(AppError::invalid(
                "Box links take a password only when anyone can open them",
            ));
        }
        link["password"] = json!(password);
    }
    Ok(json!({ "shared_link": link }))
}

/// Box's refusal to make a link, in Box's words first, then what the chosen options
/// suggest trying. A refusal can be about the plan or about the password itself, and Box
/// does not always say which.
fn link_refusal(refusal: Refusal, options: &ShareOptions) -> AppError {
    let said = refusal.error.message.trim_end_matches('.');
    let extras = options.lifetime_secs.is_some()
        || options.password.is_some()
        || options.audience == Audience::Organization;
    let advice = if refusal.error.kind == ErrorKind::PermissionDenied {
        " Your account or its administrator may not allow a link like this for the item: try a narrower audience, or ask the item's owner."
    } else if extras {
        " Links that expire, have a password or are limited to the company may need a paid Box plan: if Box's reason is not about the password itself, try without them."
    } else {
        ""
    };
    AppError::new(
        refusal.error.kind,
        format!("Box did not make the link: {said}.{advice}"),
    )
}

fn item_url(item: &BoxItem) -> String {
    format!("{API_URL}/{}/{}", item.collection(), item.id)
}

fn top_folder() -> AppError {
    AppError::invalid("The top folder itself cannot be shared. Share a folder inside it.")
}

impl BoxFs {
    async fn shareable(&self, path: &str) -> AppResult<BoxItem> {
        let path = protocol::normalize(&self.resolve(path));
        let (item, _) = self.existing(&path).await?;
        if item.id == ROOT_ID {
            return Err(top_folder());
        }
        Ok(item)
    }
}

#[async_trait]
impl LinkSharing for BoxFs {
    fn share_capabilities(&self) -> ShareCapabilities {
        ShareCapabilities {
            kinds: vec![LinkKind::View],
            audiences: vec![Audience::Anyone, Audience::Organization, Audience::Invited],
            expiry: Expiry::Optional,
            max_lifetime_secs: None,
            password: true,
            revocable: true,
            folders: true,
            single_link: true,
            organization: None,
        }
    }

    async fn share_links(&self, path: &str) -> AppResult<Vec<ShareLink>> {
        let path = protocol::normalize(&self.resolve(path));
        let (item, _) = self.existing(&path).await?;
        if item.id == ROOT_ID {
            return Ok(Vec::new());
        }
        let url = item_url(&item);
        let shared: SharedItem = self
            .api
            .json(&|http| http.get(&url).query(&[("fields", "shared_link")]))
            .await?;
        Ok(shared.shared_link.iter().map(SharedLink::link).collect())
    }

    /// Sets the item's one link, which changes the link it already has, if any, for
    /// everyone who has its address: the address stays, the audience and end become the
    /// ones asked for, and a password stays unless a new one is given.
    async fn create_share_link(&self, path: &str, options: &ShareOptions) -> AppResult<ShareLink> {
        let body = link_request(options, timestamp::now_seconds())?;
        let item = self.shareable(path).await?;
        let url = item_url(&item);
        let response = self
            .api
            .send_expecting(
                &|http| {
                    http.put(&url)
                        .query(&[("fields", "shared_link")])
                        .json(&body)
                },
                &[StatusCode::BAD_REQUEST, StatusCode::FORBIDDEN],
            )
            .await?;
        if !response.status().is_success() {
            return Err(link_refusal(api::refusal(response).await, options));
        }
        let shared: SharedItem = api::decode(response).await?;
        shared
            .shared_link
            .map(|shared_link| shared_link.link())
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::Cloud,
                    "Box took the request but gave no link for the item",
                )
            })
    }

    async fn remove_share_link(&self, path: &str, _link_id: &str) -> AppResult<bool> {
        let item = self.shareable(path).await?;
        let url = item_url(&item);
        let shared: SharedItem = self
            .api
            .json(&|http| http.get(&url).query(&[("fields", "shared_link")]))
            .await?;
        if shared.shared_link.is_none() {
            // Already removed, here or in the browser.
            return Ok(false);
        }
        let body = json!({ "shared_link": null });
        self.api
            .send(&|http| {
                http.put(&url)
                    .query(&[("fields", "shared_link")])
                    .json(&body)
            })
            .await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(audience: Audience) -> ShareOptions {
        ShareOptions {
            kind: LinkKind::View,
            audience,
            lifetime_secs: None,
            password: None,
        }
    }

    #[test]
    fn builds_the_shared_link_request() {
        assert_eq!(
            link_request(&options(Audience::Anyone), 0).unwrap(),
            json!({ "shared_link": { "access": "open", "unshared_at": null } })
        );
        assert_eq!(
            link_request(&options(Audience::Invited), 0).unwrap(),
            json!({ "shared_link": { "access": "collaborators", "unshared_at": null } })
        );
        let limited = ShareOptions {
            lifetime_secs: Some(7 * 86_400),
            ..options(Audience::Organization)
        };
        // 14 May 2024, 18:30 UTC, and a week later.
        assert_eq!(
            link_request(&limited, 1_715_711_400).unwrap(),
            json!({ "shared_link": { "access": "company", "unshared_at": "2024-05-21T18:30:00Z" } })
        );
        let protected = ShareOptions {
            password: Some("Open-Sesame1".into()),
            ..options(Audience::Anyone)
        };
        assert_eq!(
            link_request(&protected, 0).unwrap(),
            json!({ "shared_link": { "access": "open", "unshared_at": null, "password": "Open-Sesame1" } })
        );
        let narrow = ShareOptions {
            password: Some("Open-Sesame1".into()),
            ..options(Audience::Organization)
        };
        assert!(link_request(&narrow, 0).is_err());
        let edit = ShareOptions {
            kind: LinkKind::Edit,
            ..options(Audience::Anyone)
        };
        assert!(link_request(&edit, 0).is_err());
    }

    #[test]
    fn addresses_items_by_kind() {
        let item = |item_type: &str| BoxItem {
            item_type: item_type.into(),
            id: "12345".into(),
            ..BoxItem::root()
        };
        assert_eq!(
            item_url(&item("file")),
            "https://api.box.com/2.0/files/12345"
        );
        assert_eq!(
            item_url(&item("folder")),
            "https://api.box.com/2.0/folders/12345"
        );
        assert_eq!(
            item_url(&item("web_link")),
            "https://api.box.com/2.0/web_links/12345"
        );
    }

    #[test]
    fn reads_an_item_s_shared_link() {
        let shared: SharedItem = serde_json::from_str(
            r#"{"type":"file","id":"12345","etag":"1","shared_link":{
                "url":"https://app.box.com/s/kwio6b4ovt1264rnfbyqo1",
                "download_url":"https://app.box.com/shared/static/kwio6b4ovt1264rnfbyqo1.pdf",
                "vanity_url":null,"vanity_name":null,
                "access":"open","effective_access":"company","effective_permission":"can_download",
                "unshared_at":"2024-05-21T18:30:00Z","is_password_enabled":true,
                "permissions":{"can_download":true,"can_preview":true,"can_edit":false},
                "download_count":0,"preview_count":0}}"#,
        )
        .unwrap();
        let link = shared.shared_link.unwrap().link();
        assert_eq!(link.id, LINK_ID);
        assert_eq!(link.url, "https://app.box.com/s/kwio6b4ovt1264rnfbyqo1");
        assert_eq!(link.kind, LinkKind::View);
        // What works in practice, not what was asked for.
        assert_eq!(link.audience, Audience::Organization);
        assert_eq!(link.notes.len(), 1);
        assert_eq!(link.expires_at, Some(1_716_316_200));
        assert!(link.has_password && link.revocable);

        let unshared: SharedItem =
            serde_json::from_str(r#"{"type":"folder","id":"9","shared_link":null}"#).unwrap();
        assert!(unshared.shared_link.is_none());
        let plain: SharedItem = serde_json::from_str(
            r#"{"type":"folder","id":"9","shared_link":{"url":"https://app.box.com/s/abc","access":"collaborators","effective_access":"collaborators","unshared_at":null,"is_password_enabled":false}}"#,
        )
        .unwrap();
        let link = plain.shared_link.unwrap().link();
        assert_eq!(link.audience, Audience::Invited);
        assert_eq!(link.expires_at, None);
        assert!(link.notes.is_empty());
    }

    #[test]
    fn says_what_to_do_when_box_refuses() {
        let denied = br#"{"type":"error","status":403,"code":"access_denied_insufficient_permissions","message":"Access denied - insufficient permission","request_id":"abc"}"#;
        let refusal = api::refusal_from(StatusCode::FORBIDDEN, denied);
        assert!(refusal.has_reason(&["access_denied_insufficient_permissions"]));
        let error = link_refusal(refusal, &options(Audience::Anyone));
        assert_eq!(error.kind, ErrorKind::PermissionDenied);
        assert!(error.message.starts_with(
            "Box did not make the link: Access denied - insufficient permission. Your account"
        ));
        let limited = ShareOptions {
            lifetime_secs: Some(3600),
            ..options(Audience::Anyone)
        };
        let plan = link_refusal(
            api::refusal_from(
                StatusCode::BAD_REQUEST,
                br#"{"type":"error","status":400,"code":"bad_request","message":"Bad Request"}"#,
            ),
            &limited,
        );
        assert!(plan
            .message
            .starts_with("Box did not make the link: Bad Request. "));
        assert!(plan.message.contains("may need a paid Box plan"));
        let protected = ShareOptions {
            password: Some("short".into()),
            ..options(Audience::Anyone)
        };
        let weak = link_refusal(
            api::refusal_from(
                StatusCode::BAD_REQUEST,
                br#"{"type":"error","status":400,"code":"bad_request","message":"Password does not meet minimum requirements."}"#,
            ),
            &protected,
        );
        assert!(weak.message.starts_with(
            "Box did not make the link: Password does not meet minimum requirements. "
        ));
        // Box's reason comes first, and the plan is only a maybe.
        assert!(weak.message.contains(" may need a paid Box plan"));
        assert!(!weak.message.contains("limited to the company need"));
        let plain = link_refusal(
            api::refusal_from(
                StatusCode::BAD_REQUEST,
                br#"{"type":"error","status":400,"code":"bad_request","message":"Bad Request"}"#,
            ),
            &options(Audience::Anyone),
        );
        assert_eq!(plain.message, "Box did not make the link: Bad Request.");
    }
}
