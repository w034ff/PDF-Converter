//! Passwords for encrypted test fixtures (design §11.1).
//!
//! These protect only fixture files in the repository and carry no secrets.

#![allow(
    dead_code,
    reason = "constants are shared across test files where each file uses a subset"
)]

/// User password for `encrypted.pdf`.
pub const ENCRYPTED_USER_PASSWORD: &str = "fixture-user";

/// Owner password for `encrypted.pdf` and `encrypted_cafe.pdf`.
pub const OWNER_PASSWORD: &str = "fixture-owner";

/// User password for `encrypted_cafe.pdf`.
pub const ENCRYPTED_CAFE_USER_PASSWORD: &str = "café";
