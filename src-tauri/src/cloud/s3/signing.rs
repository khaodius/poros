//! AWS Signature Version 4, which S3 and every S3-compatible service check on each request,
//! either in the `Authorization` header or, for an address handed to someone else, in the
//! query.

use ring::digest::{digest, SHA256};
use ring::hmac;

const SERVICE: &str = "s3";
const ALGORITHM: &str = "AWS4-HMAC-SHA256";
/// Stands in for the body's hash in an address signed ahead of time.
const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

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

/// Escapes a query name or value the way the signature expects: everything but unreserved
/// characters.
pub fn escape(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, crate::cloud::URL_SEGMENT).to_string()
}

/// A query sorted and escaped, as both the address and the signature have it.
pub fn canonical_query(parameters: &[(String, String)]) -> String {
    let mut pairs: Vec<(String, String)> = parameters
        .iter()
        .map(|(name, value)| (escape(name), escape(value)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// What a signature is valid for: the day, the region and the service.
fn scope(region: &str, date_time: &str) -> String {
    format!("{}/{region}/{SERVICE}/aws4_request", &date_time[..8])
}

fn signed_header_names(headers: &[(String, String)]) -> String {
    let names: Vec<&str> = headers.iter().map(|(name, _)| name.as_str()).collect();
    names.join(";")
}

/// The signature of a request made at `date_time` (`20130524T000000Z`).
fn signature(secret_key: &str, region: &str, date_time: &str, request: &Signable<'_>) -> String {
    let canonical_headers: String = request
        .headers
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", value.trim()))
        .collect();
    let canonical_request = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{}\n{}",
        request.method,
        request.path,
        request.query,
        signed_header_names(request.headers),
        request.payload_hash
    );
    let string_to_sign = format!(
        "{ALGORITHM}\n{date_time}\n{}\n{}",
        scope(region, date_time),
        sha256_hex(canonical_request.as_bytes())
    );
    let date_key = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), &date_time[..8]);
    let region_key = hmac_sha256(&date_key, region);
    let service_key = hmac_sha256(&region_key, SERVICE);
    let signing_key = hmac_sha256(&service_key, "aws4_request");
    hex(&hmac_sha256(&signing_key, &string_to_sign))
}

/// The `Authorization` header for a request made at `date_time` (`20130524T000000Z`).
pub fn authorization(
    access_key: &str,
    secret_key: &str,
    region: &str,
    date_time: &str,
    request: &Signable<'_>,
) -> String {
    format!(
        "{ALGORITHM} Credential={access_key}/{}, SignedHeaders={}, Signature={}",
        scope(region, date_time),
        signed_header_names(request.headers),
        signature(secret_key, region, date_time, request)
    )
}

/// Who signs an address ahead of time, when, and for how long.
pub struct Presigning<'keys> {
    pub access_key: &'keys str,
    pub secret_key: &'keys str,
    /// Temporary credentials come with a token the address has to carry.
    pub session_token: Option<&'keys str>,
    pub region: &'keys str,
    /// When the address starts to work (`20130524T000000Z`).
    pub date_time: &'keys str,
    /// Seconds after `date_time` the address stops working: 1 to 604800, a week.
    pub expires_secs: u64,
}

