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
    };
    cfg.save(space_dir)?;

    git2::Repository::init(&root)
        .map_err(|e| AppError::Internal(format!("git init: {e}")))?;

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
