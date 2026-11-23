//! Google Drive's links: a permission on the item for anyone who has its address, or for
//! everyone in the account's organization. The address is the item's own, so the link
//! stops working when the permission is removed.

use async_trait::async_trait;
use percent_encoding::utf8_percent_encode;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;

use super::{DriveFs, FILES_URL};
use crate::cloud::api::{self, Refusal};
use crate::cloud::URL_SEGMENT;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol::{self, RemoteFileSystem};
use crate::share::{
    Audience, Expiry, LinkKind, LinkSharing, ShareCapabilities, ShareLink, ShareOptions,
};

const SHARE_FIELDS: &str =
    "webViewLink,webContentLink,permissions(id,type,role,domain,permissionDetails(inherited))";
const PERMISSION_FIELDS: &str = "id,type,role,domain";
/// Google's own mail domains, which are not organizations.
const PERSONAL_DOMAINS: [&str; 2] = ["gmail.com", "googlemail.com"];
/// The reasons Drive gives when the organization's sharing rules forbid a permission.
const POLICY_REASONS: [&str; 7] = [
    "invalidSharingRequest",
    "shareOutNotPermitted",
    "shareOutNotPermittedToUser",
    "domainPolicy",
    "publishOutNotPermitted",
    "cannotShareTeamDriveTopFolderWithAnyoneOrDomains",
    "teamDrivesSharingRestrictionNotAllowed",
];

/// The organization an account belongs to, from its address; none for a personal account.
pub(super) fn organization_of(email: &str) -> Option<String> {
    let (_, domain) = email.trim().rsplit_once('@')?;
    let domain = domain.to_ascii_lowercase();
    (!domain.is_empty() && !PERSONAL_DOMAINS.contains(&domain.as_str())).then_some(domain)
}

/// The parts of a file that sharing works with.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedFile {
    #[serde(default)]
    web_view_link: Option<String>,
    /// Only files with content of their own have a download address.
    #[serde(default)]
    web_content_link: Option<String>,
    /// Absent when the account may not see who the item is shared with.
    #[serde(default)]
    permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Permission {
    id: String,
    /// `anyone`, `domain`, `user` or `group`.
    #[serde(rename = "type")]
    grantee: String,
    role: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    permission_details: Vec<PermissionDetail>,
}

#[derive(Debug, Clone, Deserialize)]
struct PermissionDetail {
    #[serde(default)]
    inherited: bool,
}

impl Permission {
    /// Who a link permission is for; `None` for one that names a person or a group.
    fn audience(&self) -> Option<Audience> {
        match self.grantee.as_str() {
            "anyone" => Some(Audience::Anyone),
            "domain" => Some(Audience::Organization),
            _ => None,
        }
    }

    fn kind(&self) -> LinkKind {
        match self.role.as_str() {
            "writer" | "fileOrganizer" | "organizer" | "owner" => LinkKind::Edit,
            _ => LinkKind::View,
        }
    }

    /// The permission comes from a folder above and can only be removed there.
    fn is_inherited(&self) -> bool {
        !self.permission_details.is_empty()
            && self
                .permission_details
                .iter()
                .all(|detail| detail.inherited)
    }

    fn is_for(&self, audience: Audience, organization: Option<&str>) -> bool {
        match audience {
            Audience::Anyone => self.grantee == "anyone",
            Audience::Organization => {
                self.grantee == "domain"
                    && self.domain.as_deref().is_some_and(|domain| {
                        organization.is_some_and(|own| own.eq_ignore_ascii_case(domain))
                    })
            }
            Audience::Invited => false,
        }
    }

    /// The link this permission makes of the item's address.
    fn link(&self, url: &str, organization: Option<&str>) -> Option<ShareLink> {
        let audience = self.audience()?;
        let mut notes = Vec::new();
        if let Some(domain) = self.domain.as_deref() {
            if !organization.is_some_and(|own| own.eq_ignore_ascii_case(domain)) {
                notes.push(format!("Works for people signed in at {domain}."));
            }
        }
        if self.is_inherited() {
            notes.push(
                "This link comes from a folder above. Stop sharing that folder to remove it."
                    .to_string(),
            );
        }
        Some(ShareLink {
            id: self.id.clone(),
            url: url.to_string(),
            kind: self.kind(),
            audience,
            expires_at: None,
            has_password: false,
            revocable: !self.is_inherited(),
            notes,
        })
    }
}

/// The permission that makes the link `options` ask for.
fn permission_request(
    options: &ShareOptions,
    organization: Option<&str>,
) -> AppResult<serde_json::Value> {
    let role = match options.kind {
        LinkKind::Edit => "writer",
        LinkKind::View | LinkKind::Download => "reader",
    };
    match (options.audience, organization) {
        (Audience::Anyone, _) => Ok(json!({ "type": "anyone", "role": role })),
        (Audience::Organization, Some(domain)) => {
            Ok(json!({ "type": "domain", "role": role, "domain": domain }))
        }
        _ => Err(AppError::invalid(
            "This account has no organization to limit the link to",
        )),
    }
}

