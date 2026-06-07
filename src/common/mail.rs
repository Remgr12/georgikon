//! Asynchronous mail protocol — the "messaging system" that complements live chat.

use serde::{Deserialize, Serialize};

pub const MAX_MAIL_SUBJECT_LEN: usize = 60;
pub const MAX_MAIL_BODY_LEN: usize = 1000;

// ---------------------------------------------------------------------------
// Client → server
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SendMailMessage {
    pub to_name: String,
    pub subject: String,
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RequestMailMessage;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MarkMailReadMessage {
    pub mail_id: u64,
}

// ---------------------------------------------------------------------------
// Server → client
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MailEntry {
    pub id: u64,
    pub from_name: String,
    pub subject: String,
    pub body: String,
    pub read: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MailListMessage {
    pub entries: Vec<MailEntry>,
}

/// Server → client: generic ack/feedback for a mail operation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MailActionResultMessage {
    pub ok: bool,
    pub message: String,
}
