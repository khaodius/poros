//! The `207 Multi-Status` answers to `PROPFIND` and `PROPPATCH`. Elements are matched by local
//! name, so whichever prefix a server binds to `DAV:` reads the same.

use percent_encoding::percent_decode_str;
use reqwest::Url;
use serde::de::IgnoredAny;
use serde::Deserialize;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol;
use crate::timestamp;

#[derive(Debug, Deserialize)]
struct Multistatus {
    #[serde(rename = "response", default)]
    responses: Vec<DavResponse>,
}

#[derive(Debug, Deserialize)]
struct DavResponse {
    #[serde(default)]
    href: String,
    #[serde(rename = "propstat", default)]
    propstats: Vec<Propstat>,
}

#[derive(Debug, Deserialize)]
struct Propstat {
    #[serde(default)]
    prop: Prop,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Default, Deserialize)]
struct Prop {
    #[serde(default)]
    getcontentlength: Option<String>,
    #[serde(default)]
    getlastmodified: Option<String>,
    #[serde(default)]
    resourcetype: Option<ResourceType>,
}

#[derive(Debug, Default, Deserialize)]
struct ResourceType {
    #[serde(default)]
    collection: Option<IgnoredAny>,
}

/// The `<d:error>` body some servers explain a refusal with, as Nextcloud does in
/// `<s:message>`.
#[derive(Debug, Default, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    message: String,
}

/// One file or folder from a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DavItem {
    /// Decoded and normalized: no trailing slash, `/` for the root.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
}

fn is_ok(status: &str) -> bool {
    status
        .split_whitespace()
        .nth(1)
        .is_some_and(|code| code.starts_with('2'))
}

fn not_webdav() -> AppError {
    AppError::new(
        ErrorKind::InvalidInput,
        "The server did not answer as a WebDAV server; check the address and the remote folder",
    )
}

/// The items of a `PROPFIND` answer.
pub fn parse_listing(body: &[u8]) -> AppResult<Vec<DavItem>> {
    let text = std::str::from_utf8(body).map_err(|_| not_webdav())?;
    let multistatus: Multistatus = quick_xml::de::from_str(text).map_err(|_| not_webdav())?;
    Ok(multistatus
        .responses
        .into_iter()
        .filter_map(|response| {
            let path = href_path(&response.href)?;
            let mut item = DavItem {
                is_dir: response.href.ends_with('/'),
                path,
                size: 0,
                modified: None,
            };
            for propstat in response
                .propstats
                .iter()
                .filter(|propstat| is_ok(&propstat.status))
            {
                let prop = &propstat.prop;
                if let Some(resource_type) = &prop.resourcetype {
                    item.is_dir = resource_type.collection.is_some();
                }
                if let Some(length) = &prop.getcontentlength {
                    item.size = length.trim().parse().unwrap_or(0);
                }
                if let Some(modified) = &prop.getlastmodified {
                    item.modified = timestamp::parse_http_date(modified);
                }
            }
            if item.is_dir {
                item.size = 0;
            }
            Some(item)
        })
        .collect())
}

/// Whether a `PROPPATCH` answer set every property it named.
pub fn all_properties_set(body: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(body) else {
        return false;
    };
    match quick_xml::de::from_str::<Multistatus>(text) {
        Ok(multistatus) => multistatus.responses.iter().all(|response| {
            response
                .propstats
                .iter()
                .all(|propstat| is_ok(&propstat.status))
        }),
        Err(_) => false,
    }
}

/// The explanation in an error body, if the server sent one.
pub fn error_message(body: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    let error: ErrorBody = quick_xml::de::from_str(text).ok()?;
    let message = error.message.trim();
    (!message.is_empty()).then(|| message.to_string())
}

