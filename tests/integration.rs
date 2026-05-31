//! Integration test entrypoint. Cargo compiles each `tests/*.rs` file as
//! its own binary, which makes a helper used by some test files but not
//! others surface as `unused` per-binary. Folding every integration test
//! under one binary lets the helper surface be exercised collectively, so
//! the only `unused` warnings cargo can issue are honest ones — no
//! `#[allow(dead_code)]` shortcuts.
//!
//! Submodules live next to this file in `tests/integration/`. We pin the
//! `#[path]` explicitly because rustc otherwise looks at the test binary's
//! crate-root directory (`tests/`), and we want to keep the layout tidy.

#[path = "integration/auth.rs"]
mod auth;
#[path = "integration/common/mod.rs"]
mod common;
#[path = "integration/files.rs"]
mod files;
#[path = "integration/security.rs"]
mod security;