/// Says what to do about a permission Drive would not make or remove.
fn sharing_refusal(refusal: Refusal, audience: Audience, organization: Option<&str>) -> AppError {
    if refusal.has_reason(&["insufficientFilePermissions", "appNotAuthorizedToFile"]) {
        return AppError::new(
            ErrorKind::PermissionDenied,
            "You may not change who this item is shared with. Ask its owner to share it or to make you an editor.",
        );
    }
    if refusal.has_reason(&["sharingRateLimitExceeded"]) {
        return AppError::new(
            ErrorKind::Cloud,
            "Google Drive limits how often items can be shared. Try again later.",
        );
    }
    if refusal.has_reason(&[
        "cannotModifyInheritedTeamDrivePermission",
        "cannotModifyInheritedPermission",
    ]) {
        return AppError::new(
            ErrorKind::PermissionDenied,
            "This link comes from a folder above. Stop sharing that folder to remove it.",
        );
    }
    if !refusal.has_reason(&POLICY_REASONS) {
        return refusal.error;
    }
    let message = match (audience, organization) {
        (Audience::Anyone, Some(domain)) => format!(
            "Your organization does not allow links that anyone can open. Share with people in {domain} instead, or ask an administrator to allow sharing outside it."
        ),
        (Audience::Anyone, None) => format!(
            "Google Drive does not allow a link that anyone can open for this item ({}). Check the sharing settings of the folder it is in.",
            refusal.error.message
        ),
        _ => format!(
            "Your organization does not allow sharing this item with a link ({}). Ask an administrator about the sharing settings.",
            refusal.error.message
        ),
    };
    AppError::new(ErrorKind::PermissionDenied, message)
}

fn permission_url(file_id: &str, permission_id: &str) -> String {
    format!(
        "{FILES_URL}/{}/permissions/{}",
        utf8_percent_encode(file_id, URL_SEGMENT),
        utf8_percent_encode(permission_id, URL_SEGMENT)
    )
}

impl DriveFs {
    async fn shared_file(&self, file_id: &str) -> AppResult<SharedFile> {
        let url = format!("{FILES_URL}/{}", utf8_percent_encode(file_id, URL_SEGMENT));
        self.api
            .json(&|http| http.get(&url).query(&[("fields", SHARE_FIELDS)]))
            .await
    }

    async fn add_permission(&self, file_id: &str, options: &ShareOptions) -> AppResult<Permission> {
        let body = permission_request(options, self.organization.as_deref())?;
        let url = format!(
            "{FILES_URL}/{}/permissions",
            utf8_percent_encode(file_id, URL_SEGMENT)
        );
        let response = self
            .api
            .send_expecting(
                &|http| {
                    http.post(&url)
                        .query(&[("fields", PERMISSION_FIELDS)])
                        .json(&body)
                },
                &[StatusCode::BAD_REQUEST, StatusCode::FORBIDDEN],
            )
            .await?;
        if !response.status().is_success() {
            return Err(sharing_refusal(
                api::refusal(response).await,
                options.audience,
                self.organization.as_deref(),
            ));
        }
        api::decode(response).await
    }

    /// Lets the holders of a view link edit as well.
    async fn make_editable(
        &self,
        file_id: &str,
        permission: &Permission,
        audience: Audience,
    ) -> AppResult<Permission> {
        let url = permission_url(file_id, &permission.id);
        let body = json!({ "role": "writer" });
        let response = self
            .api
            .send_expecting(
                &|http| {
                    http.patch(&url)
                        .query(&[("fields", PERMISSION_FIELDS)])
                        .json(&body)
                },
                &[StatusCode::BAD_REQUEST, StatusCode::FORBIDDEN],
            )
            .await?;
        if !response.status().is_success() {
            return Err(sharing_refusal(
                api::refusal(response).await,
                audience,
                self.organization.as_deref(),
            ));
        }
        api::decode(response).await
    }
}

#[async_trait]
impl LinkSharing for DriveFs {
    fn share_capabilities(&self) -> ShareCapabilities {
        ShareCapabilities {
            kinds: vec![LinkKind::View, LinkKind::Edit, LinkKind::Download],
            audiences: match self.organization {
                Some(_) => vec![Audience::Anyone, Audience::Organization],
                None => vec![Audience::Anyone],
            },
            expiry: Expiry::Never,
            max_lifetime_secs: None,
            password: false,
            revocable: true,
            folders: true,
            organization: self.organization.clone(),
        }
    }

