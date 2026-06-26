//! Integration-test entrypoint. Cargo compiles each `tests/*.rs` as its own
//! binary, so a helper used by only some files would surface as `unused`
//! per-binary. Folding every submodule under one binary keeps cargo's
//! unused-warning honest — no `#[allow(dead_code)]` shortcuts. The explicit
//! `#[path]` keeps the submodules tidy under `tests/integration/`.

#[path = "integration/agent.rs"]
mod agent;
#[path = "integration/auth.rs"]
mod auth;
#[path = "integration/common/mod.rs"]
mod common;
#[path = "integration/files.rs"]
mod files;
#[path = "integration/health.rs"]
mod health;
#[path = "integration/security.rs"]
mod security;
