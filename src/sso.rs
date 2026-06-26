//! Verification of the cloud-drive single sign-on (SSO) token.
//!
//! The drive (the FastAPI cloud-storage app) is the login authority. On a
//! passkey or password login it issues an HS256 JWT and mirrors it into a
//! cookie shared across the registrable parent domain, so the editor — served
//! from a sibling subdomain — sees the same session. The editor never runs its
//! own SSO login; it only needs to *verify* that token to learn who the visitor
//! is.
//!
//! Verification is a small, self-contained HMAC-SHA256 check (the same
//! primitive PyJWT uses for HS256) so we avoid pulling a full JWT/`ring` stack
//! into the binary. The shared secret matches the drive's `DRIVE_SECRET_KEY`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// Identity recovered from a valid SSO token. `sub` is the drive's stable user
/// id; `email` is informational (the drive always includes it today).
#[derive(Debug, Clone)]
pub struct SsoClaims {
    pub sub: String,
    pub email: Option<String>,
}

#[derive(Deserialize)]
struct RawClaims {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    exp: i64,
}

#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
}

/// SSO configuration sourced from the environment. Without a shared secret SSO
/// is disabled and every token is treated as absent.
pub struct SsoConfig {
    secret: Option<Vec<u8>>,
    /// Name of the cookie the drive sets; must match `DRIVE_SSO_COOKIE_NAME`.
    pub cookie_name: String,
}

impl SsoConfig {
    pub fn from_env() -> Self {
        let secret = std::env::var("SPACEIO_SSO_JWT_SECRET")
            .ok()
            .filter(|s| !s.is_empty())
            .map(String::into_bytes);
        let cookie_name = std::env::var("SPACEIO_SSO_COOKIE_NAME")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "drive_sso".to_string());
        Self {
            secret,
            cookie_name,
        }
    }

    /// Verify a compact-JWS HS256 token and return its claims, or `None` if the
    /// algorithm, structure, signature, or expiry is invalid (or SSO is off).
    pub fn verify_token(&self, token: &str) -> Option<SsoClaims> {
        let secret = self.secret.as_deref()?;

        let mut parts = token.split('.');
        let header_b64 = parts.next()?;
        let payload_b64 = parts.next()?;
        let signature_b64 = parts.next()?;
        if parts.next().is_some() {
            return None; // a compact JWS has exactly three segments
        }

        // Pin the algorithm so a forged `alg: none` (or RS/HS confusion) can't
        // bypass the MAC check below.
        let header_bytes = URL_SAFE_NO_PAD.decode(header_b64).ok()?;
        let header: JwtHeader = serde_json::from_slice(&header_bytes).ok()?;
        if header.alg != "HS256" {
            return None;
        }

        // Recompute the MAC over `header.payload`; `verify_slice` compares in
        // constant time and rejects a wrong-length signature.
        let provided = URL_SAFE_NO_PAD.decode(signature_b64).ok()?;
        let signed = format!("{header_b64}.{payload_b64}");
        let mut mac = HmacSha256::new_from_slice(secret).ok()?;
        mac.update(signed.as_bytes());
        mac.verify_slice(&provided).ok()?;

        let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
        let claims: RawClaims = serde_json::from_slice(&payload_bytes).ok()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        if claims.exp <= now {
            return None;
        }
        Some(SsoClaims {
            sub: claims.sub,
            email: claims.email,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mint an HS256 token the same way the drive (PyJWT) does, for tests.
    fn mint(secret: &[u8], header_json: &str, payload_json: &str) -> String {
        let h = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
        let p = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        let signing_input = format!("{h}.{p}");
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(signing_input.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("{signing_input}.{sig}")
    }

    fn cfg(secret: &str) -> SsoConfig {
        SsoConfig {
            secret: Some(secret.as_bytes().to_vec()),
            cookie_name: "drive_sso".into(),
        }
    }

    fn future() -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        now + 3600
    }

    #[test]
    fn accepts_a_valid_token() {
        let exp = future();
        let token = mint(
            b"shared-secret",
            r#"{"alg":"HS256","typ":"JWT"}"#,
            &format!(r#"{{"sub":"user-1","email":"a@b.c","iat":0,"exp":{exp}}}"#),
        );
        let claims = cfg("shared-secret").verify_token(&token).expect("valid");
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.email.as_deref(), Some("a@b.c"));
    }

    #[test]
    fn rejects_wrong_secret() {
        let exp = future();
        let token = mint(
            b"shared-secret",
            r#"{"alg":"HS256","typ":"JWT"}"#,
            &format!(r#"{{"sub":"u","exp":{exp}}}"#),
        );
        assert!(cfg("other-secret").verify_token(&token).is_none());
    }

    #[test]
    fn rejects_expired_token() {
        let token = mint(
            b"shared-secret",
            r#"{"alg":"HS256","typ":"JWT"}"#,
            r#"{"sub":"u","exp":1}"#,
        );
        assert!(cfg("shared-secret").verify_token(&token).is_none());
    }

    #[test]
    fn rejects_alg_none() {
        // `alg: none` with an empty signature must never authenticate.
        let h = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let p = URL_SAFE_NO_PAD.encode(format!(r#"{{"sub":"u","exp":{}}}"#, future()).as_bytes());
        let token = format!("{h}.{p}.");
        assert!(cfg("shared-secret").verify_token(&token).is_none());
    }

    #[test]
    fn rejects_garbage_and_disabled() {
        assert!(cfg("s").verify_token("not-a-jwt").is_none());
        let disabled = SsoConfig {
            secret: None,
            cookie_name: "drive_sso".into(),
        };
        let token = mint(
            b"s",
            r#"{"alg":"HS256"}"#,
            &format!(r#"{{"sub":"u","exp":{}}}"#, future()),
        );
        assert!(disabled.verify_token(&token).is_none());
    }
}
