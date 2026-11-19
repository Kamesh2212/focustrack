//! End-to-end encryption (design §5).
//!
//! ```text
//! password ─Argon2id─► master key ─HKDF "auth"─► auth key   (sent at sign-in; server stores its hash)
//!                                 └HKDF "wrap"─► wrap key   (encrypts the account key; never sent)
//! account key (random) ─HKDF "records"─► record key ─XChaCha20-Poly1305─► payloads
//! ```
//! The server never sees the password, the master key, the wrap key or the
//! account key. Each payload's associated data binds it to its envelope, so a
//! server can't move a payload to another record, day or version.

use crate::Envelope;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Debug, PartialEq, Eq)]
pub enum CryptoError {
    /// Wrong key or tampered data.
    Decrypt,
    Encoding,
    Kdf(String),
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::Decrypt => write!(f, "couldn't decrypt (wrong key or damaged data)"),
            CryptoError::Encoding => write!(f, "bad encoding"),
            CryptoError::Kdf(e) => write!(f, "key derivation failed: {e}"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// Argon2id cost. Stored with the account so it can be raised later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl KdfParams {
    /// 64 MiB, 3 passes: about a second on a laptop, acceptable on a phone.
    pub fn standard() -> KdfParams {
        KdfParams {
            m_kib: 64 * 1024,
            t: 3,
            p: 1,
        }
    }

    /// Cheap parameters for tests only.
    pub fn insecure_for_tests() -> KdfParams {
        KdfParams {
            m_kib: 64,
            t: 1,
            p: 1,
        }
    }

    pub fn is_acceptable(&self) -> bool {
        // Servers reject accounts below the test floor and absurd values.
        (64..=1024 * 1024).contains(&self.m_kib)
            && (1..=10).contains(&self.t)
            && (1..=4).contains(&self.p)
    }
}

/// A random 16-byte salt, base64.
pub fn new_salt() -> String {
    let mut s = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut s);
    B64.encode(s)
}

fn hkdf(ikm: &[u8], info: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(None, ikm)
        .expand(info.as_bytes(), &mut out)
        .expect("32 bytes is a valid HKDF length");
    out
}

/// Derived from the password; stays on the device.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    pub fn derive(
        password: &str,
        salt_b64: &str,
        params: &KdfParams,
    ) -> Result<MasterKey, CryptoError> {
        let salt = B64.decode(salt_b64).map_err(|_| CryptoError::Encoding)?;
        let argon = argon2::Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(params.m_kib, params.t, params.p, Some(32))
                .map_err(|e| CryptoError::Kdf(e.to_string()))?,
        );
        let mut out = [0u8; 32];
        argon
            .hash_password_into(password.as_bytes(), &salt, &mut out)
            .map_err(|e| CryptoError::Kdf(e.to_string()))?;
        Ok(MasterKey(out))
    }

    /// Proves knowledge of the password to the server (base64).
    pub fn auth_key(&self) -> String {
        B64.encode(hkdf(&self.0, "focustrack auth v1"))
    }

    fn wrap_cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&hkdf(&self.0, "focustrack wrap v1").into())
    }
}

/// What the server stores instead of the auth key: SHA-256, hex.
/// (The auth key is already a slow-hashed, high-entropy secret.)
pub fn auth_key_hash(auth_key_b64: &str) -> String {
    let digest = Sha256::digest(auth_key_b64.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The account's data key. Same on every device of the account.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct AccountKey([u8; 32]);

impl AccountKey {
    pub fn generate() -> AccountKey {
        let mut k = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut k);
        AccountKey(k)
    }

    pub fn from_bytes(b: [u8; 32]) -> AccountKey {
        AccountKey(b)
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Encrypts this key with the password-derived wrap key, for the server to store.
    pub fn wrap(&self, master: &MasterKey) -> String {
        seal_raw(&master.wrap_cipher(), &self.0, b"focustrack account key")
    }

    pub fn unwrap(wrapped: &str, master: &MasterKey) -> Result<AccountKey, CryptoError> {
        let raw = open_raw(&master.wrap_cipher(), wrapped, b"focustrack account key")?;
        let arr: [u8; 32] = raw
            .as_slice()
            .try_into()
            .map_err(|_| CryptoError::Encoding)?;
        Ok(AccountKey(arr))
    }

    fn record_cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&hkdf(&self.0, "focustrack records v1").into())
    }
}

