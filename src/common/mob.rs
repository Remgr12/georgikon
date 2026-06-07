//! Replicated visual state for non-player units (mobs) and remote players.
//!
//! Player/mob health is normally authoritative and only sent to the *owning*
//! client via `CombatStateMessage`. To draw nameplates and health bars above
//! *other* units, we replicate a compact [`UnitVisual`] to everyone in the zone.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// What kind of unit this is (drives nameplate color / loot behaviour).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Player,
    Wolf,
    Boar,
}

impl UnitKind {
    pub fn display_name(self) -> &'static str {
        match self {
            UnitKind::Player => "Player",
            UnitKind::Wolf => "Wolf",
            UnitKind::Boar => "Boar",
        }
    }
}

/// Replicated nameplate + health bar data for a unit (mob or remote player).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UnitVisual {
    pub kind: UnitKind,
    pub name: String,
    pub level: u32,
    pub health: f32,
    pub max_health: f32,
}
