//! Blocking client for the sync API (feature `client`).
//!
//! Low-level calls map one-to-one to endpoints; [`signup`] and [`signin`] do
//! the key handling so callers only deal with a password.

use crate::api::*;
use crate::crypto::{new_salt, AccountKey, CryptoError, KdfParams, MasterKey};
use crate::Envelope;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug)]
pub enum ClientError {
    /// The server answered with an error status.
    Http {
        status: u16,
        message: String,
    },
    /// Couldn't reach the server (offline, DNS, TLS, timeout).
    Network(String),
    Crypto(CryptoError),
    Bad(String),
}

impl ClientError {
    pub fn status(&self) -> Option<u16> {
        match self {
            ClientError::Http { status, .. } => Some(*status),
            _ => None,
        }
    }
    /// Retrying later may help (offline, server busy or erroring).
    pub fn is_transient(&self) -> bool {
        match self {
            ClientError::Network(_) => true,
            ClientError::Http { status, .. } => *status == 429 || *status >= 500,
            _ => false,
        }
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Http { status, message } => write!(f, "{message} (HTTP {status})"),
            ClientError::Network(e) => write!(f, "can't reach the sync server: {e}"),
            ClientError::Crypto(e) => write!(f, "{e}"),
            ClientError::Bad(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<CryptoError> for ClientError {
    fn from(e: CryptoError) -> Self {
        ClientError::Crypto(e)
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;

pub struct Client {
    base: String,
    agent: ureq::Agent,
    access_token: Option<String>,
}

impl Client {
    /// `base` like `http://127.0.0.1:8787` or `https://sync.example.com`.
    pub fn new(base: &str) -> Client {
        Client {
            base: base.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout(Duration::from_secs(60))
                .user_agent(concat!("FocusTrack-sync/", env!("CARGO_PKG_VERSION")))
                .build(),
            access_token: None,
        }
    }

    pub fn set_access_token(&mut self, token: Option<String>) {
        self.access_token = token;
    }

    fn req(&self, method: &str, path: &str) -> ureq::Request {
        let r = self
            .agent
            .request(method, &format!("{}{}", self.base, path));
        match &self.access_token {
            Some(t) => r.set("Authorization", &format!("Bearer {t}")),
            None => r,
        }
    }

    fn finish<T: DeserializeOwned>(
        res: std::result::Result<ureq::Response, ureq::Error>,
    ) -> Result<T> {
        match res {
            Ok(r) => r
                .into_json()
                .map_err(|e| ClientError::Bad(format!("bad response: {e}"))),
            Err(ureq::Error::Status(status, r)) => {
                let message = r
                    .into_json::<ErrorBody>()
                    .map(|b| b.error)
                    .unwrap_or_else(|_| "server error".into());
                Err(ClientError::Http { status, message })
            }
            Err(ureq::Error::Transport(t)) => Err(ClientError::Network(t.to_string())),
        }
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        Self::finish(self.req("GET", path).call())
    }

    fn send<B: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: &B,
    ) -> Result<T> {
        Self::finish(self.req(method, path).send_json(body))
    }

    pub fn status(&self) -> Result<Status> {
        self.get("/v1/status")
    }

    pub fn auth_params(&self, email: &str) -> Result<AuthParams> {
        self.get(&format!("/v1/auth/params?email={}", urlencode(email)))
    }

    pub fn signup_raw(&self, req: &SignupRequest) -> Result<Session> {
        self.send("POST", "/v1/auth/signup", req)
    }

    pub fn signin_raw(&self, req: &SigninRequest) -> Result<Session> {
        self.send("POST", "/v1/auth/signin", req)
    }

    pub fn refresh(&self, req: &RefreshRequest) -> Result<AccessToken> {
        self.send("POST", "/v1/auth/refresh", req)
    }

    pub fn change_password_raw(&self, req: &ChangePasswordRequest) -> Result<serde_json::Value> {
        self.send("POST", "/v1/auth/password", req)
    }

    pub fn devices(&self) -> Result<Vec<DeviceInfo>> {
        self.get("/v1/devices")
    }

    pub fn remove_device(&self, id: &str) -> Result<serde_json::Value> {
        Self::finish(
            self.req("DELETE", &format!("/v1/devices/{}", urlencode(id)))
                .call(),
        )
    }

    pub fn push(&self, records: &[Envelope]) -> Result<PushResponse> {
        self.send(
            "POST",
            "/v1/sync/push",
            &PushRequest {
                records: records.to_vec(),
            },
        )
    }

    /// Records newer than `since`; `days` limits detailed kinds to recent days (first sync).
    pub fn pull(&self, since: i64, limit: usize, days: Option<u32>) -> Result<PullResponse> {
        let mut path = format!("/v1/sync/pull?since={since}&limit={limit}");
        if let Some(d) = days {
            path.push_str(&format!("&days={d}"));
        }
        self.get(&path)
    }

    /// Every record in the account, this device's own included (for export).
    pub fn pull_all(&self, since: i64, limit: usize) -> Result<PullResponse> {
        self.get(&format!("/v1/sync/pull?since={since}&limit={limit}&own=1"))
    }

    pub fn delete_data(&self, before: &str, device: Option<&str>) -> Result<DeleteDataResponse> {
        let mut path = format!("/v1/data?before={}", urlencode(before));
        if let Some(d) = device {
            path.push_str(&format!("&device={}", urlencode(d)));
        }
        Self::finish(self.req("DELETE", &path).call())
    }

    pub fn account(&self) -> Result<AccountInfo> {
        self.get("/v1/account")
    }

    /// How long the server keeps history (None = forever).
    pub fn set_retention(&self, days: Option<u32>) -> Result<AccountInfo> {
        self.send("PUT", "/v1/account/retention", &RetentionRequest { days })
    }

    /// Deletes the account and every record. Needs the password (auth key) again.
    pub fn delete_account(&self, auth_key: &str) -> Result<serde_json::Value> {
        self.send(
            "DELETE",
            "/v1/account",
            &serde_json::json!({ "auth_key": auth_key }),
        )
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Creates an account: a new account key, wrapped with the password. Returns
/// the session and the account key (show its recovery key to the user).
pub fn signup(
    client: &mut Client,
    email: &str,
    password: &str,
    kdf: KdfParams,
    device: NewDevice,
) -> Result<(Session, AccountKey)> {
    let salt = new_salt();
    let master = MasterKey::derive(password, &salt, &kdf)?;
    let key = AccountKey::generate();
    let session = client.signup_raw(&SignupRequest {
        email: email.into(),
        auth_key: master.auth_key(),
        salt,
        kdf,
        wrapped_key: key.wrap(&master),
        device,
    })?;
    client.set_access_token(Some(session.access_token.clone()));
    Ok((session, key))
}

/// Signs in on a (new) device and unwraps the account key with the password.
pub fn signin(
    client: &mut Client,
    email: &str,
    password: &str,
    device: NewDevice,
) -> Result<(Session, AccountKey)> {
    let params = client.auth_params(email)?;
    let master = MasterKey::derive(password, &params.salt, &params.kdf)?;
    let session = client.signin_raw(&SigninRequest {
        email: email.into(),
        auth_key: master.auth_key(),
        device,
    })?;
    let key = AccountKey::unwrap(&session.wrapped_key, &master)?;
    client.set_access_token(Some(session.access_token.clone()));
    Ok((session, key))
}

/// Re-wraps the account key under a new password.
pub fn change_password(
    client: &Client,
    email: &str,
    old_password: &str,
    new_password: &str,
    key: &AccountKey,
) -> Result<()> {
    let params = client.auth_params(email)?;
    let old = MasterKey::derive(old_password, &params.salt, &params.kdf)?;
    let new_salt = new_salt();
    let new_kdf = params.kdf.clone();
    let new = MasterKey::derive(new_password, &new_salt, &new_kdf)?;
    client.change_password_raw(&ChangePasswordRequest {
        auth_key: old.auth_key(),
        new_auth_key: new.auth_key(),
        new_salt,
        new_kdf,
        new_wrapped_key: key.wrap(&new),
    })?;
    Ok(())
}