/// The query of an address that carries its own signature, so whoever holds it can make
/// the request without credentials. `host` is the `host` header the request will have,
/// `path` the escaped path, and `parameters` what the request itself asks for, unescaped.
pub fn presigned_query(
    presigning: &Presigning<'_>,
    method: &str,
    host: &str,
    path: &str,
    parameters: &[(String, String)],
) -> String {
    let headers = [("host".to_string(), host.to_string())];
    let mut parameters = parameters.to_vec();
    let mut add = |name: &str, value: String| parameters.push((name.to_string(), value));
    add("X-Amz-Algorithm", ALGORITHM.to_string());
    add(
        "X-Amz-Credential",
        format!(
            "{}/{}",
            presigning.access_key,
            scope(presigning.region, presigning.date_time)
        ),
    );
    add("X-Amz-Date", presigning.date_time.to_string());
    add("X-Amz-Expires", presigning.expires_secs.to_string());
    add("X-Amz-SignedHeaders", signed_header_names(&headers));
    if let Some(session_token) = presigning.session_token {
        add("X-Amz-Security-Token", session_token.to_string());
    }
    let query = canonical_query(&parameters);
    let signature = signature(
        presigning.secret_key,
        presigning.region,
        presigning.date_time,
        &Signable {
            method,
            path,
            query: &query,
            headers: &headers,
            payload_hash: UNSIGNED_PAYLOAD,
        },
    );
    format!("{query}&X-Amz-Signature={signature}")
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

    fn presigning(region: &'static str, expires_secs: u64) -> Presigning<'static> {
        Presigning {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            session_token: None,
            region,
            date_time: "20130524T000000Z",
            expires_secs,
        }
    }

    /// The example in "Authenticating Requests: Using Query Parameters" of the S3 API
    /// reference.
    #[test]
    fn matches_the_documented_presigned_url_example() {
        let query = presigned_query(
            &presigning("us-east-1", 86400),
            "GET",
            "examplebucket.s3.amazonaws.com",
            "/test.txt",
            &[],
        );
        assert_eq!(
            query,
            "X-Amz-Algorithm=AWS4-HMAC-SHA256\
             &X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request\
             &X-Amz-Date=20130524T000000Z\
             &X-Amz-Expires=86400\
             &X-Amz-SignedHeaders=host\
             &X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
        );
    }

    /// The remaining signatures are the ones botocore, Amazon's own client library, makes
    /// for the same input.
    #[test]
    fn signs_a_session_token_into_the_query() {
        let with_token = Presigning {
            session_token: Some("FQoGZXIvYXdzEJr//////////wEaDKX+token/with+chars=="),
            ..presigning("eu-west-1", 900)
        };
        let query = presigned_query(
            &with_token,
            "GET",
            "my-bucket.s3.eu-west-1.amazonaws.com",
            "/folder/file%20name.txt",
            &[],
        );
        assert_eq!(
            query,
            "X-Amz-Algorithm=AWS4-HMAC-SHA256\
             &X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Feu-west-1%2Fs3%2Faws4_request\
             &X-Amz-Date=20130524T000000Z\
             &X-Amz-Expires=900\
             &X-Amz-Security-Token=FQoGZXIvYXdzEJr%2F%2F%2F%2F%2F%2F%2F%2F%2F%2FwEaDKX%2Btoken%2Fwith%2Bchars%3D%3D\
             &X-Amz-SignedHeaders=host\
             &X-Amz-Signature=d1706aebc7e180828c6c7631efe1fdf35d18400fe711af55b721003895633591"
        );
        let without_token = presigned_query(
            &presigning("eu-west-1", 900),
            "GET",
            "my-bucket.s3.eu-west-1.amazonaws.com",
            "/folder/file%20name.txt",
            &[],
        );
        assert!(!without_token.contains("X-Amz-Security-Token"));
        assert!(!without_token
            .ends_with("d1706aebc7e180828c6c7631efe1fdf35d18400fe711af55b721003895633591"));
    }

    #[test]
    fn sorts_and_escapes_the_request_s_own_parameters_with_the_signing_ones() {
        let parameters = [(
            "response-content-disposition".to_string(),
            "attachment; filename=\"c+d.txt\"".to_string(),
        )];
        let query = presigned_query(
            &presigning("us-east-1", 60),
            "GET",
            "127.0.0.1:9000",
            "/data/a%20b/c%2Bd.txt",
            &parameters,
        );
        assert_eq!(
            query,
            "X-Amz-Algorithm=AWS4-HMAC-SHA256\
             &X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request\
             &X-Amz-Date=20130524T000000Z\
             &X-Amz-Expires=60\
             &X-Amz-SignedHeaders=host\
             &response-content-disposition=attachment%3B%20filename%3D%22c%2Bd.txt%22\
             &X-Amz-Signature=5c450c7af230f55979c29afaac5a037c39c1b3bb44959a127c1e72d7a6ee7dda"
        );
    }

    #[test]
    fn escapes_everything_but_unreserved_characters() {
        assert_eq!(escape("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(
            escape("a b+c/d=e&f*g'h(i)j!k"),
            "a%20b%2Bc%2Fd%3De%26f%2Ag%27h%28i%29j%21k"
        );
        assert_eq!(
            escape("\u{dc}ber \u{65e5}\u{672c}"),
            "%C3%9Cber%20%E6%97%A5%E6%9C%AC"
        );
        assert_eq!(
            canonical_query(&[
                ("prefix".to_string(), "a b/".to_string()),
                ("list-type".to_string(), "2".to_string()),
                ("X-Amz-Date".to_string(), "now".to_string()),
            ]),
            "X-Amz-Date=now&list-type=2&prefix=a%20b%2F"
        );
    }
}
