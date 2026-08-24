//! What travels between a device and the server, and how it is sealed.
//!
//! The server is a courier. It stores an ordered log of sealed envelopes and
//! hands them back by sequence number; it cannot read one, and nothing in this
//! crate gives it the means to. That is the whole reason the sealing lives
//! here, in a crate the server links for the wire types, rather than in the
//! server itself.
//!
//! Two consequences worth being explicit about, because they are the point:
//!
//! * The hosting provider holds ciphertext. A dump of the database, a
//!   subpoena, or a mistake in the platform's access control yields nothing but
//!   opaque blobs and timing. This is what makes it defensible to put a private
//!   contact archive on rented infrastructure while ADR-018 (encryption at rest
//!   on the desktop) is still deferred.
//!
//! * **A lost passphrase is unrecoverable.** There is no reset, because there
//!   is nobody holding a copy to reset it with. The device's own database is
//!   unencrypted and the daily backup still works, so losing the passphrase
//!   costs the ability to sync, not the archive — but that distinction has to
//!   be stated where the user will see it.

use argon2::Argon2;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("סנכרון: פענוח נכשל — ככל הנראה סיסמה שונה במכשיר הזה")]
    Decrypt,
    #[error("סנכרון: המפתח אינו תקין")]
    Key,
    #[error("סנכרון: קוד החיבור אינו תקין")]
    ConnectionCode,
    #[error("סנכרון: {0}")]
    Encoding(String),
}

pub type Result<T> = std::result::Result<T, ProtoError>;

/// One sealed change, as the server stores it.
///
/// Everything the server can read is here: an id, which device sent it, when,
/// and how long it is. That metadata is deliberate — the id is what makes
/// redelivery harmless, and the ordering is the whole service being provided —
/// but it is also the entire extent of what leaks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// The mutation id, carried in the clear so a device can recognise a change
    /// it has already applied without decrypting it first.
    pub id: String,
    pub device_id: String,
    pub created_at: String,
    /// Base64url, unpadded.
    pub nonce: String,
    /// Base64url, unpadded.
    pub ciphertext: String,
}

/// Where a device has read up to. Monotonic, assigned by the server.
pub type Cursor = i64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    /// The shared secret set on the server at deploy time. Sent once, over TLS,
    /// and exchanged for a token — so it is not repeated on every request and a
    /// single captured request cannot re-enrol a device.
    pub enrolment_secret: String,
    pub device_name: String,
    /// `desktop`, `android`, `ios` or `web`.
    pub device_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterResponse {
    pub device_id: String,
    /// Presented as a bearer token afterwards. Shown once; the server keeps
    /// only a hash.
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    pub envelopes: Vec<Envelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResponse {
    /// Ids the server has durably stored. A device may only mark these settled.
    pub accepted: Vec<String>,
    /// The server's highest sequence number after the push.
    pub cursor: Cursor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullResponse {
    pub envelopes: Vec<Envelope>,
    /// Where to resume. Equal to the request's cursor when nothing came back.
    pub cursor: Cursor,
    /// Whether more is waiting beyond this page.
    pub has_more: bool,
}

/// The key that seals payloads. Never sent anywhere.
#[derive(Clone)]
pub struct SyncKey([u8; 32]);

impl std::fmt::Debug for SyncKey {
    /// Opaque on purpose: a key that prints itself ends up in a log file, and
    /// then in a bug report.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SyncKey(…)")
    }
}

impl SyncKey {
    /// A fresh random key, for the first device to set up.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Derive a key from something a person can type.
    ///
    /// Argon2id rather than a plain hash, because the alternative is that the
    /// strength of the encryption is capped by how fast an attacker who has the
    /// ciphertext can guess passphrases — and a passphrase a human chose and
    /// can retype on a phone is not a strong secret to begin with.
    pub fn derive(passphrase: &str, salt: &str) -> Result<Self> {
        let mut bytes = [0u8; 32];
        Argon2::default()
            .hash_password_into(passphrase.as_bytes(), salt.as_bytes(), &mut bytes)
            .map_err(|_| ProtoError::Key)?;
        Ok(Self(bytes))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Seal one change.
    ///
    /// A fresh random nonce every time. XChaCha20's nonce is wide enough
    /// (192 bits) that random generation is safe without tracking a counter,
    /// which matters here because several devices seal under the same key with
    /// no way to coordinate.
    pub fn seal(
        &self,
        id: &str,
        device_id: &str,
        created_at: &str,
        plaintext: &[u8],
    ) -> Result<Envelope> {
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&self.0));
        let mut nonce = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut nonce);

        let ciphertext =
            cipher.encrypt(XNonce::from_slice(&nonce), plaintext).map_err(|_| ProtoError::Key)?;

        Ok(Envelope {
            id: id.to_string(),
            device_id: device_id.to_string(),
            created_at: created_at.to_string(),
            nonce: URL_SAFE_NO_PAD.encode(nonce),
            ciphertext: URL_SAFE_NO_PAD.encode(ciphertext),
        })
    }

    /// Open one change.
    ///
    /// Fails rather than returning garbage when the key is wrong — the AEAD tag
    /// is checked before anything is handed back, so a device configured with a
    /// different passphrase reports it instead of writing nonsense into the
    /// archive.
    pub fn open(&self, envelope: &Envelope) -> Result<Vec<u8>> {
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&self.0));
        let nonce = URL_SAFE_NO_PAD
            .decode(&envelope.nonce)
            .map_err(|error| ProtoError::Encoding(error.to_string()))?;
        let ciphertext = URL_SAFE_NO_PAD
            .decode(&envelope.ciphertext)
            .map_err(|error| ProtoError::Encoding(error.to_string()))?;
        if nonce.len() != 24 {
            return Err(ProtoError::Decrypt);
        }
        cipher
            .decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| ProtoError::Decrypt)
    }
}

