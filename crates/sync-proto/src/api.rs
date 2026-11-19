//! HTTP API request and response bodies (design §6.1). JSON over HTTPS.

use crate::crypto::KdfParams;
use crate::Envelope;
use serde::{Deserialize, Serialize};

/// Largest batch a client sends or a server returns.
pub const MAX_BATCH: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub app: String,
    pub protocol: u32,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}

/// `GET /v1/auth/params?email=` — what a device needs to derive the master key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthParams {
    pub salt: String,
    pub kdf: KdfParams,
}

/// A device introducing itself. Its name is encrypted separately (a `device` record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDevice {
    pub id: String,
    /// `windows` | `android` | `ios` | `macos` | `linux`
    pub platform: String,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub auth_key: String,
    pub salt: String,
    pub kdf: KdfParams,
    pub wrapped_key: String,
    pub device: NewDevice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigninRequest {
    pub email: String,
    pub auth_key: String,
    pub device: NewDevice,
}

/// Returned by sign-up and sign-in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub account_id: String,
    pub device_id: String,
    pub access_token: String,
    pub access_expires_in: u64,
    /// Long-lived, per device. Store it securely (DPAPI / Keystore / Keychain).
    pub refresh_token: String,
    pub wrapped_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub device_id: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessToken {
    pub access_token: String,
    pub access_expires_in: u64,
}

/// Change the password: re-wrapped account key and new auth key (the data isn't re-encrypted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub auth_key: String,
    pub new_auth_key: String,
    pub new_salt: String,
    pub new_kdf: KdfParams,
    pub new_wrapped_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceInfo {
    pub id: String,
    pub platform: String,
    pub app_version: String,
    pub created: String,
    pub last_seen: Option<String>,
    /// This is the device making the request.
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub records: Vec<Envelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ack {
    pub id: String,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rejected {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    /// Records the server now holds at (at least) this version. Mark these synced.
    pub acked: Vec<Ack>,
    /// Records the server refused; they won't succeed if re-sent unchanged.
    pub rejected: Vec<Rejected>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub records: Vec<Envelope>,
    /// Pass back as `since` next time (store it only after saving the records).
    pub cursor: i64,
    pub more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteDataResponse {
    pub deleted: u64,
}
