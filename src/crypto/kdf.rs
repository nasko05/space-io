use scrypt::{scrypt, Params};
use subtle::ConstantTimeEq;

use crate::error::{AppError, AppResult};

/// Default scrypt parameters for the verifier hash, tuned for ~150 ms on a
/// recent laptop: fine for a manual unlock, painful for offline brute-force.
pub const DEFAULT_LOG_N: u8 = 15;
pub const DEFAULT_R: u32 = 8;
pub const DEFAULT_P: u32 = 1;
pub const VERIFIER_LEN: usize = 32;

pub fn derive_verifier(
    passphrase: &str,
    salt: &[u8],
    log_n: u8,
    r: u32,
    p: u32,
) -> AppResult<[u8; VERIFIER_LEN]> {
    let params = Params::new(log_n, r, p, VERIFIER_LEN)
        .map_err(|e| AppError::Internal(format!("scrypt params: {e}")))?;
    let mut out = [0u8; VERIFIER_LEN];
    scrypt(passphrase.as_bytes(), salt, &params, &mut out)
        .map_err(|e| AppError::Internal(format!("scrypt: {e}")))?;
    Ok(out)
}

pub fn verify(supplied: &[u8; VERIFIER_LEN], expected: &[u8; VERIFIER_LEN]) -> bool {
    supplied.ct_eq(expected).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SALT: &[u8] = b"test-salt-16-byt";

    #[test]
    fn derivation_is_deterministic() {
        let a = derive_verifier("passphrase", SALT, 4, 8, 1).unwrap();
        let b = derive_verifier("passphrase", SALT, 4, 8, 1).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn different_passphrases_produce_different_verifiers() {
        let a = derive_verifier("one", SALT, 4, 8, 1).unwrap();
        let b = derive_verifier("two", SALT, 4, 8, 1).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn different_salts_produce_different_verifiers() {
        let a = derive_verifier("p", b"salt-A-aaaaaaaaa", 4, 8, 1).unwrap();
        let b = derive_verifier("p", b"salt-B-bbbbbbbbb", 4, 8, 1).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn invalid_params_error() {
        let err = derive_verifier("p", SALT, 64, 8, 1).unwrap_err();
        assert!(matches!(err, crate::error::AppError::Internal(_)));
    }

    #[test]
    fn verify_matches_identical_buffers() {
        let v = derive_verifier("p", SALT, 4, 8, 1).unwrap();
        assert!(verify(&v, &v));
    }

    #[test]
    fn verify_rejects_one_bit_difference() {
        let mut a = derive_verifier("p", SALT, 4, 8, 1).unwrap();
        let b = a;
        a[0] ^= 0x01;
        assert!(!verify(&a, &b));
    }
}