/// Everything a second device needs, as one string to copy across.
///
/// Setting up another machine should be one paste, not a form. Two separate
/// secrets — the server's enrolment secret and the data key — is the correct
/// design (the server must never hold the second one) and a bad thing to ask a
/// person to transcribe twice, so they travel together and are split on
/// arrival. Only the enrolment half is ever sent to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionCode {
    pub server_url: String,
    pub enrolment_secret: String,
    pub key: [u8; 32],
}

impl ConnectionCode {
    pub fn encode(&self) -> String {
        let joined = format!(
            "{}\n{}\n{}",
            self.server_url,
            self.enrolment_secret,
            URL_SAFE_NO_PAD.encode(self.key)
        );
        format!("yanuka1_{}", URL_SAFE_NO_PAD.encode(joined))
    }

    pub fn decode(code: &str) -> Result<Self> {
        let body = code.trim().strip_prefix("yanuka1_").ok_or(ProtoError::ConnectionCode)?;
        let decoded = URL_SAFE_NO_PAD.decode(body).map_err(|_| ProtoError::ConnectionCode)?;
        let text = String::from_utf8(decoded).map_err(|_| ProtoError::ConnectionCode)?;

        let mut parts = text.splitn(3, '\n');
        let server_url = parts.next().ok_or(ProtoError::ConnectionCode)?.to_string();
        let enrolment_secret = parts.next().ok_or(ProtoError::ConnectionCode)?.to_string();
        let key_part = parts.next().ok_or(ProtoError::ConnectionCode)?;

        let bytes = URL_SAFE_NO_PAD.decode(key_part).map_err(|_| ProtoError::ConnectionCode)?;
        let key: [u8; 32] = bytes.try_into().map_err(|_| ProtoError::ConnectionCode)?;

        if server_url.is_empty() || enrolment_secret.is_empty() {
            return Err(ProtoError::ConnectionCode);
        }
        Ok(Self { server_url, enrolment_secret, key })
    }
}

