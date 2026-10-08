//! HTTP plumbing shared by the Google Drive and OneDrive clients: one pooled HTTPS client,
//! access tokens renewed before they expire, and retries while the service is busy.

use std::sync::{Arc, Mutex as SyncMutex};
use std::time::{Duration, Instant};

use reqwest::header::RETRY_AFTER;
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode};
use ring::rand::{SecureRandom, SystemRandom};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::oauth::{self, OAuthClient};
use super::TokenRotation;
use crate::error::{AppError, AppResult, ErrorKind};

const ATTEMPTS: u32 = 5;
const FIRST_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// An access token is renewed this long before it expires, so none lapses mid-request.
const RENEW_BEFORE: Duration = Duration::from_secs(120);
const DEFAULT_TOKEN_LIFETIME: Duration = Duration::from_secs(3600);

pub fn http_client() -> AppResult<Client> {
    let tls = crate::tls::platform_config(&[b"h2", b"http/1.1"])?;
    Client::builder()
        .tls_backend_preconfigured(tls)
        .connect_timeout(Duration::from_secs(crate::ssh::DEFAULT_TIMEOUT_SECS))
        .read_timeout(Duration::from_secs(60))
        .pool_idle_timeout(Duration::from_secs(90))
        .user_agent(concat!("Poros/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| {
            AppError::new(
                ErrorKind::Connection,
                format!("Could not set up HTTPS: {}", error.without_url()),
            )
        })
}

/// Errors carry no URL: upload addresses include their own authorization.
pub fn transport_error(error: reqwest::Error) -> AppError {
    let kind = if error.is_timeout() {
        ErrorKind::Timeout
    } else if error.is_connect() {
        ErrorKind::Connection
    } else if error.is_decode() {
        ErrorKind::Cloud
    } else if error.is_body() || error.is_request() {
        ErrorKind::Disconnected
    } else {
        ErrorKind::Cloud
    };
    let message = match kind {
        ErrorKind::Timeout => "The service did not respond in time".to_string(),
        ErrorKind::Connection => format!("Could not reach the service: {}", source_of(&error)),
        ErrorKind::Disconnected => format!("The connection failed: {}", source_of(&error)),
        _ => format!(
            "Unexpected answer from the service: {}",
            error.without_url()
        ),
    };
    AppError::new(kind, message)
}

/// The innermost cause, which says more than reqwest's own wording.
fn source_of(error: &reqwest::Error) -> String {
    let mut cause: &dyn std::error::Error = error;
    while let Some(source) = cause.source() {
        cause = source;
    }
    cause.to_string()
}

struct AccessToken {
    value: String,
    renew_at: Instant,
}

pub struct AccessTokens {
    http: Client,
    client: OAuthClient,
    refresh_token: SyncMutex<String>,
    current: Mutex<Option<AccessToken>>,
    rotation: Option<TokenRotation>,
}

impl AccessTokens {
    pub fn new(
        http: Client,
        client: OAuthClient,
        refresh_token: String,
        rotation: Option<TokenRotation>,
    ) -> Self {
        Self {
            http,
            client,
            refresh_token: SyncMutex::new(refresh_token),
            current: Mutex::new(None),
            rotation,
        }
    }

    async fn get(&self) -> AppResult<String> {
        let mut current = self.current.lock().await;
        if let Some(token) = current.as_ref() {
            if Instant::now() < token.renew_at {
                return Ok(token.value.clone());
            }
        }
        let refresh_token = self.refresh_token.lock().unwrap().clone();
        let response = oauth::refresh(&self.http, &self.client, &refresh_token).await?;
        if let Some(replacement) = response
            .refresh_token
            .filter(|replacement| !replacement.is_empty() && *replacement != refresh_token)
        {
            *self.refresh_token.lock().unwrap() = replacement.clone();
            if let Some(rotation) = &self.rotation {
                rotation(&replacement);
            }
        }
        let lifetime = response
            .expires_in
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TOKEN_LIFETIME);
        *current = Some(AccessToken {
            value: response.access_token.clone(),
            renew_at: Instant::now() + lifetime - RENEW_BEFORE.min(lifetime / 2),
        });
        Ok(response.access_token)
    }

    /// The service refused `token` before it was due to expire (revoked, or a clock jump).
    async fn reject(&self, token: &str) {
        let mut current = self.current.lock().await;
        if current
            .as_ref()
            .is_some_and(|current| current.value == token)
        {
            *current = None;
        }
    }
}

