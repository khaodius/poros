//! The XML documents S3 answers with and the few it takes.

use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::error::{AppError, AppResult, ErrorKind};

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
    pub code: String,
    #[serde(default)]
    pub message: String,
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
    }
}
