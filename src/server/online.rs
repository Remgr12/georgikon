//! Server-side index of currently-connected, authenticated players.
//!
//! Social systems (chat, guild, party, mail) need to translate a character id
//! or name into the lightyear connection entity to deliver a targeted message,
//! and into the game entity to read live state. This resource keeps both maps.

use bevy::prelude::*;
use std::collections::HashMap;

/// Fired (server-side) when an authenticated player disconnects, carrying their
/// character id. Subsystems (party, etc.) observe this instead of `Disconnected`
/// so they don't race the `OnlinePlayers` cleanup.
#[derive(Event)]
pub struct PlayerLeft(pub u64);

/// One online player's links.
#[derive(Clone, Copy, Debug)]
pub struct OnlineEntry {
    /// The `ClientOf` connection entity (a message *sender* target).
    pub conn: Entity,
    /// The authoritative game entity.
    pub game: Entity,
    pub account_id: i64,
}

#[derive(Resource, Default)]
pub struct OnlinePlayers {
    by_char: HashMap<u64, OnlineEntry>,
    name_by_char: HashMap<u64, String>,
    char_by_conn: HashMap<Entity, u64>,
}

impl OnlinePlayers {
    pub fn insert(&mut self, char_id: u64, name: String, entry: OnlineEntry) {
        self.by_char.insert(char_id, entry);
        self.name_by_char.insert(char_id, name);
        self.char_by_conn.insert(entry.conn, char_id);
    }

    pub fn remove_by_conn(&mut self, conn: Entity) -> Option<u64> {
        let char_id = self.char_by_conn.remove(&conn)?;
        self.by_char.remove(&char_id);
        self.name_by_char.remove(&char_id);
        Some(char_id)
    }

    pub fn entry(&self, char_id: u64) -> Option<OnlineEntry> {
        self.by_char.get(&char_id).copied()
    }

    pub fn conn_of(&self, char_id: u64) -> Option<Entity> {
        self.by_char.get(&char_id).map(|e| e.conn)
    }

    pub fn game_of(&self, char_id: u64) -> Option<Entity> {
        self.by_char.get(&char_id).map(|e| e.game)
    }

    pub fn char_of_conn(&self, conn: Entity) -> Option<u64> {
        self.char_by_conn.get(&conn).copied()
    }

    pub fn name_of(&self, char_id: u64) -> Option<&str> {
        self.name_by_char.get(&char_id).map(|s| s.as_str())
    }

    pub fn char_by_name(&self, name: &str) -> Option<u64> {
        self.name_by_char
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(id, _)| *id)
    }

    pub fn is_online(&self, char_id: u64) -> bool {
        self.by_char.contains_key(&char_id)
    }

    /// Iterate over (char_id, entry) for all online players.
    pub fn iter(&self) -> impl Iterator<Item = (u64, OnlineEntry)> + '_ {
        self.by_char.iter().map(|(id, e)| (*id, *e))
    }
}
