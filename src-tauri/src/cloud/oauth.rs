//! Browser sign-in for Google and Microsoft accounts: the OAuth 2.0 authorization code flow
//! with PKCE, redirected to a one-time listener on the loopback interface (RFC 8252). Poros
//! never sees the account password, and the refresh token it receives stays in the backend.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Mutex;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use reqwest::{Client, Url};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::CloudProvider;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::settings::CloudSettings;

const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
/// A browser connection that sends no request within this long is a speculative one.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REQUEST_BYTES: usize = 16 * 1024;

const GOOGLE_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_SCOPES: &str = "openid email https://www.googleapis.com/auth/drive";
const MICROSOFT_LOGIN_URL: &str = "https://login.microsoftonline.com";
const MICROSOFT_SCOPES: &str = "openid profile offline_access Files.ReadWrite.All";
/// Work, school and personal Microsoft accounts.
pub const DEFAULT_TENANT: &str = "common";

/// OAuth apps a release build was configured with, used when Settings names none.
const BUILT_IN_GOOGLE_CLIENT_ID: Option<&str> = option_env!("POROS_GOOGLE_CLIENT_ID");
const BUILT_IN_GOOGLE_CLIENT_SECRET: Option<&str> = option_env!("POROS_GOOGLE_CLIENT_SECRET");
const BUILT_IN_MICROSOFT_CLIENT_ID: Option<&str> = option_env!("POROS_MICROSOFT_CLIENT_ID");

/// The app registration Poros signs in as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthClient {
    pub provider: CloudProvider,
    pub client_id: String,
    /// Google gives desktop apps a secret and wants it with every token request, although an
    /// installed app cannot keep it secret. Microsoft desktop apps have none.
    pub client_secret: String,
    pub tenant: String,
}

impl OAuthClient {
    /// The app named in Settings, or else the one built into this release.
    pub fn configured(provider: CloudProvider, settings: &CloudSettings) -> Option<Self> {
        let (client_id, client_secret) = match provider {
            CloudProvider::Google if !settings.google_client_id.is_empty() => (
                settings.google_client_id.clone(),
                settings.google_client_secret.clone(),
            ),
            CloudProvider::Google => (
                built_in(BUILT_IN_GOOGLE_CLIENT_ID)?,
                built_in(BUILT_IN_GOOGLE_CLIENT_SECRET).unwrap_or_default(),
            ),
            CloudProvider::Microsoft if !settings.microsoft_client_id.is_empty() => {
                (settings.microsoft_client_id.clone(), String::new())
            }
            CloudProvider::Microsoft => (built_in(BUILT_IN_MICROSOFT_CLIENT_ID)?, String::new()),
        };
        let tenant = match settings.microsoft_tenant.as_str() {
            "" => DEFAULT_TENANT.to_string(),
            tenant => tenant.to_string(),
        };
        Some(Self {
            provider,
            client_id,
            client_secret,
            tenant,
        })
    }

    fn authorize_url(&self) -> String {
        match self.provider {
            CloudProvider::Google => GOOGLE_AUTHORIZE_URL.to_string(),
            CloudProvider::Microsoft => {
                format!(
                    "{MICROSOFT_LOGIN_URL}/{}/oauth2/v2.0/authorize",
                    self.tenant
                )
            }
        }
    }

    fn token_url(&self) -> String {
        match self.provider {
            CloudProvider::Google => GOOGLE_TOKEN_URL.to_string(),
            CloudProvider::Microsoft => {
                format!("{MICROSOFT_LOGIN_URL}/{}/oauth2/v2.0/token", self.tenant)
            }
        }
    }

    fn scopes(&self) -> &'static str {
        match self.provider {
            CloudProvider::Google => GOOGLE_SCOPES,
            CloudProvider::Microsoft => MICROSOFT_SCOPES,
        }
    }
}

fn built_in(value: Option<&'static str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn missing_client(provider: CloudProvider) -> AppError {
    AppError::invalid(format!(
        "Add a {} client ID in Settings, under Cloud accounts, to sign in",
        provider.name()
    ))
}

/// Whether a provider can be signed in to, for the connection dialog. Mirrored in
/// `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub provider: CloudProvider,
    pub configured: bool,
    /// The app comes with this release rather than from Settings.
    pub built_in: bool,
}

pub fn provider_statuses(settings: &CloudSettings) -> Vec<ProviderStatus> {
    [CloudProvider::Google, CloudProvider::Microsoft]
        .into_iter()
        .map(|provider| {
            let from_settings = match provider {
                CloudProvider::Google => !settings.google_client_id.is_empty(),
                CloudProvider::Microsoft => !settings.microsoft_client_id.is_empty(),
            };
            let configured = OAuthClient::configured(provider, settings).is_some();
            ProviderStatus {
                provider,
                configured,
                built_in: configured && !from_settings,
            }
        })
        .collect()
}