    async fn share_links(&self, path: &str) -> AppResult<Vec<ShareLink>> {
        let path = protocol::normalize(&self.resolve(path));
        let file = self.existing(&path).await?;
        let shared = self.shared_file(&file.id).await?;
        let Some(url) = shared.web_view_link.as_deref() else {
            return Ok(Vec::new());
        };
        Ok(shared
            .permissions
            .iter()
            .filter_map(|permission| permission.link(url, self.organization.as_deref()))
            .collect())
    }

    /// Reuses the link permission the item already has for the audience, so asking twice
    /// does not change who can open it.
    async fn create_share_link(&self, path: &str, options: &ShareOptions) -> AppResult<ShareLink> {
        let path = protocol::normalize(&self.resolve(path));
        if path == "/" {
            return Err(AppError::invalid(
                "My Drive itself cannot be shared. Share a folder inside it.",
            ));
        }
        let file = self.existing(&path).await?;
        if options.kind == LinkKind::Download && (file.is_folder() || file.is_native()) {
            return Err(AppError::invalid(
                "Folders and Google Docs files have no download link. Make a view link instead.",
            )
            .with_path(path));
        }
        let organization = self.organization.as_deref();
        let shared = self.shared_file(&file.id).await?;
        let existing = shared
            .permissions
            .iter()
            .find(|permission| permission.is_for(options.audience, organization));
        let mut notes = Vec::new();
        let permission = match existing {
            Some(permission) if permission.kind() == LinkKind::Edit => {
                if options.kind != LinkKind::Edit {
                    notes.push(
                        "This item already had a link that lets people edit it, which was kept. Stop sharing first to make a link that only shows it."
                            .to_string(),
                    );
                }
                permission.clone()
            }
            Some(permission) if options.kind == LinkKind::Edit => {
                self.make_editable(&file.id, permission, options.audience)
                    .await?
            }
            Some(permission) => permission.clone(),
            None => self.add_permission(&file.id, options).await?,
        };
        let address = match options.kind {
            LinkKind::Download => shared.web_content_link.as_deref(),
            LinkKind::View | LinkKind::Edit => shared.web_view_link.as_deref(),
        };
        let no_address = || {
            AppError::new(
                ErrorKind::Cloud,
                "Google Drive shared the item but gave no address for it",
            )
        };
        let mut link = permission
            .link(address.ok_or_else(no_address)?, organization)
            .ok_or_else(no_address)?;
        if options.kind == LinkKind::Download {
            link.kind = LinkKind::Download;
        }
        notes.append(&mut link.notes);
        link.notes = notes;
        Ok(link)
    }

