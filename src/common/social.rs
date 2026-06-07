//! Chat protocol. Routing (who actually receives a message) is resolved
//! server-side in `server::chat` from the sender's zone / party / guild.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChatChannel {
    /// Proximity chat: everyone near the sender in the same zone.
    Local,
    /// The sender's party.
    Party,
    /// One of the sender's guilds (selected by `guild_id`).
    Guild,
    /// Global trade channel.
    Trade,
    /// Global world chat.
    World,
    /// Private message to a single named player.
    Whisper,
}

impl ChatChannel {
    pub fn tag(self) -> &'static str {
        match self {
            ChatChannel::Local => "local",
            ChatChannel::Party => "party",
            ChatChannel::Guild => "guild",
            ChatChannel::Trade => "trade",
            ChatChannel::World => "world",
            ChatChannel::Whisper => "whisper",
        }
    }
}

/// Client → server: a chat send request (pre-validation).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChatNetMessage {
    pub channel: ChatChannel,
    pub body: String,
    /// Recipient name for `Whisper` (empty otherwise).
    pub target_name: String,
    /// Which guild to post to for `Guild` (0 = the sender's first guild).
    pub guild_id: u64,
}

/// Server → client: a validated, routed, normalized chat line ready to display.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChatBroadcastMessage {
    /// Display name of the sender (server-resolved).
    pub sender_name: String,
    pub channel: ChatChannel,
    /// Normalized body (trimmed, length-capped by server).
    pub body: String,
    /// For `Whisper`: the recipient's display name (so the sender's own client
    /// can show "to <name>").
    pub target_name: String,
}
