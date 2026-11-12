//! HTTP API layer for the Ace ID service.

pub mod client;
pub mod errors;
pub mod url;

pub use client::{api_fetch, api_get_json, api_json};