/// The path an `href` names, which may be a full URL or an absolute path, percent-encoded.
fn href_path(href: &str) -> Option<String> {
    let href = href.trim();
    let encoded = if href.starts_with("http://") || href.starts_with("https://") {
        Url::parse(href).ok()?.path().to_string()
    } else {
        href.split(['?', '#']).next()?.to_string()
    };
    let decoded = percent_decode_str(&encoded).decode_utf8_lossy();
    Some(protocol::normalize(&decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEXTCLOUD: &str = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:oc="http://owncloud.org/ns">
 <d:response>
  <d:href>/remote.php/dav/files/ada/</d:href>
  <d:propstat>
   <d:prop><d:resourcetype><d:collection/></d:resourcetype><d:getlastmodified>Tue, 05 Mar 2024 10:00:00 GMT</d:getlastmodified></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
  <d:propstat>
   <d:prop><d:getcontentlength/></d:prop>
   <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/files/ada/Notes%20%26%20Ideas.md</d:href>
  <d:propstat>
   <d:prop><d:resourcetype/><d:getcontentlength>1234</d:getcontentlength><d:getlastmodified>Wed, 06 Mar 2024 11:30:15 GMT</d:getlastmodified></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/files/ada/Photos/</d:href>
  <d:propstat>
   <d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
</d:multistatus>"#;

    #[test]
    fn reads_a_nextcloud_listing() {
        let items = parse_listing(NEXTCLOUD.as_bytes()).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].path, "/remote.php/dav/files/ada");
        assert!(items[0].is_dir);
        assert_eq!(
            items[1],
            DavItem {
                path: "/remote.php/dav/files/ada/Notes & Ideas.md".into(),
                is_dir: false,
                size: 1234,
                modified: timestamp::parse_http_date("Wed, 06 Mar 2024 11:30:15 GMT"),
            }
        );
        assert!(items[2].is_dir);
        assert_eq!(items[2].path, "/remote.php/dav/files/ada/Photos");
    }

    #[test]
    fn reads_other_prefixes_full_urls_and_entities() {
        let apache = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:ns0="DAV:">
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/">
<D:href>http://files.example.com/share/a&amp;b.txt</D:href>
<D:propstat>
<D:prop>
<lp1:resourcetype/>
<lp1:getcontentlength>7</lp1:getcontentlength>
<lp1:getlastmodified>Sun, 06 Nov 1994 08:49:37 GMT</lp1:getlastmodified>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>
</D:multistatus>"#;
        let items = parse_listing(apache.as_bytes()).unwrap();
        assert_eq!(items[0].path, "/share/a&b.txt");
        assert_eq!(items[0].size, 7);
        assert_eq!(items[0].modified, Some(784_111_777));
    }

    #[test]
    fn falls_back_to_the_trailing_slash_and_rejects_other_answers() {
        let bare = r#"<multistatus xmlns="DAV:"><response><href>/a/</href></response><response><href>/b</href></response></multistatus>"#;
        let items = parse_listing(bare.as_bytes()).unwrap();
        assert!(items[0].is_dir && !items[1].is_dir);
        assert!(
            parse_listing(b"<html><body>Login</body></html>").is_ok_and(|items| items.is_empty())
        );
        assert!(parse_listing(b"not xml at all <").is_err());
    }

    #[test]
    fn checks_property_updates() {
        let refused = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/f</d:href><d:propstat><d:prop><d:lastmodified/></d:prop><d:status>HTTP/1.1 403 Forbidden</d:status></d:propstat></d:response></d:multistatus>"#;
        assert!(!all_properties_set(refused.as_bytes()));
        let accepted = refused.replace("403 Forbidden", "200 OK");
        assert!(all_properties_set(accepted.as_bytes()));
    }

    #[test]
    fn reads_error_messages() {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:error xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns">
  <s:exception>Sabre\DAV\Exception\InsufficientStorage</s:exception>
  <s:message>Insufficient space in /ada</s:message>
</d:error>"#;
        assert_eq!(
            error_message(body.as_bytes()).as_deref(),
            Some("Insufficient space in /ada")
        );
        assert_eq!(error_message(b"<html/>"), None);
    }
}
