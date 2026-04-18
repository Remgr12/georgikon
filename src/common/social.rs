use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChatChannel {
    Local,
    Party,
    Guild,
    Trade,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChatNetMessage {
    pub channel: ChatChannel,
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GroupInviteMessage {
    pub from_player_id: u64,
    pub to_player_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupInviteResponseKind {
    Accept,
    Decline,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GroupInviteResponseMessage {
    pub from_player_id: u64,
    pub to_player_id: u64,
    pub response: GroupInviteResponseKind,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TradeRequestMessage {
    pub from_player_id: u64,
    pub to_player_id: u64,
}
