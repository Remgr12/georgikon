//! Guild-island build protocol + zone travel.
//!
//! Island props are replicated to clients via the [`IslandObjectInfo`] component
//! (registered in `SharedPlugin`) together with the shared `PlayerPosition`
//! component for their world position. The client looks the prefab up in the
//! catalog ([`PrefabCatalogMessage`]) and spawns a matching mesh.

use crate::common::zone::ZoneId;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Maximum number of placed objects allowed on a single guild island.
pub const MAX_ISLAND_OBJECTS: usize = 500;

/// Replicated descriptor of a placed island object. Position comes from the
/// shared `PlayerPosition` component on the same entity.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IslandObjectInfo {
    /// Stable DB id (used by move/remove ops).
    pub object_id: u64,
    pub prefab_id: u32,
    pub rot_y: f32,
    pub scale: f32,
}

// ---------------------------------------------------------------------------
// Client → server build operations
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlaceObjectMessage {
    pub guild_id: u64,
    pub prefab_id: u32,
    pub pos: [f32; 3],
    pub rot_y: f32,
    pub scale: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MoveObjectMessage {
    pub object_id: u64,
    pub pos: [f32; 3],
    pub rot_y: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RemoveObjectMessage {
    pub object_id: u64,
}

// ---------------------------------------------------------------------------
// Travel
// ---------------------------------------------------------------------------

/// Where a player wants to travel.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TravelTarget {
    Overworld,
    /// Travel to the island of the given guild id (must be a member).
    GuildIsland(u64),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TravelRequestMessage {
    pub target: TravelTarget,
}

/// Server → client: your current zone changed (so the client swaps terrain).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ZoneChangedMessage {
    pub zone: ZoneId,
}

// ---------------------------------------------------------------------------
// Prefab catalog (server → client)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PrefabInfo {
    pub id: u32,
    pub name: String,
    pub color: [f32; 3],
    pub size: [f32; 3],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PrefabCatalogMessage {
    pub prefabs: Vec<PrefabInfo>,
}
