//! Library facade that lets integration tests (under `tests/`) reach the
//! same modules `main.rs` consumes. Re-exports the bits the binary already
//! wires up; the binary stays the single point of truth for argv parsing
//! and the tokio runtime.

pub mod config;
pub mod crypto;
pub mod error;
pub mod routes;
pub mod space;
pub mod state;