/// Builds the request for one attempt; called again for each retry.
pub type BuildRequest<'build> = &'build (dyn Fn(&Client) -> RequestBuilder + Send + Sync);

#[derive(Clone)]
pub struct Api {
    tokens: Arc<AccessTokens>,
}

impl Api {
    pub fn new(tokens: Arc<AccessTokens>) -> Self {
        Self { tokens }
    }

    /// Sends an authorized request. A refused token is renewed once; throttling, server errors
    /// and dropped connections are retried with backoff, though a `POST` that may have gone
    /// through is retried only when the service said it was too busy to take it.
    pub async fn send(&self, build: BuildRequest<'_>) -> AppResult<Response> {
        self.execute(build, true).await
    }

    /// For upload addresses that carry their own authorization and reject a second one.
    pub async fn send_unauthorized(&self, build: BuildRequest<'_>) -> AppResult<Response> {
        self.execute(build, false).await
    }

    pub async fn json<T: DeserializeOwned>(&self, build: BuildRequest<'_>) -> AppResult<T> {
        decode(self.send(build).await?).await
    }

    async fn execute(&self, build: BuildRequest<'_>, authorize: bool) -> AppResult<Response> {
        let mut attempt = 0;
        let mut renewed = false;
        loop {
            attempt += 1;
            let token = match authorize {
                true => Some(self.tokens.get().await?),
                false => None,
            };
            let mut builder = build(&self.tokens.http);
            if let Some(token) = &token {
                builder = builder.bearer_auth(token);
            }
            let request = builder.build().map_err(transport_error)?;
            let repeatable = request.method() != Method::POST;
            let response = match self.tokens.http.execute(request).await {
                Ok(response) => response,
                Err(error) => {
                    if attempt < ATTEMPTS && (repeatable || error.is_connect()) {
                        tokio::time::sleep(backoff(attempt)).await;
                        continue;
                    }
                    return Err(transport_error(error));
                }
            };
            let status = response.status();
            // Google answers 308 to an upload chunk that leaves the file incomplete.
            if status.is_success() || status == StatusCode::PERMANENT_REDIRECT {
                return Ok(response);
            }
            if status == StatusCode::UNAUTHORIZED && !renewed {
                if let Some(token) = &token {
                    renewed = true;
                    self.tokens.reject(token).await;
                    continue;
                }
            }
            let wait = retry_after(&response);
            let failure = ServiceError::read(response).await;
            let transient = failure.throttled
                || (repeatable
                    && matches!(
                        status,
                        StatusCode::INTERNAL_SERVER_ERROR
                            | StatusCode::BAD_GATEWAY
                            | StatusCode::SERVICE_UNAVAILABLE
                            | StatusCode::GATEWAY_TIMEOUT
                    ));
            if transient && attempt < ATTEMPTS {
                tokio::time::sleep(wait.unwrap_or_else(|| backoff(attempt)).min(MAX_BACKOFF)).await;
                continue;
            }
            return Err(failure.error);
        }
    }
}

pub async fn decode<T: DeserializeOwned>(response: Response) -> AppResult<T> {
    response.json::<T>().await.map_err(transport_error)
}

/// Doubles from half a second, with up to a quarter second of jitter so parallel workers do
/// not retry in step.
fn backoff(attempt: u32) -> Duration {
    let mut jitter = [0u8; 1];
    let _ = SystemRandom::new().fill(&mut jitter);
    let doubled = FIRST_BACKOFF.saturating_mul(1 << attempt.saturating_sub(1).min(6));
    (doubled + Duration::from_millis(u64::from(jitter[0]))).min(MAX_BACKOFF)
}

