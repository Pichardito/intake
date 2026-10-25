//! Two small credentials: the visitor's conversation token (random, stored argon2-hashed)
//! and the admin's session cookie (HMAC-signed JSON, nothing stored server-side).

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// 256 bits of randomness, URL-safe base64.
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> anyhow::Result<String> {
    let mut salt_bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|e| anyhow::anyhow!("salt: {e}"))?;
    let hash = Argon2::default()
        .hash_password(token.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2: {e}"))?;
    Ok(hash.to_string())
}

pub fn verify_token(token: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default().verify_password(token.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

type HmacSha256 = Hmac<Sha256>;

/// `base64url(payload).base64url(hmac_sha256(payload))`.
pub fn sign(payload: &[u8], secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(payload);
    let tag = mac.finalize().into_bytes();
    format!("{}.{}", URL_SAFE_NO_PAD.encode(payload), URL_SAFE_NO_PAD.encode(tag))
}

/// The payload if the signature checks out, else `None`.
pub fn verify_signed(value: &str, secret: &str) -> Option<Vec<u8>> {
    let (payload_b64, tag_b64) = value.split_once('.')?;
    let payload = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let tag = URL_SAFE_NO_PAD.decode(tag_b64).ok()?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(&payload);
    let expected = mac.finalize().into_bytes();
    if expected.as_slice().ct_eq(&tag).into() {
        Some(payload)
    } else {
        None
    }
}

pub const SESSION_COOKIE: &str = "intake_session";
pub const OAUTH_STATE_COOKIE: &str = "intake_oauth_state";
pub const SESSION_TTL_SECS: i64 = 8 * 60 * 60;
pub const OAUTH_STATE_TTL_SECS: i64 = 10 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub login: String,
    /// Unix seconds.
    pub exp: i64,
}

pub fn encode_session(session: &Session, secret: &str) -> String {
    sign(&serde_json::to_vec(session).expect("session serializes"), secret)
}

/// A valid, unexpired session, or `None`.
pub fn decode_session(value: &str, secret: &str, now_unix: i64) -> Option<Session> {
    let payload = verify_signed(value, secret)?;
    let session: Session = serde_json::from_slice(&payload).ok()?;
    (session.exp > now_unix).then_some(session)
}

/// Build a `Set-Cookie` value. `max_age` of zero clears the cookie.
pub fn cookie(name: &str, value: &str, max_age_secs: i64, secure: bool) -> String {
    let mut c = format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}");
    if secure {
        c.push_str("; Secure");
    }
    c
}

/// Read one cookie out of a `Cookie` header value.
pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.split(';').map(str::trim).find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name).then_some(v)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip_and_mismatch() {
        let t = new_token();
        assert!(t.len() >= 43);
        let h = hash_token(&t).unwrap();
        assert!(h.starts_with("$argon2"));
        assert!(verify_token(&t, &h));
        assert!(!verify_token("not-it", &h));
        assert!(!verify_token(&t, "garbage"));
    }

    #[test]
    fn signed_values_detect_tampering() {
        let secret = "0123456789abcdef0123456789abcdef";
        let v = sign(b"hello", secret);
        assert_eq!(verify_signed(&v, secret).as_deref(), Some(&b"hello"[..]));
        assert!(verify_signed(&v, "another-secret-another-secret-00").is_none());
        let mut forged = v.clone();
        forged.replace_range(0..1, if &v[0..1] == "a" { "b" } else { "a" });
        assert!(verify_signed(&forged, secret).is_none());
        assert!(verify_signed("no-dot", secret).is_none());
    }

    #[test]
    fn session_expiry_is_enforced() {
        let secret = "0123456789abcdef0123456789abcdef";
        let s = Session { login: "octocat".into(), exp: 1_000 };
        let v = encode_session(&s, secret);
        assert_eq!(decode_session(&v, secret, 999), Some(s));
        assert_eq!(decode_session(&v, secret, 1_000), None);
    }

    #[test]
    fn cookie_header_parsing() {
        let h = "a=1; intake_session=abc.def; z=9";
        assert_eq!(cookie_value(h, "intake_session"), Some("abc.def"));
        assert_eq!(cookie_value(h, "missing"), None);
        assert!(cookie("n", "v", 0, true).contains("Secure"));
        assert!(!cookie("n", "v", 0, false).contains("Secure"));
    }
}
