//! Zones split the world into separately-replicated instances.
//!
//! `ZoneId::Overworld` is the single shared world; each guild owns a private
//! `ZoneId::GuildIsland(guild_id)` instance. Server-side, every gameplay entity
//! carries a [`Zone`] component and is placed in a lightyear [`Room`] matching
//! its zone, so a client only ever receives the entities in the zone it is in.
//!
//! [`Room`]: lightyear::prelude::Room

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Identifies which world instance an entity lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ZoneId {
    /// The single shared overworld.
    Overworld,
    /// A guild's private island instance, keyed by guild id.
    GuildIsland(u64),
}

impl ZoneId {
    /// Reconstruct from the two-column DB representation (kind, guild_id).
    pub fn from_db(kind: &str, guild_id: Option<i64>) -> Self {
        match kind {
            "island" => ZoneId::GuildIsland(guild_id.unwrap_or(0) as u64),
            _ => ZoneId::Overworld,
        }
    }

    /// Lower to the two-column DB representation (kind, guild_id).
    pub fn to_db(self) -> (&'static str, Option<i64>) {
        match self {
            ZoneId::Overworld => ("overworld", None),
            ZoneId::GuildIsland(g) => ("island", Some(g as i64)),
        }
    }
}

/// Component placed on every gameplay entity (players, mobs, island props)
/// recording which zone it currently belongs to. Replicated so the owning
/// client knows which terrain to render.
#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Zone(pub ZoneId);

impl Default for Zone {
    fn default() -> Self {
        Zone(ZoneId::Overworld)
    }
}
