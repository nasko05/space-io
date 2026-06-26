//! Library facade exposing the modules the binary wires up, so integration
//! tests under `tests/` can reach them. `main.rs` owns argv parsing and the
//! tokio runtime.

pub mod agent;
pub mod config;
pub mod crypto;
pub mod error;
pub mod fs_atomic;
pub mod routes;
pub mod space;
pub mod sso;
pub mod state;
