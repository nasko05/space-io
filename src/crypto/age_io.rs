use std::io::{Read, Write};

use age::secrecy::SecretString;
use age::{Decryptor, Encryptor};

use crate::error::{AppError, AppResult};

pub fn encrypt_bytes(plaintext: &[u8], passphrase: &SecretString) -> AppResult<Vec<u8>> {
    let encryptor = Encryptor::with_user_passphrase(passphrase.clone());
    let mut out = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut out)
        .map_err(|e| AppError::Internal(format!("age wrap: {e}")))?;
    writer
        .write_all(plaintext)
        .map_err(|e| AppError::Internal(format!("age write: {e}")))?;
    writer
        .finish()
        .map_err(|e| AppError::Internal(format!("age finish: {e}")))?;
    Ok(out)
}

pub fn decrypt_bytes(ciphertext: &[u8], passphrase: &SecretString) -> AppResult<Vec<u8>> {
    let decryptor = match Decryptor::new(ciphertext) {
        Ok(Decryptor::Passphrase(d)) => d,
        Ok(Decryptor::Recipients(_)) => {
            return Err(AppError::Internal(
                "file expects a recipient identity, not a passphrase".into(),
            ));
        }
        Err(e) => return Err(AppError::Internal(format!("age decryptor: {e}"))),
    };

    // `None` caps the accepted work factor at ~16 seconds of CPU. age's
    // encryption side auto-tunes to ~1 second on the writer's hardware, so
    // a note written on a fast box may legitimately need 30s+ to decrypt on
    // a slow one. Allow up to log_n=22 (~2 minutes on a modern CPU) — this
    // still rejects pathologically high work factors that would constitute
    // a DoS, but it accepts every file we produce ourselves.
    let mut reader = decryptor
        .decrypt(passphrase, Some(22))
        .map_err(|e| AppError::Internal(format!("age decrypt: {e}")))?;
    let mut out = Vec::new();
    reader
        .read_to_end(&mut out)
        .map_err(|e| AppError::Internal(format!("age read: {e}")))?;
    Ok(out)
}
