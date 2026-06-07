//! Party protocol: a lightweight, runtime-only (non-persisted) group of up to
//! [`MAX_PARTY_SIZE`] players that share the Party chat channel and party HUD.

use serde::{Deserialize, Serialize};

/// Maximum number of members in a single party (including the leader).
pub const MAX_PARTY_SIZE: usize = 5;

// ---------------------------------------------------------------------------
// Client → server operations
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PartyInviteMessage {
    pub target_name: String,
}

/// Response to a pending party invite.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PartyInviteResponseMessage {
    pub accept: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LeavePartyMessage;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KickPartyMemberMessage {
    pub target_char_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PromotePartyLeaderMessage {
    pub target_char_id: u64,
}

// ---------------------------------------------------------------------------
// Server → client state
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PartyMemberInfo {
    pub char_id: u64,
    pub name: String,
    pub level: u32,
    pub health: f32,
    pub max_health: f32,
}

/// Full party state for the receiving client. `in_party == false` means the
/// player is solo (the HUD hides itself).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PartyStateMessage {
    pub in_party: bool,
    pub leader_char_id: u64,
    pub members: Vec<PartyMemberInfo>,
}

/// Server → client: you have been invited to a party.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PartyInvitePushMessage {
    pub from_char_id: u64,
    pub from_name: String,
}
