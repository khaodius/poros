//! The XML documents S3 answers with and the few it takes.

use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::timestamp;
use crate::versions::{HistoryRecord, ObjectVersion, VersionKind};

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ErrorBody {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    /// Where the bucket is, when a request went to the wrong region.
    #[serde(default)]
    pub region: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListAllMyBucketsResult {
    #[serde(default)]
    pub buckets: Buckets,
}

#[derive(Debug, Default, Deserialize)]
pub struct Buckets {
    #[serde(rename = "Bucket", default)]
    pub buckets: Vec<Bucket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Bucket {
    pub name: String,
    #[serde(default)]
    pub creation_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListBucketResult {
    #[serde(default)]
    pub is_truncated: bool,
    #[serde(rename = "Contents", default)]
    pub contents: Vec<Object>,
    #[serde(rename = "CommonPrefixes", default)]
    pub common_prefixes: Vec<CommonPrefix>,
    #[serde(default)]
    pub next_continuation_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Object {
    pub key: String,
    #[serde(default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommonPrefix {
    pub prefix: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InitiateMultipartUploadResult {
    pub upload_id: String,
}

/// The answer to copying an object or a part of one.
#[derive(Debug, Deserialize)]
pub struct CopyResult {
    #[serde(rename = "ETag", default)]
    pub etag: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct DeleteResult {
    #[serde(rename = "Error", default)]
    pub errors: Vec<DeleteError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeleteError {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
}

/// The answer to `GetBucketVersioning`. A bucket that never had versioning sends neither.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VersioningConfiguration {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub mfa_delete: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ListVersionsResult {
    #[serde(default)]
    is_truncated: bool,
    #[serde(rename = "Version", default)]
    versions: Vec<VersionEntry>,
    #[serde(rename = "DeleteMarker", default)]
    delete_markers: Vec<VersionEntry>,
    #[serde(rename = "CommonPrefixes", default)]
    common_prefixes: Vec<CommonPrefix>,
    #[serde(default)]
    next_key_marker: Option<String>,
    #[serde(default)]
    next_version_id_marker: Option<String>,
    /// `url` when the keys in the answer are percent-encoded, as the request asked.
    #[serde(default)]
    encoding_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct VersionEntry {
    key: String,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    is_latest: bool,
    #[serde(default)]
    last_modified: Option<String>,
    #[serde(rename = "ETag", default)]
    etag: Option<String>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    storage_class: Option<String>,
}

/// Where the next page of a listing of versions starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionMarker {
    pub key: String,
    /// Empty when the service named none, which starts at the key's newest version.
    pub version_id: String,
}

/// One page of `ListObjectVersions`, keys decoded.
#[derive(Debug, Default)]
pub struct VersionPage {
    /// Versions and delete markers in the order the service sent them.
    pub records: Vec<HistoryRecord>,
    /// The prefixes a delimiter grouped deeper keys into, each ending with it.
    pub folders: Vec<String>,
    /// `None` on the last page.
    pub next: Option<VersionMarker>,
}

/// Reads a page of `ListObjectVersions`. An empty answer, as some services without versioning
/// give, is a page with nothing on it.
pub fn version_page(body: &[u8]) -> AppResult<VersionPage> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(VersionPage::default());
    }
    let listing: ListVersionsResult = parse(body)?;
    let encoded = listing
        .encoding_type
        .as_deref()
        .is_some_and(|encoding| encoding.eq_ignore_ascii_case("url"));
    let decode = |text: String| if encoded { url_decode(&text) } else { text };
    let order = element_order(std::str::from_utf8(body).unwrap_or_default());
    let record = |entry: VersionEntry, kind: VersionKind| HistoryRecord {
        key: decode(entry.key),
        version: ObjectVersion {
            // Objects from before versioning have the version `null`, which some services
            // leave out.
            version_id: entry
                .version_id
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| NULL_VERSION.to_string()),
            kind,
            latest: entry.is_latest,
            modified: entry
                .last_modified
                .as_deref()
                .and_then(timestamp::parse_rfc3339),
            size: entry.size,
            etag: entry
                .etag
                .map(|etag| etag.trim_matches('"').to_string())
                .filter(|etag| !etag.is_empty()),
            storage_class: entry.storage_class.filter(|class| !class.is_empty()),
        },
    };
    let mut versions = listing
        .versions
        .into_iter()
        .map(|entry| record(entry, VersionKind::Version));
    let mut markers = listing
        .delete_markers
        .into_iter()
        .map(|entry| record(entry, VersionKind::DeleteMarker));
    let mut records = Vec::with_capacity(versions.len() + markers.len());
    let in_document_order = order.iter().filter(|is_marker| !**is_marker).count() == versions.len()
        && order.len() == versions.len() + markers.len();
    if in_document_order {
        for is_marker in order {
            records.extend(if is_marker {
                markers.next()
            } else {
                versions.next()
            });
        }
    }
    records.extend(versions);
    records.extend(markers);
    let next = match (listing.is_truncated, listing.next_key_marker) {
        (true, Some(key)) if !key.is_empty() => Some(VersionMarker {
            key: decode(key),
            version_id: listing.next_version_id_marker.unwrap_or_default(),
        }),
        // A service that names no marker continues after the last entry it sent.
        (true, _) => records.last().map(|last| VersionMarker {
            key: last.key.clone(),
            version_id: last.version.version_id.clone(),
        }),
        (false, _) => None,
    };
    Ok(VersionPage {
        records,
        folders: listing
            .common_prefixes
            .into_iter()
            .map(|folder| decode(folder.prefix))
            .collect(),
        next,
    })
}

/// The version id of an object written before versioning was turned on, or while suspended.
pub const NULL_VERSION: &str = "null";

/// Whether each `<Version>` and `<DeleteMarker>` element of a listing is a marker, in the order
/// they come in. Reading the two kinds into separate lists loses how they interleave, which on
/// Amazon is each key's history, newest first.
fn element_order(text: &str) -> Vec<bool> {
    text.match_indices('<')
        .filter_map(|(start, _)| {
            let rest = &text[start + 1..];
            if rest.starts_with("Version>") {
                Some(false)
            } else if rest.starts_with("DeleteMarker>") {
                Some(true)
            } else {
                None
            }
        })
        .collect()
}

/// Decodes a key from a listing asked for with `encoding-type=url`, where a space comes as `+`
/// and other bytes as `%XX`.
fn url_decode(text: &str) -> String {
    let spaced = text.replace('+', " ");
    percent_encoding::percent_decode_str(&spaced)
        .decode_utf8_lossy()
        .into_owned()
}

pub fn parse<T: DeserializeOwned>(body: &[u8]) -> AppResult<T> {
    let text = std::str::from_utf8(body)
        .map_err(|_| AppError::new(ErrorKind::Cloud, "S3 sent an answer that is not text"))?;
    quick_xml::de::from_str(text).map_err(|error| {
        AppError::new(
            ErrorKind::Cloud,
            format!("Unexpected answer from S3: {error}"),
        )
    })
}

/// The error a `200 OK` may still carry: S3 reports some failures of copies and multipart
/// uploads only in the body, after the status has gone out.
pub fn embedded_error(body: &[u8]) -> Option<ErrorBody> {
    let text = std::str::from_utf8(body).ok()?;
    let start = text.find('<')?;
    let after_declaration = match text[start..].strip_prefix("<?") {
        Some(_) => &text[start + text[start..].find("?>")? + 2..],
        None => &text[start..],
    };
    if !after_declaration.trim_start().starts_with("<Error") {
        return None;
    }
    quick_xml::de::from_str(text).ok()
}

/// Escapes text for an XML element.
pub fn escape(text: &str) -> String {
    quick_xml::escape::escape(text).into_owned()
}

/// The body that finishes a multipart upload from its parts' ETags.
pub fn complete_multipart(parts: &[String]) -> String {
    let parts: String = parts
        .iter()
        .enumerate()
        .map(|(index, etag)| {
            format!(
                "<Part><PartNumber>{}</PartNumber><ETag>{}</ETag></Part>",
                index + 1,
                escape(etag)
            )
        })
        .collect();
    format!("<CompleteMultipartUpload>{parts}</CompleteMultipartUpload>")
}

/// The body of a `DeleteObjects` request for up to 1000 keys.
pub fn delete_objects(keys: &[String]) -> String {
    let objects: String = keys
        .iter()
        .map(|key| format!("<Object><Key>{}</Key></Object>", escape(key)))
        .collect();
    format!("<Delete><Quiet>true</Quiet>{objects}</Delete>")
}

/// The body of a `DeleteObjects` request that removes up to 1000 versions for good, each a key
/// and a version id.
pub fn delete_versions(versions: &[(String, String)]) -> String {
    let objects: String = versions
        .iter()
        .map(|(key, version_id)| {
            format!(
                "<Object><Key>{}</Key><VersionId>{}</VersionId></Object>",
                escape(key),
                escape(version_id)
            )
        })
        .collect();
    format!("<Delete><Quiet>true</Quiet>{objects}</Delete>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_listings_with_folders() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>photos</Name><Prefix>2024/</Prefix><KeyCount>3</KeyCount><MaxKeys>1000</MaxKeys>
  <Delimiter>/</Delimiter><IsTruncated>true</IsTruncated>
  <Contents><Key>2024/</Key><LastModified>2024-03-01T10:00:00.000Z</LastModified><ETag>&quot;d41d8cd98f00b204e9800998ecf8427e&quot;</ETag><Size>0</Size><StorageClass>STANDARD</StorageClass></Contents>
  <CommonPrefixes><Prefix>2024/march/</Prefix></CommonPrefixes>
  <Contents><Key>2024/a+b&amp;c.jpg</Key><LastModified>2024-03-02T10:00:00.000Z</LastModified><Size>2048</Size></Contents>
  <NextContinuationToken>1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=</NextContinuationToken>
</ListBucketResult>"#;
        let listing: ListBucketResult = parse(body.as_bytes()).unwrap();
        assert!(listing.is_truncated);
        assert_eq!(listing.contents.len(), 2);
        assert_eq!(listing.contents[1].key, "2024/a+b&c.jpg");
        assert_eq!(listing.contents[1].size, 2048);
        assert_eq!(listing.common_prefixes[0].prefix, "2024/march/");
        assert_eq!(
            listing.next_continuation_token.as_deref(),
            Some("1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=")
        );
    }

    #[test]
    fn reads_bucket_lists_and_errors() {
        let buckets = r#"<ListAllMyBucketsResult><Owner><ID>x</ID></Owner><Buckets><Bucket><Name>one</Name><CreationDate>2020-01-01T00:00:00.000Z</CreationDate></Bucket><Bucket><Name>two</Name></Bucket></Buckets></ListAllMyBucketsResult>"#;
        let list: ListAllMyBucketsResult = parse(buckets.as_bytes()).unwrap();
        assert_eq!(list.buckets.buckets.len(), 2);
        assert_eq!(list.buckets.buckets[1].name, "two");
        let empty: ListAllMyBucketsResult =
            parse(b"<ListAllMyBucketsResult><Buckets/></ListAllMyBucketsResult>").unwrap();
        assert!(empty.buckets.buckets.is_empty());

        let error = r#"<?xml version="1.0" encoding="UTF-8"?>
<Error><Code>AuthorizationHeaderMalformed</Code><Message>the region 'us-east-1' is wrong; expecting 'eu-west-1'</Message><Region>eu-west-1</Region></Error>"#;
        let parsed = embedded_error(error.as_bytes()).unwrap();
        assert_eq!(parsed.code, "AuthorizationHeaderMalformed");
        assert_eq!(parsed.region, "eu-west-1");
        assert!(embedded_error(
            b"<CompleteMultipartUploadResult><ETag>x</ETag></CompleteMultipartUploadResult>"
        )
        .is_none());
    }

    #[test]
    fn writes_request_bodies() {
        assert_eq!(
            complete_multipart(&["\"a\"".into(), "\"b\"".into()]),
            "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>&quot;a&quot;</ETag></Part><Part><PartNumber>2</PartNumber><ETag>&quot;b&quot;</ETag></Part></CompleteMultipartUpload>"
        );
        assert_eq!(
            delete_objects(&["a&b".into()]),
            "<Delete><Quiet>true</Quiet><Object><Key>a&amp;b</Key></Object></Delete>"
        );
        let result: DeleteResult = parse(
            b"<DeleteResult><Error><Key>k</Key><Code>AccessDenied</Code><Message>no</Message></Error></DeleteResult>",
        )
        .unwrap();
        assert_eq!(result.errors[0].code, "AccessDenied");
        let copied: CopyResult =
            parse(b"<CopyObjectResult><LastModified>x</LastModified><ETag>\"e\"</ETag></CopyObjectResult>")
                .unwrap();
        assert_eq!(copied.etag, "\"e\"");
        assert_eq!(
            delete_versions(&[("a&b".into(), "null".into()), ("c".into(), "v<1>".into())]),
            "<Delete><Quiet>true</Quiet><Object><Key>a&amp;b</Key><VersionId>null</VersionId></Object><Object><Key>c</Key><VersionId>v&lt;1&gt;</VersionId></Object></Delete>"
        );
        let refused: DeleteResult = parse(
            b"<DeleteResult><Error><Key>k</Key><VersionId>v1</VersionId><Code>AccessDenied</Code><Message>Mfa Authentication must be provided in the request.</Message></Error></DeleteResult>",
        )
        .unwrap();
        assert_eq!(refused.errors[0].version_id.as_deref(), Some("v1"));
    }

    fn summary(record: &HistoryRecord) -> (&str, &str, bool, bool) {
        (
            record.key.as_str(),
            record.version.version_id.as_str(),
            record.version.is_marker(),
            record.version.latest,
        )
    }

    #[test]
    fn reads_a_truncated_page_of_versions_in_document_order() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListVersionsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>photos</Name><Prefix>2024/</Prefix><KeyMarker></KeyMarker><VersionIdMarker></VersionIdMarker>
  <NextKeyMarker>2024/b.txt</NextKeyMarker><NextVersionIdMarker>b-1</NextVersionIdMarker>
  <MaxKeys>5</MaxKeys><Delimiter>/</Delimiter><IsTruncated>true</IsTruncated>
  <DeleteMarker><Key>2024/a.txt</Key><VersionId>a-3</VersionId><IsLatest>true</IsLatest><LastModified>2024-03-03T10:00:00.000Z</LastModified><Owner><ID>x</ID></Owner></DeleteMarker>
  <Version><Key>2024/a.txt</Key><VersionId>a-2</VersionId><IsLatest>false</IsLatest><LastModified>2024-03-02T10:00:00.000Z</LastModified><ETag>&quot;9b2cf535f27731c974343645a3985328&quot;</ETag><Size>2048</Size><StorageClass>STANDARD_IA</StorageClass><Owner><ID>x</ID></Owner></Version>
  <DeleteMarker><Key>2024/a.txt</Key><VersionId>a-1</VersionId><IsLatest>false</IsLatest><LastModified>2024-03-01T12:00:00.000Z</LastModified></DeleteMarker>
  <Version><Key>2024/a.txt</Key><VersionId>null</VersionId><IsLatest>false</IsLatest><LastModified>2024-03-01T10:00:00.000Z</LastModified><ETag>&quot;d41d8cd98f00b204e9800998ecf8427e&quot;</ETag><Size>0</Size><StorageClass>STANDARD</StorageClass></Version>
  <Version><Key>2024/b.txt</Key><VersionId>b-1</VersionId><IsLatest>true</IsLatest><LastModified>2024-03-01T10:00:00.000Z</LastModified><ETag>&quot;e&quot;</ETag><Size>7</Size></Version>
  <CommonPrefixes><Prefix>2024/march/</Prefix></CommonPrefixes>
</ListVersionsResult>"#;
        let page = version_page(body.as_bytes()).unwrap();
        let records: Vec<_> = page.records.iter().map(summary).collect();
        assert_eq!(
            records,
            [
                ("2024/a.txt", "a-3", true, true),
                ("2024/a.txt", "a-2", false, false),
                ("2024/a.txt", "a-1", true, false),
                ("2024/a.txt", "null", false, false),
                ("2024/b.txt", "b-1", false, true),
            ]
        );
        let second = &page.records[1].version;
        assert_eq!(second.size, 2048);
        assert_eq!(second.modified, Some(1_709_373_600));
        assert_eq!(
            second.etag.as_deref(),
            Some("9b2cf535f27731c974343645a3985328")
        );
        assert_eq!(second.storage_class.as_deref(), Some("STANDARD_IA"));
        assert_eq!(page.records[0].version.etag, None);
        assert_eq!(page.folders, ["2024/march/"]);
        assert_eq!(
            page.next,
            Some(VersionMarker {
                key: "2024/b.txt".into(),
                version_id: "b-1".into()
            })
        );
    }

    #[test]
    fn decodes_keys_only_when_the_answer_says_they_are_encoded() {
        let encoded = r#"<ListVersionsResult>
  <Prefix>dir+a/</Prefix><EncodingType>url</EncodingType><IsTruncated>true</IsTruncated>
  <NextKeyMarker>dir+a/na%2Bme+%C3%A9%26.txt</NextKeyMarker><NextVersionIdMarker>null</NextVersionIdMarker>
  <Version><Key>dir+a/na%2Bme+%C3%A9%26.txt</Key><VersionId>null</VersionId><IsLatest>true</IsLatest><LastModified>2024-03-01T10:00:00.000Z</LastModified><Size>1</Size></Version>
  <Version><Key>dir+a/line%0Abreak</Key><VersionId>v</VersionId><IsLatest>true</IsLatest><Size>1</Size></Version>
  <CommonPrefixes><Prefix>dir+a/sub+folder/</Prefix></CommonPrefixes>
</ListVersionsResult>"#;
        let page = version_page(encoded.as_bytes()).unwrap();
        assert_eq!(page.records[0].key, "dir a/na+me \u{e9}&.txt");
        assert_eq!(page.records[1].key, "dir a/line\nbreak");
        assert_eq!(page.folders, ["dir a/sub folder/"]);
        assert_eq!(page.next.unwrap().key, "dir a/na+me \u{e9}&.txt");

        // A service that ignores `encoding-type` sends the keys as they are.
        let plain = r#"<ListVersionsResult><IsTruncated>false</IsTruncated>
  <Version><Key>dir a/na+me%20&amp;.txt</Key><VersionId>v</VersionId><IsLatest>true</IsLatest><Size>1</Size></Version>
</ListVersionsResult>"#;
        let page = version_page(plain.as_bytes()).unwrap();
        assert_eq!(page.records[0].key, "dir a/na+me%20&.txt");
        assert_eq!(page.next, None);
    }

    #[test]
    fn reads_listings_that_group_markers_after_versions_or_name_no_marker() {
        // Versions first, then delete markers, with no version id on an object from before
        // versioning and no marker for the next page.
        let body = r#"<ListVersionsResult><IsTruncated>true</IsTruncated>
  <Version><Key>a.txt</Key><IsLatest>false</IsLatest><LastModified>2024-03-01T10:00:00Z</LastModified><Size>3</Size></Version>
  <Version><Key>b.txt</Key><VersionId>b-1</VersionId><IsLatest>true</IsLatest><Size>3</Size></Version>
  <DeleteMarker><Key>a.txt</Key><VersionId>a-2</VersionId><IsLatest>true</IsLatest><LastModified>2024-03-02T10:00:00Z</LastModified></DeleteMarker>
</ListVersionsResult>"#;
        let page = version_page(body.as_bytes()).unwrap();
        let records: Vec<_> = page.records.iter().map(summary).collect();
        assert_eq!(
            records,
            [
                ("a.txt", "null", false, false),
                ("b.txt", "b-1", false, true),
                ("a.txt", "a-2", true, true),
            ]
        );
        assert_eq!(
            page.next,
            Some(VersionMarker {
                key: "a.txt".into(),
                version_id: "a-2".into()
            })
        );
    }

    #[test]
    fn reads_empty_listings_and_versioning_answers() {
        assert!(version_page(b"").unwrap().records.is_empty());
        assert!(version_page(b"  \n").unwrap().next.is_none());
        let none = version_page(
            b"<ListVersionsResult><Name>b</Name><IsTruncated>false</IsTruncated></ListVersionsResult>",
        )
        .unwrap();
        assert!(none.records.is_empty() && none.folders.is_empty() && none.next.is_none());
        // A service without versioning may answer with a plain listing of objects.
        let objects = version_page(
            b"<ListBucketResult><IsTruncated>false</IsTruncated><Contents><Key>a</Key><Size>1</Size></Contents></ListBucketResult>",
        )
        .unwrap();
        assert!(objects.records.is_empty());
        assert!(version_page(b"<html>not a listing").is_err());

        let enabled: VersioningConfiguration = parse(
            br#"<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Status>Enabled</Status><MfaDelete>Enabled</MfaDelete></VersioningConfiguration>"#,
        )
        .unwrap();
        assert_eq!(enabled.status.as_deref(), Some("Enabled"));
        assert_eq!(enabled.mfa_delete.as_deref(), Some("Enabled"));
        let never: VersioningConfiguration = parse(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"/>"#,
        )
        .unwrap();
        assert_eq!(never.status, None);
        assert_eq!(never.mfa_delete, None);
    }
}
