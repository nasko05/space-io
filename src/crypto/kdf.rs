use scrypt::{scrypt, Params};
use subtle::ConstantTimeEq;

use crate::error::{AppError, AppResult};

/// Default scrypt parameters for the verifier hash.
/// Tuned for ~150 ms on a recent laptop — comfortable for a manual unlock,
/// painful for offline brute-force.
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