fn seal_raw(cipher: &XChaCha20Poly1305, plain: &[u8], aad: &[u8]) -> String {
    let mut nonce = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .expect("encryption with a valid key doesn't fail");
    let mut out = Vec::with_capacity(24 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    B64.encode(out)
}

fn open_raw(cipher: &XChaCha20Poly1305, b64: &str, aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let raw = B64.decode(b64).map_err(|_| CryptoError::Encoding)?;
    if raw.len() < 24 + 16 {
        return Err(CryptoError::Encoding);
    }
    let (nonce, ct) = raw.split_at(24);
    cipher
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad })
        .map_err(|_| CryptoError::Decrypt)
}

/// Associated data: everything in the envelope except the payload.
fn aad(env: &Envelope) -> Vec<u8> {
    format!(
        "focustrack/1|{}|{}|{}|{}|{}|{}|{}",
        env.id,
        env.kind,
        env.device,
        env.day,
        env.version,
        env.hlc.as_deref().unwrap_or(""),
        env.deleted
    )
    .into_bytes()
}

impl Envelope {
    /// Builds an envelope and encrypts `body` into it.
    #[allow(clippy::too_many_arguments)]
    pub fn seal<T: Serialize>(
        key: &AccountKey,
        id: &str,
        kind: &str,
        device: &str,
        day: &str,
        version: i64,
        hlc: Option<String>,
        deleted: bool,
        body: &T,
    ) -> Envelope {
        let mut env = Envelope {
            id: id.into(),
            kind: kind.into(),
            device: device.into(),
            day: day.into(),
            version,
            hlc,
            deleted,
            payload: String::new(),
        };
        let plain = serde_json::to_vec(body).expect("record bodies serialise");
        env.payload = seal_raw(&key.record_cipher(), &plain, &aad(&env));
        env
    }

    /// Decrypts and parses the body. Fails if anything in the envelope was changed.
    pub fn open<T: DeserializeOwned>(&self, key: &AccountKey) -> Result<T, CryptoError> {
        let plain = open_raw(&key.record_cipher(), &self.payload, &aad(self))?;
        serde_json::from_slice(&plain).map_err(|_| CryptoError::Encoding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn master(pw: &str, salt: &str) -> MasterKey {
        MasterKey::derive(pw, salt, &KdfParams::insecure_for_tests()).unwrap()
    }

    #[test]
    fn password_unwraps_account_key_wrong_password_doesnt() {
        let salt = new_salt();
        let key = AccountKey::generate();
        let wrapped = key.wrap(&master("correct horse", &salt));
        let back = AccountKey::unwrap(&wrapped, &master("correct horse", &salt)).unwrap();
        assert_eq!(back.bytes(), key.bytes());
        assert_eq!(
            AccountKey::unwrap(&wrapped, &master("wrong", &salt)).err(),
            Some(CryptoError::Decrypt)
        );
    }

    #[test]
    fn auth_key_is_stable_and_not_the_wrap_key() {
        let salt = new_salt();
        let a = master("pw", &salt).auth_key();
        assert_eq!(a, master("pw", &salt).auth_key());
        assert_ne!(a, master("pw2", &salt).auth_key());
        assert_ne!(a, master("pw", &new_salt()).auth_key(), "salt matters");
        assert_eq!(auth_key_hash(&a).len(), 64);
    }

    #[test]
    fn records_round_trip_and_detect_tampering() {
        let key = AccountKey::generate();
        let env = Envelope::seal(
            &key,
            &crate::new_id(),
            "usage",
            &crate::new_id(),
            "2026-11-19",
            3,
            None,
            false,
            &json!({"app": "win:code.exe", "secs": 600}),
        );
        let body: serde_json::Value = env.open(&key).unwrap();
        assert_eq!(body["secs"], 600);

        // The server moves the payload to another day or version: decryption fails.
        for tampered in [
            Envelope {
                day: "2026-11-20".into(),
                ..env.clone()
            },
            Envelope {
                version: 4,
                ..env.clone()
            },
            Envelope {
                deleted: true,
                ..env.clone()
            },
            Envelope {
                id: crate::new_id(),
                ..env.clone()
            },
        ] {
            assert_eq!(
                tampered.open::<serde_json::Value>(&key).err(),
                Some(CryptoError::Decrypt)
            );
        }
        // Another account's key can't read it.
        assert!(env
            .open::<serde_json::Value>(&AccountKey::generate())
            .is_err());
    }

    #[test]
    fn standard_params_are_acceptable() {
        assert!(KdfParams::standard().is_acceptable());
        assert!(KdfParams::insecure_for_tests().is_acceptable());
        assert!(!KdfParams {
            m_kib: 8,
            t: 1,
            p: 1
        }
        .is_acceptable());
    }
}
