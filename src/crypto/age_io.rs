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

/// Decrypts an age passphrase-wrapped blob.
///
/// The accepted scrypt work factor is capped at `MAX_DECRYPT_LOG_N`: high enough
/// to decrypt anything we wrote (age auto-tunes the writer's factor to its own
/// hardware), low enough to reject a pathologically expensive file as a DoS.
pub fn decrypt_bytes(ciphertext: &[u8], passphrase: &SecretString) -> AppResult<Vec<u8>> {
    const MAX_DECRYPT_LOG_N: u8 = 22;

    let decryptor = match Decryptor::new(ciphertext) {
        Ok(Decryptor::Passphrase(d)) => d,
        Ok(Decryptor::Recipients(_)) => {
            return Err(AppError::Internal(
                "file expects a recipient identity, not a passphrase".into(),
            ));
        }
        Err(e) => return Err(AppError::Internal(format!("age decryptor: {e}"))),
    };

    let mut reader = decryptor
        .decrypt(passphrase, Some(MAX_DECRYPT_LOG_N))
        .map_err(|e| AppError::Internal(format!("age decrypt: {e}")))?;
    let mut out = Vec::new();
    reader
        .read_to_end(&mut out)
        .map_err(|e| AppError::Internal(format!("age read: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(s: &str) -> SecretString {
        SecretString::from(s.to_string())
    }

    #[test]
    fn encrypt_decrypt_roundtrip_is_lossless() {
        let pass = secret("correct horse battery staple");
        let plaintext = b"# Hello\n\nThe *quick* brown fox jumps over the lazy dog.";
        let ct = encrypt_bytes(plaintext, &pass).unwrap();
        assert_ne!(ct.as_slice(), &plaintext[..]);
        assert!(ct.starts_with(b"age-encryption.org/v1\n"));
        let pt = decrypt_bytes(&ct, &pass).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn wrong_passphrase_fails_to_decrypt() {
        let right = secret("right one");
        let wrong = secret("wrong one");
        let ct = encrypt_bytes(b"secret note", &right).unwrap();
        let err = decrypt_bytes(&ct, &wrong).unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }

    #[test]
    fn empty_plaintext_roundtrips() {
        let pass = secret("p");
        let ct = encrypt_bytes(b"", &pass).unwrap();
        let pt = decrypt_bytes(&ct, &pass).unwrap();
        assert_eq!(pt.as_slice(), b"");
    }

    #[test]
    fn truncated_ciphertext_is_rejected() {
        let pass = secret("p");
        let ct = encrypt_bytes(b"hello", &pass).unwrap();
        let truncated_past_aead_tag = &ct[..ct.len().saturating_sub(8)];
        assert!(decrypt_bytes(truncated_past_aead_tag, &pass).is_err());
    }
}
