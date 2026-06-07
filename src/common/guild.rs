//! Guild protocol: ranks, settings, the client→server operation messages, and
//! the server→client state/browse/invite messages.
//!
//! Headline rule: a character may belong to up to [`MAX_GUILDS_PER_CHAR`] guilds
//! at once, but a guild flagged `exclusive` forbids its members from being in
//! any other guild. Enforcement lives server-side (`server::guild`).

use serde::{Deserialize, Serialize};

/// A character may be in at most this many guilds simultaneously.
pub const MAX_GUILDS_PER_CHAR: usize = 2;

/// Maximum guild name length.
pub const MAX_GUILD_NAME_LEN: usize = 28;

/// Rank within a guild; governs which operations a member may perform.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuildRank {
    Leader,
    Officer,
    Member,
}

impl GuildRank {
    pub fn as_str(self) -> &'static str {
        match self {
            GuildRank::Leader => "Leader",
            GuildRank::Officer => "Officer",
            GuildRank::Member => "Member",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "Leader" => GuildRank::Leader,
            "Officer" => GuildRank::Officer,
            _ => GuildRank::Member,
        }
    }

    /// Can invite / kick members and edit the island.
    pub fn can_manage_members(self) -> bool {
        matches!(self, GuildRank::Leader | GuildRank::Officer)
    }

    /// Can edit guild settings, motd, ranks, and disband.
    pub fn is_leader(self) -> bool {
        matches!(self, GuildRank::Leader)
    }

    /// Can place/move/remove objects on the guild island.
    pub fn can_build(self) -> bool {
        // Everyone in the guild can build; restrict to officers+ by changing this.
        true
    }
}

/// How new members may join a guild.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinPolicy {
    /// Anyone may join instantly.
    Open,
    /// Join only via an officer's invite.
    InviteOnly,
    /// No new members.
    Closed,
}

impl JoinPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            JoinPolicy::Open => "open",
            JoinPolicy::InviteOnly => "invite",
            JoinPolicy::Closed => "closed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "open" => JoinPolicy::Open,
            "closed" => JoinPolicy::Closed,
            _ => JoinPolicy::InviteOnly,
        }
    }
}

// ---------------------------------------------------------------------------
// Client → server operations
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CreateGuildMessage {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DisbandGuildMessage {
    pub guild_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildInviteRequestMessage {
    pub guild_id: u64,
    pub target_name: String,
}

/// Client's response to a pending guild invite (accept / decline).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildInviteResponseMessage {
    pub guild_id: u64,
    pub accept: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LeaveGuildMessage {
    pub guild_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KickGuildMemberMessage {
    pub guild_id: u64,
    pub target_char_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetGuildRankMessage {
    pub guild_id: u64,
    pub target_char_id: u64,
    pub rank: GuildRank,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetGuildSettingsMessage {
    pub guild_id: u64,
    pub exclusive: bool,
    pub join_policy: JoinPolicy,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetGuildMotdMessage {
    pub guild_id: u64,
    pub motd: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RequestGuildListMessage;

// ---------------------------------------------------------------------------
// Server → client state
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildMemberInfo {
    pub char_id: u64,
    pub name: String,
    pub rank: GuildRank,
    pub online: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildInfo {
    pub id: u64,
    pub name: String,
    pub motd: String,
    pub exclusive: bool,
    pub join_policy: JoinPolicy,
    pub members: Vec<GuildMemberInfo>,
    /// The receiving player's own rank in this guild.
    pub my_rank: GuildRank,
}

/// Full state of the caller's guild memberships (0..=`MAX_GUILDS_PER_CHAR`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildStateMessage {
    pub guilds: Vec<GuildInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildBrowseEntry {
    pub id: u64,
    pub name: String,
    pub member_count: u32,
    pub join_policy: JoinPolicy,
    pub exclusive: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildListMessage {
    pub guilds: Vec<GuildBrowseEntry>,
}

/// Server → client: you have been invited to a guild.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildInvitePushMessage {
    pub guild_id: u64,
    pub guild_name: String,
    pub from_name: String,
}

/// Server → client: generic ack / error feedback for a guild operation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildActionResultMessage {
    pub ok: bool,
    pub message: String,
}