/// A finished sign-in, named by `grant_id` until it is saved with a connection or used to
/// connect. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedIn {
    pub grant_id: String,
    pub provider: CloudProvider,
    /// The account's email address or name, to tell accounts apart.
    pub account: String,
}

struct Grant {
    provider: CloudProvider,
    refresh_token: String,
}

/// Sign-ins in progress, and finished ones the frontend refers to by id.
#[derive(Default)]
pub struct OAuthVault {
    grants: Mutex<HashMap<String, Grant>>,
    pending: Mutex<HashMap<String, CancellationToken>>,
}

impl OAuthVault {
    pub fn add(&self, provider: CloudProvider, refresh_token: String) -> String {
        let grant_id = uuid::Uuid::new_v4().to_string();
        self.grants.lock().unwrap().insert(
            grant_id.clone(),
            Grant {
                provider,
                refresh_token,
            },
        );
        grant_id
    }

    pub fn refresh_token(&self, grant_id: &str, provider: CloudProvider) -> AppResult<String> {
        self.grants
            .lock()
            .unwrap()
            .get(grant_id)
            .filter(|grant| grant.provider == provider)
            .map(|grant| grant.refresh_token.clone())
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::AuthFailed,
                    format!("Sign in with {} again", provider.name()),
                )
            })
    }

    pub fn begin(&self, request_id: &str) -> CancellationToken {
        let cancel = CancellationToken::new();
        if let Some(previous) = self
            .pending
            .lock()
            .unwrap()
            .insert(request_id.to_string(), cancel.clone())
        {
            previous.cancel();
        }
        cancel
    }

    pub fn end(&self, request_id: &str) {
        self.pending.lock().unwrap().remove(request_id);
    }

    pub fn cancel(&self, request_id: &str) {
        if let Some(cancel) = self.pending.lock().unwrap().remove(request_id) {
            cancel.cancel();
        }
    }
}

pub struct Authorization {
    pub account: String,
    pub refresh_token: String,
}

/// Opens the provider's sign-in page with `open_browser` and waits for the browser to come
/// back with an authorization code, which it trades for tokens.
pub async fn sign_in(
    http: &Client,
    client: &OAuthClient,
    open_browser: impl FnOnce(&str) -> AppResult<()>,
    cancel: &CancellationToken,
) -> AppResult<Authorization> {
    let verifier = random_token(32)?;
    let challenge = URL_SAFE_NO_PAD.encode(ring::digest::digest(
        &ring::digest::SHA256,
        verifier.as_bytes(),
    ));
    let state = random_token(16)?;
    let loopback = Loopback::bind().await?;
    let redirect_uri = loopback.redirect_uri(client.provider);

    let mut url = Url::parse(&client.authorize_url())
        .map_err(|error| AppError::invalid(format!("Invalid sign-in address: {error}")))?;
    url.query_pairs_mut()
        .append_pair("client_id", &client.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", client.scopes())
        .append_pair("state", &state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
    match client.provider {
        // Offline access with `consent` gets a refresh token on every sign-in, not only the
        // first; `select_account` offers the account chooser even when one account is signed in.
        CloudProvider::Google => {
            url.query_pairs_mut()
                .append_pair("access_type", "offline")
                .append_pair("prompt", "select_account consent");
        }
        CloudProvider::Microsoft => {
            url.query_pairs_mut()
                .append_pair("prompt", "select_account");
        }
    }
    open_browser(url.as_str())?;

    let code = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(AppError::cancelled()),
        _ = tokio::time::sleep(SIGN_IN_TIMEOUT) => {
            return Err(AppError::new(ErrorKind::Timeout, "The sign-in was not finished in time"))
        }
        code = loopback.wait_for_code(&state) => code?,
    };
    let tokens = request_tokens(
        http,
        client,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect_uri),
            ("code_verifier", &verifier),
        ],
    )
    .await?;
    let refresh_token = tokens
        .refresh_token
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            AppError::new(
                ErrorKind::AuthFailed,
                format!(
                    "{} did not allow Poros to stay signed in; sign in again",
                    client.provider.name()
                ),
            )
        })?;
    let account = tokens
        .id_token
        .as_deref()
        .and_then(account_name)
        .unwrap_or_else(|| format!("{} account", client.provider.name()));
    Ok(Authorization {
        account,
        refresh_token,
    })
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// A new access token, and with some providers a replacement refresh token.
pub async fn refresh(
    http: &Client,
    client: &OAuthClient,
    refresh_token: &str,
) -> AppResult<TokenResponse> {
    request_tokens(
        http,
        client,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
    .await
}

async fn request_tokens(
    http: &Client,
    client: &OAuthClient,
    parameters: &[(&str, &str)],
) -> AppResult<TokenResponse> {
    let mut form: Vec<(&str, &str)> = parameters.to_vec();
    form.push(("client_id", &client.client_id));
    if !client.client_secret.is_empty() {
        form.push(("client_secret", &client.client_secret));
    }
    if client.provider == CloudProvider::Microsoft {
        form.push(("scope", client.scopes()));
    }
    let response = http
        .post(client.token_url())
        .form(&form)
        .send()
        .await
        .map_err(super::api::transport_error)?;
    let status = response.status();
    if status.is_success() {
        return response
            .json::<TokenResponse>()
            .await
            .map_err(super::api::transport_error);
    }
    let provider = client.provider.name();
    if status.is_server_error() {
        return Err(AppError::new(
            ErrorKind::Cloud,
            format!("{provider} sign-in is not available right now ({status})"),
        ));
    }
    let refusal = response.json::<TokenError>().await.ok();
    let description = refusal
        .as_ref()
        .and_then(|refusal| refusal.error_description.clone())
        .map(|description| first_line(&description))
        .unwrap_or_else(|| status.to_string());
    let message = match refusal.as_ref().map(|refusal| refusal.error.as_str()) {
        Some("invalid_grant") => {
            format!("The {provider} sign-in expired or was revoked; sign in again")
        }
        Some("invalid_client" | "unauthorized_client") => {
            format!("{provider} does not accept the client ID in Settings: {description}")
        }
        _ => format!("{provider} refused the sign-in: {description}"),
    };
    Err(AppError::new(ErrorKind::AuthFailed, message))
}

/// Microsoft appends trace and correlation ids on further lines.
fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).trim().to_string()
}

