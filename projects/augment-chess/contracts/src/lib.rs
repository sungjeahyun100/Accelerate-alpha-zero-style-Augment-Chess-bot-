//! Versioned game-specific contract resources packaged with the Rust engine.
//!
//! Runtime readers use the adjacent catalog files directly so there is one
//! canonical copy of each reviewed source-derived definition.

#![forbid(unsafe_code)]

pub const CATALOG_DIRECTORY: &str = "catalog";
