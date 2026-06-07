//! Quest + progression protocol.

use serde::{Deserialize, Serialize};

/// Lifecycle state of a quest for a given character.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestState {
    /// Offered but not yet accepted.
    Available,
    /// Accepted, objectives in progress.
    Active,
    /// All objectives met, ready to turn in.
    Complete,
    /// Finished and rewarded.
    TurnedIn,
}

// ---------------------------------------------------------------------------
// Client → server
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AcceptQuestMessage {
    pub quest_id: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AbandonQuestMessage {
    pub quest_id: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TurnInQuestMessage {
    pub quest_id: u32,
}

// ---------------------------------------------------------------------------
// Server → client
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QuestObjectiveInfo {
    pub text: String,
    pub count: u32,
    pub required: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QuestInfo {
    pub quest_id: u32,
    pub name: String,
    pub description: String,
    pub objectives: Vec<QuestObjectiveInfo>,
    pub state: QuestState,
}

/// Full quest log (available + active + complete) for the receiving client.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QuestLogMessage {
    pub quests: Vec<QuestInfo>,
}

/// Server → client: level / xp HUD update.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProgressionMessage {
    pub level: u32,
    pub xp: u64,
    /// Total xp required to reach the next level from level 0.
    pub xp_to_next: u64,
}