/// The email address or name in an ID token. Only displayed, so the signature is not checked;
/// the token came straight from the provider over TLS.
fn account_name(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?)
            .ok()?;
    ["email", "preferred_username", "name"]
        .into_iter()
        .filter_map(|claim| claims.get(claim)?.as_str())
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_string)
}

fn random_token(bytes: usize) -> AppResult<String> {
    let mut buffer = vec![0u8; bytes];
    SystemRandom::new()
        .fill(&mut buffer)
        .map_err(|_| AppError::new(ErrorKind::Io, "The system random generator failed"))?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// Listens on one port of both loopback addresses, since the browser may resolve `localhost`
/// to either.
struct Loopback {
    listeners: Vec<TcpListener>,
    port: u16,
}

impl Loopback {
    async fn bind() -> AppResult<Self> {
        let ipv4 = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let port = ipv4.local_addr()?.port();
        let mut listeners = vec![ipv4];
        if let Ok(ipv6) = TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await {
            listeners.push(ipv6);
        }
        Ok(Self { listeners, port })
    }

    /// The address each provider's desktop app registration accepts with any port: Google
    /// asks for the IP literal, Microsoft for `http://localhost`.
    fn redirect_uri(&self, provider: CloudProvider) -> String {
        match provider {
            CloudProvider::Google => format!("http://127.0.0.1:{}", self.port),
            CloudProvider::Microsoft => format!("http://localhost:{}", self.port),
        }
    }

    async fn wait_for_code(self, state: &str) -> AppResult<String> {
        let (sender, mut receiver) = mpsc::channel::<AppResult<String>>(4);
        let mut accepting = JoinSet::new();
        for listener in self.listeners {
            let sender = sender.clone();
            let state = state.to_string();
            accepting.spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let sender = sender.clone();
                    let state = state.clone();
                    tokio::spawn(async move {
                        if let Some(outcome) = answer(stream, &state).await {
                            let _ = sender.send(outcome).await;
                        }
                    });
                }
            });
        }
        drop(sender);
        receiver.recv().await.unwrap_or_else(|| {
            Err(AppError::new(
                ErrorKind::Connection,
                "The sign-in listener stopped",
            ))
        })
    }
}

/// Serves one browser request. Returns the outcome when the request was the redirect back
/// from the provider, and `None` for anything else (a favicon, a stale tab, a preconnect).
async fn answer(mut stream: TcpStream, state: &str) -> Option<AppResult<String>> {
    let target = tokio::time::timeout(REQUEST_TIMEOUT, read_request_target(&mut stream))
        .await
        .ok()??;
    let Some(parameters) = callback_parameters(&target) else {
        respond(&mut stream, "404 Not Found", "Not found", "").await;
        return None;
    };
    if parameters.get("state").map(String::as_str) != Some(state) {
        respond(
            &mut stream,
            "400 Bad Request",
            "This sign-in has expired",
            "Start the sign-in again from Poros.",
        )
        .await;
        return None;
    }
    if let Some(error) = parameters.get("error") {
        let reason = parameters
            .get("error_description")
            .map(|description| first_line(description))
            .unwrap_or_else(|| error.clone());
        respond(
            &mut stream,
            "200 OK",
            "Sign-in was not completed",
            "You can close this tab and try again from Poros.",
        )
        .await;
        let message = if error == "access_denied" {
            "The sign-in was declined".to_string()
        } else {
            format!("The sign-in failed: {reason}")
        };
        return Some(Err(AppError::new(ErrorKind::AuthFailed, message)));
    }
    let code = parameters
        .get("code")
        .filter(|code| !code.is_empty())?
        .clone();
    respond(
        &mut stream,
        "200 OK",
        "Poros is signed in",
        "You can close this tab and return to Poros.",
    )
    .await;
    Some(Ok(code))
}

async fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let mut request = Vec::with_capacity(1024);
    let mut buffer = [0u8; 2048];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 || request.len() + read > MAX_REQUEST_BYTES {
            return None;
        }
        request.extend_from_slice(&buffer[..read]);
    }
    let head = String::from_utf8_lossy(&request);
    let mut request_line = head.lines().next()?.split(' ');
    match (request_line.next(), request_line.next()) {
        (Some("GET"), Some(target)) => Some(target.to_string()),
        _ => None,
    }
}

/// The query of a request for `/` that carries an OAuth response.
fn callback_parameters(target: &str) -> Option<HashMap<String, String>> {
    let url = Url::parse(&format!("http://localhost{target}")).ok()?;
    if url.path() != "/" {
        return None;
    }
    let parameters: HashMap<String, String> = url.query_pairs().into_owned().collect();
    (parameters.contains_key("code") || parameters.contains_key("error")).then_some(parameters)
}

async fn respond(stream: &mut TcpStream, status: &str, heading: &str, detail: &str) {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Poros</title>\
         <style>body{{font-family:system-ui,sans-serif;background:#16181d;color:#e6e8ee;\
         display:flex;align-items:center;justify-content:center;height:100vh;margin:0}}\
         main{{text-align:center}}p{{color:#9aa1ad}}</style></head>\
         <body><main><h1>{heading}</h1><p>{detail}</p></main></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_account_from_an_id_token() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"1","email":"ada@example.com"}"#);
        assert_eq!(
            account_name(&format!("header.{payload}.signature")).as_deref(),
            Some("ada@example.com")
        );
        let payload = URL_SAFE_NO_PAD.encode(br#"{"preferred_username":"ada@contoso.com"}"#);
        assert_eq!(
            account_name(&format!("h.{payload}.s")).as_deref(),
            Some("ada@contoso.com")
        );
        assert_eq!(account_name("not a token"), None);
    }

    #[test]
    fn recognizes_the_redirect() {
        let parameters = callback_parameters("/?code=abc&state=xyz").unwrap();
        assert_eq!(parameters["code"], "abc");
        assert_eq!(parameters["state"], "xyz");
        assert!(callback_parameters("/favicon.ico").is_none());
        assert!(callback_parameters("/?state=xyz").is_none());
        assert!(callback_parameters("/?error=access_denied&state=x").is_some());
    }

    #[test]
    fn prefers_settings_over_the_built_in_app() {
        let settings = CloudSettings {
            google_client_id: "id.apps.googleusercontent.com".into(),
            google_client_secret: "secret".into(),
            microsoft_client_id: "00000000-0000-0000-0000-000000000000".into(),
            microsoft_tenant: String::new(),
        };
        let google = OAuthClient::configured(CloudProvider::Google, &settings).unwrap();
        assert_eq!(google.client_id, "id.apps.googleusercontent.com");
        assert_eq!(google.client_secret, "secret");
        let microsoft = OAuthClient::configured(CloudProvider::Microsoft, &settings).unwrap();
        assert_eq!(microsoft.tenant, DEFAULT_TENANT);
        assert!(microsoft.client_secret.is_empty());
        assert!(microsoft.token_url().ends_with("/common/oauth2/v2.0/token"));
    }

    #[tokio::test]
    async fn the_listener_hands_back_the_code() {
        let loopback = Loopback::bind().await.unwrap();
        let port = loopback.port;
        let waiting = tokio::spawn(loopback.wait_for_code("expected"));

        let mut stray = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        stray
            .write_all(b"GET /?code=old&state=other HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut reply = String::new();
        stray.read_to_string(&mut reply).await.unwrap();
        assert!(reply.starts_with("HTTP/1.1 400"));

        let mut browser = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        browser
            .write_all(b"GET /?code=fresh&state=expected HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut reply = String::new();
        browser.read_to_string(&mut reply).await.unwrap();
        assert!(reply.starts_with("HTTP/1.1 200"));
        assert_eq!(waiting.await.unwrap().unwrap(), "fresh");
    }
}