fn retry_after(response: &Response) -> Option<Duration> {
    let seconds = response
        .headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(Duration::from_secs(seconds))
}

/// The `error` object both Google and Microsoft send with a failed request.
#[derive(Debug, Default, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: ErrorDetail,
}

#[derive(Debug, Default, Deserialize)]
struct ErrorDetail {
    #[serde(default)]
    message: String,
    /// A number from Google, a name such as `itemNotFound` from Microsoft.
    #[serde(default)]
    code: serde_json::Value,
    #[serde(default)]
    errors: Vec<ErrorReason>,
}

#[derive(Debug, Default, Deserialize)]
struct ErrorReason {
    #[serde(default)]
    reason: String,
}

struct ServiceError {
    error: AppError,
    /// The service asked to slow down; the request did not go through.
    throttled: bool,
}

impl ServiceError {
    async fn read(response: Response) -> Self {
        let status = response.status();
        let body: ErrorBody = response.json().await.unwrap_or_default();
        let detail = body.error;
        let reasons: Vec<&str> = detail
            .errors
            .iter()
            .map(|reason| reason.reason.as_str())
            .chain(detail.code.as_str())
            .collect();
        let has_reason = |names: &[&str]| reasons.iter().any(|reason| names.contains(reason));
        let throttled = status == StatusCode::TOO_MANY_REQUESTS
            || has_reason(&[
                "rateLimitExceeded",
                "userRateLimitExceeded",
                "activityLimitReached",
            ]);
        let out_of_space = status == StatusCode::INSUFFICIENT_STORAGE
            || has_reason(&[
                "storageQuotaExceeded",
                "quotaLimitReached",
                "insufficientStorage",
            ]);
        let message = match detail.message.trim() {
            "" => format!("The service answered {status}"),
            message => message.to_string(),
        };
        let kind = if out_of_space {
            ErrorKind::PermissionDenied
        } else if throttled || status.is_server_error() {
            ErrorKind::Cloud
        } else {
            match status {
                StatusCode::UNAUTHORIZED => ErrorKind::AuthFailed,
                StatusCode::FORBIDDEN => ErrorKind::PermissionDenied,
                StatusCode::NOT_FOUND | StatusCode::GONE => ErrorKind::NotFound,
                StatusCode::CONFLICT => ErrorKind::AlreadyExists,
                StatusCode::PRECONDITION_FAILED | StatusCode::REQUEST_TIMEOUT => ErrorKind::Cloud,
                _ => ErrorKind::InvalidInput,
            }
        };
        let message = match kind {
            ErrorKind::AuthFailed => {
                format!("The sign-in is no longer valid; sign in again ({message})")
            }
            ErrorKind::PermissionDenied if out_of_space => {
                format!("Out of storage space: {message}")
            }
            _ => message,
        };
        Self {
            error: AppError::new(kind, message),
            throttled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_is_capped() {
        assert!(backoff(1) < Duration::from_secs(1));
        assert!(backoff(3) >= Duration::from_secs(2));
        assert_eq!(backoff(40), MAX_BACKOFF);
    }

    #[test]
    fn reads_both_error_shapes() {
        let google: ErrorBody = serde_json::from_str(
            r#"{"error":{"code":403,"message":"Rate limit","errors":[{"reason":"userRateLimitExceeded"}]}}"#,
        )
        .unwrap();
        assert_eq!(google.error.errors[0].reason, "userRateLimitExceeded");
        let microsoft: ErrorBody =
            serde_json::from_str(r#"{"error":{"code":"itemNotFound","message":"Item not found"}}"#)
                .unwrap();
        assert_eq!(microsoft.error.code.as_str(), Some("itemNotFound"));
    }
}
