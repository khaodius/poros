//! AWS Signature Version 4, which S3 and every S3-compatible service check on each request.

use ring::digest::{digest, SHA256};
use ring::hmac;

const SERVICE: &str = "s3";
const ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// What goes into a signature, already in canonical form: an escaped path, a sorted and
/// escaped query, and headers with lower-case names, sorted, including `host`.
pub struct Signable<'request> {
    pub method: &'request str,
    pub path: &'request str,
    pub query: &'request str,
    pub headers: &'request [(String, String)],
    pub payload_hash: &'request str,
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(digest(&SHA256, data).as_ref())
}

fn hmac_sha256(key: &[u8], data: &str) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), data.as_bytes())
        .as_ref()
        .to_vec()
}

/// The `Authorization` header for a request made at `date_time` (`20130524T000000Z`).
pub fn authorization(
    access_key: &str,
    secret_key: &str,
    region: &str,
    date_time: &str,
    request: &Signable<'_>,
) -> String {
    let date = &date_time[..8];
    let signed_headers: Vec<&str> = request
        .headers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    let signed_headers = signed_headers.join(";");
    let canonical_headers: String = request
        .headers
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", value.trim()))
        .collect();
    let canonical_request = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{}",
        request.method, request.path, request.query, request.payload_hash
    );
    let scope = format!("{date}/{region}/{SERVICE}/aws4_request");
    let string_to_sign = format!(
        "{ALGORITHM}\n{date_time}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let date_key = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date);
    let region_key = hmac_sha256(&date_key, region);
    let service_key = hmac_sha256(&region_key, SERVICE);
    let signing_key = hmac_sha256(&service_key, "aws4_request");
    let signature = hex(&hmac_sha256(&signing_key, &string_to_sign));
    format!(
        "{ALGORITHM} Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
    const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn matches_the_documented_get_object_example() {
        let headers = headers(&[
            ("host", "examplebucket.s3.amazonaws.com"),
            ("range", "bytes=0-9"),
            ("x-amz-content-sha256", EMPTY_HASH),
            ("x-amz-date", "20130524T000000Z"),
        ]);
        let request = Signable {
            method: "GET",
            path: "/test.txt",
            query: "",
            headers: &headers,
            payload_hash: EMPTY_HASH,
        };
        assert_eq!(
            authorization(
                ACCESS_KEY,
                SECRET_KEY,
                "us-east-1",
                "20130524T000000Z",
                &request
            ),
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
             SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
             Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn matches_the_documented_list_objects_example() {
        let headers = headers(&[
            ("host", "examplebucket.s3.amazonaws.com"),
            ("x-amz-content-sha256", EMPTY_HASH),
            ("x-amz-date", "20130524T000000Z"),
        ]);
        let request = Signable {
            method: "GET",
            path: "/",
            query: "max-keys=2&prefix=J",
            headers: &headers,
            payload_hash: EMPTY_HASH,
        };
        let header = authorization(
            ACCESS_KEY,
            SECRET_KEY,
            "us-east-1",
            "20130524T000000Z",
            &request,
        );
        assert!(header.ends_with(
            "Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"
        ));
    }

    #[test]
    fn matches_the_documented_put_object_example() {
        let payload = sha256_hex(b"Welcome to Amazon S3.");
        let headers = headers(&[
            ("date", "Fri, 24 May 2013 00:00:00 GMT"),
            ("host", "examplebucket.s3.amazonaws.com"),
            ("x-amz-content-sha256", &payload),
            ("x-amz-date", "20130524T000000Z"),
            ("x-amz-storage-class", "REDUCED_REDUNDANCY"),
        ]);
        let request = Signable {
            method: "PUT",
            path: "/test%24file.text",
            query: "",
            headers: &headers,
            payload_hash: &payload,
        };
        let header = authorization(
            ACCESS_KEY,
            SECRET_KEY,
            "us-east-1",
            "20130524T000000Z",
            &request,
        );
        assert!(header.ends_with(
            "Signature=98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd"
        ));
    }
}
