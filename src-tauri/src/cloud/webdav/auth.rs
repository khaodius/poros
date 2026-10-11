//! HTTP sign-in for WebDAV servers: Basic, or Digest (RFC 7616) with MD5 or SHA-256. Nothing is
//! sent until the server asks, so a password never goes out in the clear to a server that
//! would have taken Digest.

use std::collections::HashMap;
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use md5::Md5;
use reqwest::header::WWW_AUTHENTICATE;
use reqwest::{Method, Response, Url};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest as _, Sha256};

use crate::error::{AppError, AppResult, ErrorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Algorithm {
    Md5,
    Md5Session,
    Sha256,
    Sha256Session,
}

impl Algorithm {
    fn parse(name: Option<&str>) -> Option<Self> {
        match name.map(str::to_ascii_uppercase).as_deref() {
            None | Some("MD5") => Some(Self::Md5),
            Some("MD5-SESS") => Some(Self::Md5Session),
            Some("SHA-256") => Some(Self::Sha256),
            Some("SHA-256-SESS") => Some(Self::Sha256Session),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Md5Session => "MD5-sess",
            Self::Sha256 => "SHA-256",
            Self::Sha256Session => "SHA-256-sess",
        }
    }

    fn hash(self, text: &str) -> String {
        let digest: Vec<u8> = match self {
            Self::Md5 | Self::Md5Session => Md5::digest(text.as_bytes()).to_vec(),
            Self::Sha256 | Self::Sha256Session => Sha256::digest(text.as_bytes()).to_vec(),
        };
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn is_session(self) -> bool {
        matches!(self, Self::Md5Session | Self::Sha256Session)
    }

    /// SHA-256 first, the stronger hash, when a server offers both.
    fn strength(self) -> u8 {
        match self {
            Self::Sha256 | Self::Sha256Session => 1,
            Self::Md5 | Self::Md5Session => 0,
        }
    }
}

#[derive(Debug, Clone)]
struct DigestChallenge {
    realm: String,
    nonce: String,
    opaque: Option<String>,
    algorithm: Algorithm,
    /// `auth` when the server offers it; servers from before RFC 2617 offer none.
    protect: bool,
    count: u32,
    client_nonce: String,
}

#[derive(Debug, Clone)]
enum Scheme {
    Basic(String),
    Digest(DigestChallenge),
}

/// One challenge from a `WWW-Authenticate` header, its parameter names in lower case.
#[derive(Debug, PartialEq, Eq)]
struct Challenge {
    scheme: String,
    parameters: HashMap<String, String>,
}

pub struct Authenticator {
    username: String,
    password: String,
    scheme: Mutex<Option<Scheme>>,
}

impl Authenticator {
    pub fn new(username: &str, password: &str) -> Self {
        Self {
            username: username.to_string(),
            password: password.to_string(),
            scheme: Mutex::new(None),
        }
    }

    /// The `Authorization` value for a request, once the server has said how it wants it.
    pub fn header(&self, method: &Method, url: &Url) -> Option<String> {
        let mut scheme = self.scheme.lock().unwrap();
        match scheme.as_mut()? {
            Scheme::Basic(value) => Some(value.clone()),
            Scheme::Digest(challenge) => {
                challenge.count += 1;
                let target = match url.query() {
                    Some(query) => format!("{}?{query}", url.path()),
                    None => url.path().to_string(),
                };
                Some(digest_header(
                    &self.username,
                    &self.password,
                    challenge,
                    method.as_str(),
                    &target,
                ))
            }
        }
    }

    /// Takes in the challenges of a 401 answer. True when another attempt can sign in; false
    /// when the server refused credentials it was already sent.
    pub fn challenge(&self, response: &Response) -> AppResult<bool> {
        let challenges: Vec<Challenge> = response
            .headers()
            .get_all(WWW_AUTHENTICATE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(parse_challenges)
            .collect();
        let mut scheme = self.scheme.lock().unwrap();
        let digest = challenges
            .iter()
            .filter(|challenge| challenge.scheme == "digest")
            .filter_map(digest_challenge)
            .max_by_key(|challenge| challenge.algorithm.strength());
        if let Some(fresh) = digest {
            let stale = challenges.iter().any(|challenge| {
                challenge.scheme == "digest"
                    && challenge
                        .parameters
                        .get("stale")
                        .is_some_and(|stale| stale.eq_ignore_ascii_case("true"))
            });
            // A stale nonce is a new round, not a refused password.
            let retry = scheme.is_none() || stale;
            *scheme = Some(Scheme::Digest(fresh));
            return Ok(retry);
        }
        if challenges
            .iter()
            .any(|challenge| challenge.scheme == "basic")
        {
            let retry = scheme.is_none();
            let credentials = format!("{}:{}", self.username, self.password);
            *scheme = Some(Scheme::Basic(format!(
                "Basic {}",
                STANDARD.encode(credentials)
            )));
            return Ok(retry);
        }
        match challenges.first() {
            Some(challenge) => Err(AppError::new(
                ErrorKind::AuthFailed,
                format!(
                    "The server asks for {} sign-in, which Poros does not support",
                    challenge.scheme
                ),
            )),
            None => Ok(false),
        }
    }
}

fn digest_challenge(challenge: &Challenge) -> Option<DigestChallenge> {
    let parameter = |name: &str| challenge.parameters.get(name).cloned();
    let algorithm = Algorithm::parse(parameter("algorithm").as_deref())?;
    let protect = match parameter("qop") {
        None => false,
        Some(offered) => {
            let offers_auth = offered
                .split(',')
                .any(|option| option.trim().eq_ignore_ascii_case("auth"));
            // Only `auth-int` would mean hashing every request body; not offered.
            if !offers_auth {
                return None;
            }
            true
        }
    };
    Some(DigestChallenge {
        realm: parameter("realm").unwrap_or_default(),
        nonce: parameter("nonce")?,
        opaque: parameter("opaque"),
        algorithm,
        protect,
        count: 0,
        client_nonce: client_nonce(),
    })
}

fn client_nonce() -> String {
    let mut bytes = [0u8; 16];
    let _ = SystemRandom::new().fill(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A quoted-string with its quotes and backslashes escaped.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn digest_header(
    username: &str,
    password: &str,
    challenge: &DigestChallenge,
    method: &str,
    target: &str,
) -> String {
    let count = format!("{:08x}", challenge.count);
    let response = digest_response(
        challenge.algorithm,
        [username, &challenge.realm, password],
        &challenge.nonce,
        challenge
            .protect
            .then_some((count.as_str(), challenge.client_nonce.as_str())),
        method,
        target,
    );
    let mut header = format!(
        "Digest username={}, realm={}, nonce={}, uri={}, algorithm={}, response=\"{response}\"",
        quoted(username),
        quoted(&challenge.realm),
        quoted(&challenge.nonce),
        quoted(target),
        challenge.algorithm.name(),
    );
    if challenge.protect {
        header.push_str(&format!(
            ", qop=auth, nc={count}, cnonce=\"{}\"",
            challenge.client_nonce
        ));
    }
    if let Some(opaque) = &challenge.opaque {
        header.push_str(&format!(", opaque={}", quoted(opaque)));
    }
    header
}

/// The `response` value: the hash proving the password, for `qop=auth` when `count_and_nonce`
/// carries the request count and client nonce.
fn digest_response(
    algorithm: Algorithm,
    [username, realm, password]: [&str; 3],
    nonce: &str,
    count_and_nonce: Option<(&str, &str)>,
    method: &str,
    target: &str,
) -> String {
    let mut secret = algorithm.hash(&format!("{username}:{realm}:{password}"));
    if algorithm.is_session() {
        let client_nonce = count_and_nonce.map_or("", |(_, client_nonce)| client_nonce);
        secret = algorithm.hash(&format!("{secret}:{nonce}:{client_nonce}"));
    }
    let request = algorithm.hash(&format!("{method}:{target}"));
    match count_and_nonce {
        Some((count, client_nonce)) => algorithm.hash(&format!(
            "{secret}:{nonce}:{count}:{client_nonce}:auth:{request}"
        )),
        None => algorithm.hash(&format!("{secret}:{nonce}:{request}")),
    }
}

/// The challenges in one `WWW-Authenticate` value, which may hold several.
fn parse_challenges(header: &str) -> Vec<Challenge> {
    let mut challenges: Vec<Challenge> = Vec::new();
    let mut rest = header;
    loop {
        rest = rest
            .trim_start_matches(|character: char| character == ',' || character.is_whitespace());
        if rest.is_empty() {
            return challenges;
        }
        let token_end = rest
            .find(|character: char| {
                character == '=' || character == ',' || character.is_whitespace()
            })
            .unwrap_or(rest.len());
        let token = &rest[..token_end];
        let after = rest[token_end..].trim_start();
        match after.strip_prefix('=') {
            Some(value) => {
                let value = value.trim_start();
                let (value, remaining) = match value.strip_prefix('"') {
                    Some(quoted) => read_quoted(quoted),
                    None => {
                        let end = value
                            .find(|character: char| character == ',' || character.is_whitespace())
                            .unwrap_or(value.len());
                        (value[..end].to_string(), &value[end..])
                    }
                };
                if let Some(challenge) = challenges.last_mut() {
                    challenge
                        .parameters
                        .insert(token.to_ascii_lowercase(), value);
                }
                rest = remaining;
            }
            None => {
                challenges.push(Challenge {
                    scheme: token.to_ascii_lowercase(),
                    parameters: HashMap::new(),
                });
                rest = &rest[token_end..];
            }
        }
    }
}

/// The content of a quoted-string whose opening quote is already read, and what follows it.
fn read_quoted(text: &str) -> (String, &str) {
    let mut value = String::new();
    let mut characters = text.char_indices();
    while let Some((index, character)) = characters.next() {
        match character {
            '"' => return (value, &text[index + 1..]),
            '\\' => {
                if let Some((_, escaped)) = characters.next() {
                    value.push(escaped);
                }
            }
            other => value.push(other),
        }
    }
    (value, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_rfc_2617_digests() {
        let response = digest_response(
            Algorithm::Md5,
            ["Mufasa", "testrealm@host.com", "Circle Of Life"],
            "dcd98b7102dd2f0e8b11d0f600bfb0c093",
            Some(("00000001", "0a4f113b")),
            "GET",
            "/dir/index.html",
        );
        assert_eq!(response, "6629fae49393a05397450978507c4ef1");
    }

    #[test]
    fn computes_rfc_7616_digests() {
        let credentials = ["Mufasa", "http-auth@example.org", "Circle of Life"];
        let nonce = "7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v";
        let client = Some(("00000001", "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ"));
        assert_eq!(
            digest_response(
                Algorithm::Md5,
                credentials,
                nonce,
                client,
                "GET",
                "/dir/index.html"
            ),
            "8ca523f5e9506fed4657c9700eebdbec"
        );
        assert_eq!(
            digest_response(
                Algorithm::Sha256,
                credentials,
                nonce,
                client,
                "GET",
                "/dir/index.html"
            ),
            "753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1"
        );
    }

    #[test]
    fn reads_several_challenges_in_one_header() {
        let challenges = parse_challenges(
            r#"Basic realm="files, and more", Digest realm="dav", qop="auth,auth-int", nonce="abc", algorithm=SHA-256, stale=TRUE"#,
        );
        assert_eq!(challenges.len(), 2);
        assert_eq!(challenges[0].scheme, "basic");
        assert_eq!(challenges[0].parameters["realm"], "files, and more");
        assert_eq!(challenges[1].scheme, "digest");
        assert_eq!(challenges[1].parameters["qop"], "auth,auth-int");
        assert_eq!(challenges[1].parameters["algorithm"], "SHA-256");
        let digest = digest_challenge(&challenges[1]).unwrap();
        assert_eq!(digest.algorithm, Algorithm::Sha256);
        assert!(digest.protect);
        assert_eq!(digest.nonce, "abc");
    }

    #[test]
    fn unescapes_quoted_values_and_survives_odd_input() {
        let challenges = parse_challenges(r#"Digest realm="a \"b\" c", nonce=xyz"#);
        assert_eq!(challenges[0].parameters["realm"], r#"a "b" c"#);
        assert_eq!(challenges[0].parameters["nonce"], "xyz");
        assert_eq!(parse_challenges("Negotiate abc==").len(), 1);
        assert!(parse_challenges("").is_empty());
        assert_eq!(parse_challenges("=,=\"").len(), 0);
    }

    #[test]
    fn skips_digests_it_cannot_answer() {
        let only_integrity = parse_challenges(r#"Digest realm="r", nonce="n", qop="auth-int""#);
        assert!(digest_challenge(&only_integrity[0]).is_none());
        let unknown = parse_challenges(r#"Digest realm="r", nonce="n", algorithm=SHA-512-256"#);
        assert!(digest_challenge(&unknown[0]).is_none());
    }

    #[test]
    fn signs_requests_once_challenged() {
        let authenticator = Authenticator::new("ada", "secret");
        let url = Url::parse("https://dav.example.com/a%20b/?x=1").unwrap();
        assert_eq!(authenticator.header(&Method::GET, &url), None);
        *authenticator.scheme.lock().unwrap() = Some(Scheme::Digest(DigestChallenge {
            realm: "dav".into(),
            nonce: "n".into(),
            opaque: Some("o".into()),
            algorithm: Algorithm::Md5,
            protect: true,
            count: 0,
            client_nonce: "c".into(),
        }));
        let first = authenticator.header(&Method::GET, &url).unwrap();
        let second = authenticator.header(&Method::GET, &url).unwrap();
        assert!(first.contains(r#"uri="/a%20b/?x=1""#), "{first}");
        assert!(first.contains("nc=00000001") && second.contains("nc=00000002"));
        assert!(first.contains(r#"opaque="o""#));
    }
}
