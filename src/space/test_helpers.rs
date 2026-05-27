//! Test fixtures shared across the `space::*` test modules.
//!
//! Compiled only under `#[cfg(test)]`. Uses cheap KDF parameters
//! (`log_n=4`) so test runs don't pay the production 150ms scrypt cost.

use age::secrecy::SecretString;
use rand::RngCore;
use tempfile::TempDir;

use crate::config::SpaceConfig;
use crate::crypto::kdf;
use crate::space::Space;

/// Cheap scrypt params for tests — produces correct verifier hashes but
/// runs in <1ms.
const TEST_LOG_N: u8 = 4;
const TEST_R: u32 = 8;
const TEST_P: u32 = 1;

pub fn make_space(passphrase: &str) -> (TempDir, Space, SecretString) {
    let dir = TempDir::new().expect("tempdir");
    let space_root = SpaceConfig::space_root(dir.path());
    std::fs::create_dir_all(&space_root).expect("mkdir space root");

    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let verifier =
        kdf::derive_verifier(passphrase, &salt, TEST_LOG_N, TEST_R, TEST_P).expect("kdf");

    let cfg = SpaceConfig {
        owner: "test@home.lan".into(),
        salt_verify_hex: hex::encode(salt),
        verifier_hash_hex: hex::encode(verifier),
        kdf_log_n: TEST_LOG_N,
        kdf_r: TEST_R,
        kdf_p: TEST_P,
        passkey: None,
    };
    cfg.save(dir.path()).expect("save config");

    git2::Repository::init(&space_root).expect("git init");

    let space = Space::open(dir.path().to_path_buf()).expect("space open");
    let pass = SecretString::from(passphrase.to_string());
    (dir, space, pass)
}

/// Count the total commits reachable from HEAD.
pub fn count_commits(repo_path: &std::path::Path) -> usize {
    let repo = git2::Repository::open(repo_path).unwrap();
    let mut walk = repo.revwalk().unwrap();
    if walk.push_head().is_err() {
        return 0;
    }
    walk.filter_map(Result::ok).count()
}

/// Convenience: build a space and pre-populate a single encrypted note.
pub fn make_space_with_note(
    passphrase: &str,
    rel_path: &str,
    content: &str,
) -> (TempDir, Space, SecretString) {
    let (dir, space, pass) = make_space(passphrase);
    let result =
        crate::space::write::write_file(&space, &pass, rel_path, content, None).expect("seed note");
    assert_eq!(result.path, rel_path);
    (dir, space, pass)
}
