use std::path::PathBuf;

use age::secrecy::SecretString;
use rand::RngCore;

use crate::config::SpaceConfig;
use crate::crypto::{age_io, kdf};
use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;

const SEED_REL_PATH: &str = "Journal/2026/welcome.md";
const SEED_CONTENT: &str = "# Welcome to your space

Sunday · 27 May 2026

This is your first note. Everything you write here lives on disk under your
own roof, encrypted with the passphrase you just chose. Nothing leaves the
machine unless you ask it to.

## A few things to know

- Markdown is canonical. *Italics* and **bold** work the way you'd expect.
- Wikilinks like [[On memory palaces]] will connect notes together once you
  write more of them.
- Every save is a commit. You can rewind without ceremony.

> *\"To pay attention, this is our endless and proper work.\"*
> — Mary Oliver

## Next

1. Lock the door and walk away (the cookie clears in 8 hours anyway).
2. Come back tomorrow and write the second note.
3. Don't tell anyone the passphrase.
";

pub struct InitOptions {
    pub space_dir: PathBuf,
    pub passphrase: SecretString,
    pub owner: String,
}

pub fn init_space(opts: InitOptions) -> AppResult<()> {
    let space_dir = &opts.space_dir;
    let config_path = SpaceConfig::config_path(space_dir);
    if config_path.exists() {
        return Err(AppError::BadRequest(format!(
            "space already initialised at {}",
            space_dir.display()
        )));
    }

    std::fs::create_dir_all(space_dir)?;
    let root = SpaceConfig::space_root(space_dir);
    std::fs::create_dir_all(&root)?;

    let mut salt_verify = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt_verify);
    let verifier = kdf::derive_verifier(
        age::secrecy::ExposeSecret::expose_secret(&opts.passphrase),
        &salt_verify,
        kdf::DEFAULT_LOG_N,
        kdf::DEFAULT_R,
        kdf::DEFAULT_P,
    )?;

    let cfg = SpaceConfig {
        owner: opts.owner,
        salt_verify_hex: hex::encode(salt_verify),
        verifier_hash_hex: hex::encode(verifier),
        kdf_log_n: kdf::DEFAULT_LOG_N,
        kdf_r: kdf::DEFAULT_R,
        kdf_p: kdf::DEFAULT_P,
        passkey: None,
    };
    cfg.save(space_dir)?;

    git2::Repository::init(&root).map_err(|e| AppError::Internal(format!("git init: {e}")))?;

    let seed_full_rel = format!("{SEED_REL_PATH}.age");
    let seed_path = root.join(&seed_full_rel);
    if let Some(parent) = seed_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let ciphertext = age_io::encrypt_bytes(SEED_CONTENT.as_bytes(), &opts.passphrase)?;
    std::fs::write(&seed_path, &ciphertext)?;

    commit_all(&root, "Initial commit — welcome note")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_with(passphrase: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        init_space(InitOptions {
            space_dir: dir.path().to_path_buf(),
            passphrase: SecretString::from(passphrase.to_string()),
            owner: "test@home.lan".into(),
        })
        .expect("init succeeded");
        dir
    }

    #[test]
    fn creates_space_toml() {
        let dir = init_with("p");
        let cfg = SpaceConfig::load(dir.path()).unwrap();
        assert_eq!(cfg.owner, "test@home.lan");
        assert!(!cfg.salt_verify_hex.is_empty());
        assert!(!cfg.verifier_hash_hex.is_empty());
        assert!(cfg.passkey.is_none());
    }

    #[test]
    fn creates_seed_note_as_encrypted_blob() {
        let dir = init_with("p");
        let seed = dir.path().join("space").join("Journal/2026/welcome.md.age");
        assert!(seed.is_file(), "seed note exists");
        let bytes = std::fs::read(&seed).unwrap();
        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
    }

    #[test]
    fn produces_a_git_repository_with_one_root_commit() {
        let dir = init_with("p");
        let repo = git2::Repository::open(dir.path().join("space")).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(
            head.message().unwrap().trim(),
            "Initial commit — welcome note"
        );
        assert_eq!(head.parent_count(), 0);
    }

    #[test]
    fn rejects_double_init() {
        let dir = init_with("p");
        let err = init_space(InitOptions {
            space_dir: dir.path().to_path_buf(),
            passphrase: SecretString::from("p".to_string()),
            owner: "x".into(),
        })
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn seed_decrypts_with_the_chosen_passphrase() {
        let dir = init_with("right one");
        let seed = dir.path().join("space").join("Journal/2026/welcome.md.age");
        let bytes = std::fs::read(&seed).unwrap();
        let pass = SecretString::from("right one".to_string());
        let pt = crate::crypto::age_io::decrypt_bytes(&bytes, &pass).unwrap();
        assert!(String::from_utf8(pt)
            .unwrap()
            .starts_with("# Welcome to your space"));
    }
}