/// A random secret, printable and safe to paste.
pub fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_change_comes_back_intact() {
        let key = SyncKey::generate();
        let plaintext = r#"{"city":"ירושלים"}"#.as_bytes();
        let envelope = key.seal("01ABC", "device-a", "2026-08-24T10:00:00Z", plaintext).unwrap();

        assert_eq!(key.open(&envelope).unwrap(), plaintext);
        // The metadata the server sorts and deduplicates by stays legible.
        assert_eq!(envelope.id, "01ABC");
        assert_eq!(envelope.device_id, "device-a");
    }

    #[test]
    fn the_payload_is_not_readable_without_the_key() {
        // The claim the whole hosting decision rests on: what sits in the
        // provider's database is not the contact.
        let key = SyncKey::generate();
        let envelope = key.seal("01ABC", "device-a", "now", "ירושלים".as_bytes()).unwrap();

        assert!(!envelope.ciphertext.contains("ירושלים"));
        let decoded = URL_SAFE_NO_PAD.decode(&envelope.ciphertext).unwrap();
        assert!(!String::from_utf8_lossy(&decoded).contains("ירושלים"));
    }

    #[test]
    fn a_different_key_fails_loudly_rather_than_returning_rubbish() {
        // A device set up with the wrong passphrase must say so. Returning
        // plausible-looking bytes would write corruption into the archive, and
        // corruption in a contact record is not obvious on sight.
        let envelope = SyncKey::generate().seal("01ABC", "device-a", "now", b"secret").unwrap();
        assert!(matches!(SyncKey::generate().open(&envelope), Err(ProtoError::Decrypt)));
    }

    #[test]
    fn tampering_with_the_ciphertext_is_detected() {
        let key = SyncKey::generate();
        let mut envelope = key.seal("01ABC", "device-a", "now", b"a phone number").unwrap();
        let mut raw = URL_SAFE_NO_PAD.decode(&envelope.ciphertext).unwrap();
        raw[0] ^= 0x01;
        envelope.ciphertext = URL_SAFE_NO_PAD.encode(raw);

        assert!(matches!(key.open(&envelope), Err(ProtoError::Decrypt)));
    }

    #[test]
    fn sealing_the_same_thing_twice_produces_different_ciphertext() {
        // Otherwise the server could tell that two devices sent the same edit,
        // and with a small enough set of likely values, tell which one.
        let key = SyncKey::generate();
        let first = key.seal("01A", "device-a", "now", b"same").unwrap();
        let second = key.seal("01B", "device-a", "now", b"same").unwrap();
        assert_ne!(first.ciphertext, second.ciphertext);
    }

    #[test]
    fn the_same_passphrase_derives_the_same_key_on_every_device() {
        let first = SyncKey::derive("סיסמה ארוכה של המשתמש", "yanuka-data-v1").unwrap();
        let second = SyncKey::derive("סיסמה ארוכה של המשתמש", "yanuka-data-v1").unwrap();
        assert_eq!(first.as_bytes(), second.as_bytes());

        let other = SyncKey::derive("סיסמה אחרת לגמרי", "yanuka-data-v1").unwrap();
        assert_ne!(first.as_bytes(), other.as_bytes());
    }

    #[test]
    fn a_connection_code_survives_the_round_trip() {
        let code = ConnectionCode {
            server_url: "https://sync.example.com".into(),
            enrolment_secret: random_secret(),
            key: *SyncKey::generate().as_bytes(),
        };
        assert_eq!(ConnectionCode::decode(&code.encode()).unwrap(), code);
    }

    #[test]
    fn a_mistyped_connection_code_is_rejected() {
        // Pasted by hand on a phone, so a truncated or mangled code is the
        // normal failure. It must not produce a key that silently encrypts
        // everything into something no other device can read.
        assert!(ConnectionCode::decode("yanuka1_not-base64!!").is_err());
        assert!(ConnectionCode::decode("https://sync.example.com").is_err());
        assert!(ConnectionCode::decode("yanuka1_").is_err());

        let valid = ConnectionCode {
            server_url: "https://sync.example.com".into(),
            enrolment_secret: "secret".into(),
            key: [7u8; 32],
        }
        .encode();
        assert!(ConnectionCode::decode(&valid[..valid.len() - 4]).is_err());
    }
}
