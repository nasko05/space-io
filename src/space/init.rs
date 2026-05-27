use std::path::{Path, PathBuf};

use age::secrecy::SecretString;
use rand::RngCore;

use crate::config::SpaceConfig;
use crate::crypto::{age_io, kdf};
use crate::error::{AppError, AppResult};

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

    // 1. Derive verifier hash.
    let mut salt_verify = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt_verify);
    let verifier = kdf::derive_verifier(
        age::secrecy::ExposeSecret::expose_secret(&opts.passphrase),
        &salt_verify,
        kdf::DEFAULT_LOG_N,
        kdf::DEFAULT_R,
        kdf::DEFAULT_P,
    )?;

    // 2. Write config.
    let cfg = SpaceConfig {
        owner: opts.owner,
        salt_verify_hex: hex::encode(salt_verify),
        verifier_hash_hex: hex::encode(verifier),
        kdf_log_n: kdf::DEFAULT_LOG_N,
        kdf_r: kdf::DEFAULT_R,
        kdf_p: kdf::DEFAULT_P,
    };
    cfg.save(space_dir)?;

    // 3. git init the space root.
    let _repo = git2::Repository::init(&root)
        .map_err(|e| AppError::Internal(format!("git init: {e}")))?;

    // 4. Seed note.
    let seed_full_rel = format!("{SEED_REL_PATH}.age");
    let seed_path = root.join(&seed_full_rel);
    if let Some(parent) = seed_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let ciphertext = age_io::encrypt_bytes(SEED_CONTENT.as_bytes(), &opts.passphrase)?;
    std::fs::write(&seed_path, &ciphertext)?;

    // 5. First commit.
    commit_all(&root, "Initial commit — welcome note")?;

    Ok(())
}

fn commit_all(repo_path: &Path, message: &str) -> AppResult<()> {
    let repo = git2::Repository::open(repo_path)
        .map_err(|e| AppError::Internal(format!("git open: {e}")))?;
    let mut index = repo
        .index()
        .map_err(|e| AppError::Internal(format!("git index: {e}")))?;
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .map_err(|e| AppError::Internal(format!("git add: {e}")))?;
    index
        .write()
        .map_err(|e| AppError::Internal(format!("git index write: {e}")))?;
    let tree_oid = index
        .write_tree()
        .map_err(|e| AppError::Internal(format!("git write_tree: {e}")))?;
    let tree = repo
        .find_tree(tree_oid)
        .map_err(|e| AppError::Internal(format!("git find_tree: {e}")))?;
    let sig = git2::Signature::now("hearth", "hearth@local")
        .map_err(|e| AppError::Internal(format!("git signature: {e}")))?;
    let parents: Vec<git2::Commit> = match repo.head() {
        Ok(head) => head
            .peel_to_commit()
            .ok()
            .map(|c| vec![c])
            .unwrap_or_default(),
        Err(_) => vec![],
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
        .map_err(|e| AppError::Internal(format!("git commit: {e}")))?;
    Ok(())
}
