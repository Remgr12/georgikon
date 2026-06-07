//! Account / login protocol messages (client ↔ server).
//!
//! The handshake is: the lightyear netcode connection comes up first, then the
//! client must send a [`LoginRequestMessage`] (or [`RegisterRequestMessage`])
//! before the server will spawn its character. On success the server replies
//! with [`LoginResultMessage`] and spawns the replicated game entity.

use serde::{Deserialize, Serialize};

/// Maximum accepted username / character-name length.
pub const MAX_NAME_LEN: usize = 24;

/// Client → server: create a new account, then log in.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RegisterRequestMessage {
    pub username: String,
    pub password: String,
}

/// Client → server: authenticate an existing account.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LoginRequestMessage {
    pub username: String,
    pub password: String,
}

/// Server → client: result of a register/login attempt.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LoginResultMessage {
    pub ok: bool,
    /// Human-readable failure reason (empty on success).
    pub reason: String,
    /// Persistent character id (0 on failure).
    pub character_id: u64,
    /// Character display name (empty on failure).
    pub name: String,
}
