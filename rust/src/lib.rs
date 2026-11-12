//! Ace ID CLI v2 library.
//!
//! The binary in `src/main.rs` is a thin wrapper around [`cli::run`]. Keeping the
//! implementation in a library lets the integration tests exercise real command
//! flows against a local mock server.

pub mod api;
pub mod auth;
pub mod cli;
pub mod config;
pub mod error;
pub mod http;
pub mod project;
pub mod sdk;
pub mod terminal;
pub mod util;