    async fn remove_share_link(&self, path: &str, link_id: &str) -> AppResult<()> {
        let path = protocol::normalize(&self.resolve(path));
        let file = self.existing(&path).await?;
        let url = permission_url(&file.id, link_id);
        let response = self
            .api
            .send_expecting(
                &|http| http.delete(&url),
                &[
                    StatusCode::BAD_REQUEST,
                    StatusCode::FORBIDDEN,
                    StatusCode::NOT_FOUND,
                ],
            )
            .await?;
        match response.status() {
            // Already removed, here or in the browser.
            status if status.is_success() || status == StatusCode::NOT_FOUND => Ok(()),
            _ => Err(sharing_refusal(
                api::refusal(response).await,
                Audience::Anyone,
                None,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW_URL: &str = "https://drive.google.com/file/d/1AbC/view?usp=drivesdk";

    fn options(kind: LinkKind, audience: Audience) -> ShareOptions {
        ShareOptions {
            kind,
            audience,
            lifetime_secs: None,
            password: None,
        }
    }

    fn shared_file() -> SharedFile {
        serde_json::from_str(
            r#"{
                "webViewLink": "https://drive.google.com/file/d/1AbC/view?usp=drivesdk",
                "webContentLink": "https://drive.google.com/uc?id=1AbC&export=download",
                "permissions": [
                    {"id": "06218121954352358342", "type": "user", "role": "owner"},
                    {"id": "anyoneWithLink", "type": "anyone", "role": "reader"},
                    {"id": "11112222", "type": "domain", "role": "writer", "domain": "example.com"},
                    {"id": "33334444", "type": "domain", "role": "reader", "domain": "partner.example",
                     "permissionDetails": [{"permissionType": "file", "role": "reader", "inherited": true, "inheritedFrom": "0Folder"}]}
                ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn tells_organizations_from_personal_accounts() {
        assert_eq!(
            organization_of("ada@Example.com"),
            Some("example.com".into())
        );
        assert_eq!(organization_of("someone@gmail.com"), None);
        assert_eq!(organization_of("someone@googlemail.com"), None);
        assert_eq!(organization_of("not an address"), None);
        assert_eq!(organization_of("trailing@"), None);
    }

    #[test]
    fn builds_the_permission_a_link_needs() {
        assert_eq!(
            permission_request(&options(LinkKind::View, Audience::Anyone), None).unwrap(),
            json!({ "type": "anyone", "role": "reader" })
        );
        assert_eq!(
            permission_request(&options(LinkKind::Download, Audience::Anyone), None).unwrap(),
            json!({ "type": "anyone", "role": "reader" })
        );
        assert_eq!(
            permission_request(
                &options(LinkKind::Edit, Audience::Organization),
                Some("example.com")
            )
            .unwrap(),
            json!({ "type": "domain", "role": "writer", "domain": "example.com" })
        );
        assert!(
            permission_request(&options(LinkKind::View, Audience::Organization), None).is_err()
        );
        assert_eq!(
            permission_url("1AbC", "anyoneWithLink"),
            "https://www.googleapis.com/drive/v3/files/1AbC/permissions/anyoneWithLink"
        );
        // An id from the page cannot reach another address.
        assert_eq!(
            permission_url("1AbC", "../../other?x=1"),
            "https://www.googleapis.com/drive/v3/files/1AbC/permissions/..%2F..%2Fother%3Fx%3D1"
        );
    }

    #[test]
    fn reads_the_links_an_item_has() {
        let shared = shared_file();
        let links: Vec<ShareLink> = shared
            .permissions
            .iter()
            .filter_map(|permission| permission.link(VIEW_URL, Some("example.com")))
            .collect();
        // The owner's own permission is not a link.
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].id, "anyoneWithLink");
        assert_eq!(links[0].url, VIEW_URL);
        assert_eq!(links[0].kind, LinkKind::View);
        assert_eq!(links[0].audience, Audience::Anyone);
        assert_eq!(links[0].expires_at, None);
        assert!(links[0].revocable && links[0].notes.is_empty());
        assert_eq!(links[1].kind, LinkKind::Edit);
        assert_eq!(links[1].audience, Audience::Organization);
        assert!(links[1].notes.is_empty());
        assert!(!links[2].revocable);
        assert_eq!(
            links[2].notes,
            [
                "Works for people signed in at partner.example.",
                "This link comes from a folder above. Stop sharing that folder to remove it."
            ]
        );
        assert_eq!(
            shared.web_content_link.as_deref(),
            Some("https://drive.google.com/uc?id=1AbC&export=download")
        );
    }

    #[test]
    fn finds_the_permission_to_reuse() {
        let shared = shared_file();
        let found = |audience, organization| {
            shared
                .permissions
                .iter()
                .find(|permission| permission.is_for(audience, organization))
                .map(|permission| permission.id.as_str())
        };
        assert_eq!(found(Audience::Anyone, None), Some("anyoneWithLink"));
        assert_eq!(
            found(Audience::Organization, Some("EXAMPLE.com")),
            Some("11112222")
        );
        assert_eq!(found(Audience::Organization, Some("other.example")), None);
        assert_eq!(found(Audience::Organization, None), None);
        let unshared: SharedFile =
            serde_json::from_str(r#"{"webViewLink":"https://drive.google.com/x"}"#).unwrap();
        assert!(unshared.permissions.is_empty());
        assert_eq!(unshared.web_content_link, None);
    }

    #[test]
    fn says_what_to_do_when_drive_refuses() {
        let policy = br#"{"error":{"code":400,"message":"Bad Request. User message: \"ACL change not allowed.\"","errors":[{"domain":"global","reason":"invalidSharingRequest","message":"Bad Request"}]}}"#;
        let with_organization = sharing_refusal(
            api::refusal_from(StatusCode::BAD_REQUEST, policy),
            Audience::Anyone,
            Some("example.com"),
        );
        assert_eq!(with_organization.kind, ErrorKind::PermissionDenied);
        assert_eq!(
            with_organization.message,
            "Your organization does not allow links that anyone can open. Share with people in example.com instead, or ask an administrator to allow sharing outside it."
        );
        let personal = sharing_refusal(
            api::refusal_from(StatusCode::BAD_REQUEST, policy),
            Audience::Anyone,
            None,
        );
        assert!(personal.message.contains("ACL change not allowed"));
        let not_allowed = br#"{"error":{"code":403,"message":"The user does not have sufficient permissions for this file.","errors":[{"reason":"insufficientFilePermissions"}]}}"#;
        let owner = sharing_refusal(
            api::refusal_from(StatusCode::FORBIDDEN, not_allowed),
            Audience::Anyone,
            None,
        );
        assert!(owner
            .message
            .starts_with("You may not change who this item is shared with"));
        let other = sharing_refusal(
            api::refusal_from(
                StatusCode::FORBIDDEN,
                br#"{"error":{"code":403,"message":"Something else"}}"#,
            ),
            Audience::Anyone,
            None,
        );
        assert_eq!(other.message, "Something else");
        assert_eq!(other.kind, ErrorKind::PermissionDenied);
    }
}
