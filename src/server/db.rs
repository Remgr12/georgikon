//! Thin SQLite helpers. All game content (items, spells, prefabs, quests) and
//! persistent player/guild state live in `game.db` next to the binary. Tables
//! are created and seeded on first run.
//!
//! This module is intentionally a thin data layer: it returns primitives / small
//! row structs and leaves interpretation to the gameplay systems.

use bevy::prelude::KeyCode;
use rusqlite::{params, Connection, OptionalExtension, Result};

const DB_PATH: &str = "game.db";

pub fn open() -> Result<Connection> {
    let conn = Connection::open(DB_PATH)?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;",
    )?;
    Ok(conn)
}

/// Create tables and insert default rows if the tables are empty.
pub fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS items (
            id       INTEGER PRIMARY KEY,
            name     TEXT    NOT NULL,
            color_r  REAL    NOT NULL,
            color_g  REAL    NOT NULL,
            color_b  REAL    NOT NULL
        );
        CREATE TABLE IF NOT EXISTS spells (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            name          TEXT   NOT NULL,
            key_code      TEXT   NOT NULL,
            cooldown_secs REAL   NOT NULL,
            color_r       REAL   NOT NULL,
            color_g       REAL   NOT NULL,
            color_b       REAL   NOT NULL
        );
        CREATE TABLE IF NOT EXISTS prefabs (
            id       INTEGER PRIMARY KEY,
            name     TEXT    NOT NULL,
            color_r  REAL    NOT NULL,
            color_g  REAL    NOT NULL,
            color_b  REAL    NOT NULL,
            size_x   REAL    NOT NULL,
            size_y   REAL    NOT NULL,
            size_z   REAL    NOT NULL
        );

        -- accounts & characters -------------------------------------------------
        CREATE TABLE IF NOT EXISTS accounts (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            username   TEXT    UNIQUE NOT NULL,
            pw_hash    TEXT    NOT NULL,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );
        CREATE TABLE IF NOT EXISTS characters (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id    INTEGER NOT NULL,
            name          TEXT    NOT NULL,
            level         INTEGER NOT NULL DEFAULT 1,
            xp            INTEGER NOT NULL DEFAULT 0,
            zone_kind     TEXT    NOT NULL DEFAULT 'overworld',
            zone_guild_id INTEGER,
            x             REAL    NOT NULL DEFAULT 0.0,
            y             REAL    NOT NULL DEFAULT 1.0,
            z             REAL    NOT NULL DEFAULT 0.0
        );
        CREATE TABLE IF NOT EXISTS char_inventory (
            character_id INTEGER NOT NULL,
            slot         INTEGER NOT NULL,
            item_id      INTEGER NOT NULL,
            qty          INTEGER NOT NULL,
            PRIMARY KEY (character_id, slot)
        );

        -- guilds ----------------------------------------------------------------
        CREATE TABLE IF NOT EXISTS guilds (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            name           TEXT    UNIQUE NOT NULL,
            leader_char_id INTEGER NOT NULL,
            motd           TEXT    NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS guild_settings (
            guild_id    INTEGER PRIMARY KEY,
            exclusive   INTEGER NOT NULL DEFAULT 0,
            join_policy TEXT    NOT NULL DEFAULT 'invite'
        );
        CREATE TABLE IF NOT EXISTS guild_members (
            guild_id     INTEGER NOT NULL,
            character_id INTEGER NOT NULL,
            rank         TEXT    NOT NULL,
            PRIMARY KEY (guild_id, character_id)
        );

        -- islands ---------------------------------------------------------------
        CREATE TABLE IF NOT EXISTS island_objects (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            guild_id  INTEGER NOT NULL,
            prefab_id INTEGER NOT NULL,
            x         REAL    NOT NULL,
            y         REAL    NOT NULL,
            z         REAL    NOT NULL,
            rot_y     REAL    NOT NULL DEFAULT 0.0,
            scale     REAL    NOT NULL DEFAULT 1.0
        );

        -- quests ----------------------------------------------------------------
        CREATE TABLE IF NOT EXISTS quests (
            id          INTEGER PRIMARY KEY,
            name        TEXT    NOT NULL,
            description TEXT    NOT NULL,
            reward_item INTEGER NOT NULL DEFAULT 0,
            reward_qty  INTEGER NOT NULL DEFAULT 0,
            reward_xp   INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS quest_objectives (
            quest_id  INTEGER NOT NULL,
            idx       INTEGER NOT NULL,
            kind      TEXT    NOT NULL,
            target_id INTEGER NOT NULL DEFAULT 0,
            required  INTEGER NOT NULL DEFAULT 1,
            text      TEXT    NOT NULL DEFAULT '',
            PRIMARY KEY (quest_id, idx)
        );
        CREATE TABLE IF NOT EXISTS quest_progress (
            character_id INTEGER NOT NULL,
            quest_id     INTEGER NOT NULL,
            objective_idx INTEGER NOT NULL,
            count        INTEGER NOT NULL DEFAULT 0,
            state        TEXT    NOT NULL DEFAULT 'active',
            PRIMARY KEY (character_id, quest_id, objective_idx)
        );

        -- social ----------------------------------------------------------------
        CREATE TABLE IF NOT EXISTS friends (
            character_id   INTEGER NOT NULL,
            friend_char_id INTEGER NOT NULL,
            PRIMARY KEY (character_id, friend_char_id)
        );
        CREATE TABLE IF NOT EXISTS mail (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            to_char_id INTEGER NOT NULL,
            from_name  TEXT    NOT NULL,
            subject    TEXT    NOT NULL,
            body       TEXT    NOT NULL,
            read       INTEGER NOT NULL DEFAULT 0,
            sent_at    INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );",
    )?;

    seed(conn)
}

/// Insert default content rows when their tables are empty.
fn seed(conn: &Connection) -> Result<()> {
    // INSERT OR IGNORE so we can add new items even to existing databases.
    conn.execute_batch(
        "INSERT OR IGNORE INTO items VALUES (1, 'Iron Sword',    0.72, 0.72, 0.82);
         INSERT OR IGNORE INTO items VALUES (2, 'Health Potion', 0.90, 0.18, 0.18);
         INSERT OR IGNORE INTO items VALUES (3, 'Mana Potion',   0.20, 0.30, 0.95);
         INSERT OR IGNORE INTO items VALUES (4, 'Gold Coin',     0.95, 0.82, 0.10);
         INSERT OR IGNORE INTO items VALUES (5, 'Magic Staff',   0.62, 0.18, 0.88);
         INSERT OR IGNORE INTO items VALUES (6, 'Wolf Pelt',     0.55, 0.40, 0.28);
         INSERT OR IGNORE INTO items VALUES (7, 'Boar Tusk',     0.92, 0.90, 0.80);
         INSERT OR IGNORE INTO items VALUES (8, 'Silver Coin',   0.80, 0.80, 0.80);
         INSERT OR IGNORE INTO items VALUES (9, 'Healing Herb',  0.18, 0.70, 0.18);",
    )?;

    if count(conn, "spells")? == 0 {
        conn.execute_batch(
            "INSERT INTO spells (name, key_code, cooldown_secs, color_r, color_g, color_b)
             VALUES ('Fireball',  'F1',  5.0, 1.00, 0.38, 0.08);
             INSERT INTO spells (name, key_code, cooldown_secs, color_r, color_g, color_b)
             VALUES ('Frost Nova','F2', 12.0, 0.38, 0.68, 1.00);
             INSERT INTO spells (name, key_code, cooldown_secs, color_r, color_g, color_b)
             VALUES ('Heal',      'F3',  3.0, 0.18, 0.88, 0.30);
             INSERT INTO spells (name, key_code, cooldown_secs, color_r, color_g, color_b)
             VALUES ('Shield',    'F4', 20.0, 0.85, 0.85, 0.18);",
        )?;
    }

    if count(conn, "prefabs")? == 0 {
        // id, name, color rgb, size xyz
        conn.execute_batch(
            "INSERT INTO prefabs VALUES (1, 'Stone Wall',   0.55, 0.55, 0.58, 2.0, 3.0, 0.4);
             INSERT INTO prefabs VALUES (2, 'Wood Floor',   0.55, 0.40, 0.24, 4.0, 0.2, 4.0);
             INSERT INTO prefabs VALUES (3, 'Pillar',       0.70, 0.70, 0.72, 0.6, 4.0, 0.6);
             INSERT INTO prefabs VALUES (4, 'Crystal',      0.40, 0.85, 0.95, 1.0, 2.0, 1.0);
             INSERT INTO prefabs VALUES (5, 'Tree',         0.20, 0.55, 0.20, 1.5, 4.0, 1.5);
             INSERT INTO prefabs VALUES (6, 'Lantern',      0.95, 0.85, 0.40, 0.5, 1.2, 0.5);
             INSERT INTO prefabs VALUES (7, 'Portal Home',  0.65, 0.30, 0.95, 1.5, 3.0, 0.3);",
        )?;
    }

    // INSERT OR IGNORE so new quests appear even in existing databases.
    conn.execute_batch(
        "INSERT OR IGNORE INTO quests VALUES (1, 'Cull the Wolves',
            'The wolves north of town grow bold. Thin their numbers.',
            4, 50, 100);
         INSERT OR IGNORE INTO quests VALUES (2, 'Boar Hunt',
            'Gather boar tusks for the alchemist.',
            2, 3, 60);
         INSERT OR IGNORE INTO quests VALUES (3, 'The Pelt Trader',
            'Elder Savan needs wolf pelts for the coming winter.',
            8, 3, 80);
         INSERT OR IGNORE INTO quests VALUES (4, 'Gathering Herbs',
            'Lira the merchant needs healing herbs — enemies carry them.',
            9, 5, 120);",
    )?;
    conn.execute_batch(
        "INSERT OR IGNORE INTO quest_objectives VALUES (1, 0, 'kill',    1, 3, 'Slay 3 Wolves');
         INSERT OR IGNORE INTO quest_objectives VALUES (2, 0, 'collect', 7, 2, 'Collect 2 Boar Tusks');
         INSERT OR IGNORE INTO quest_objectives VALUES (3, 0, 'collect', 6, 3, 'Collect 3 Wolf Pelts');
         INSERT OR IGNORE INTO quest_objectives VALUES (4, 0, 'collect', 9, 5, 'Collect 5 Healing Herbs');",
    )?;

    Ok(())
}

fn count(conn: &Connection, table: &str) -> Result<i64> {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
}

// ---------------------------------------------------------------------------
// Content loaders (items / spells / prefabs)
// ---------------------------------------------------------------------------

/// Load all item rows as `(id, name, r, g, b)`.
pub fn load_items(conn: &Connection) -> Result<Vec<(u32, String, f32, f32, f32)>> {
    let mut stmt =
        conn.prepare("SELECT id, name, color_r, color_g, color_b FROM items ORDER BY id")?;
    stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)? as u32,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)? as f32,
            row.get::<_, f64>(3)? as f32,
            row.get::<_, f64>(4)? as f32,
        ))
    })?
    .collect()
}

/// Load all spell rows as `(name, key_code_str, cooldown_secs, r, g, b)`.
pub fn load_spells(conn: &Connection) -> Result<Vec<(String, String, f32, f32, f32, f32)>> {
    let mut stmt = conn.prepare(
        "SELECT name, key_code, cooldown_secs, color_r, color_g, color_b
         FROM spells ORDER BY id",
    )?;
    stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)? as f32,
            row.get::<_, f64>(3)? as f32,
            row.get::<_, f64>(4)? as f32,
            row.get::<_, f64>(5)? as f32,
        ))
    })?
    .collect()
}

/// A buildable prefab definition.
#[derive(Clone, Debug)]
pub struct PrefabRow {
    pub id: u32,
    pub name: String,
    pub color: [f32; 3],
    pub size: [f32; 3],
}

pub fn load_prefabs(conn: &Connection) -> Result<Vec<PrefabRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, color_r, color_g, color_b, size_x, size_y, size_z
         FROM prefabs ORDER BY id",
    )?;
    stmt.query_map([], |row| {
        Ok(PrefabRow {
            id: row.get::<_, i64>(0)? as u32,
            name: row.get::<_, String>(1)?,
            color: [
                row.get::<_, f64>(2)? as f32,
                row.get::<_, f64>(3)? as f32,
                row.get::<_, f64>(4)? as f32,
            ],
            size: [
                row.get::<_, f64>(5)? as f32,
                row.get::<_, f64>(6)? as f32,
                row.get::<_, f64>(7)? as f32,
            ],
        })
    })?
    .collect()
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

/// Create a new account row. Returns the new account id, or an error if the
/// username is already taken (UNIQUE constraint).
pub fn create_account(conn: &Connection, username: &str, pw_hash: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO accounts (username, pw_hash) VALUES (?1, ?2)",
        params![username, pw_hash],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Look up an account by username. Returns `(account_id, pw_hash)`.
pub fn find_account(conn: &Connection, username: &str) -> Result<Option<(i64, String)>> {
    conn.query_row(
        "SELECT id, pw_hash FROM accounts WHERE username = ?1",
        params![username],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
    )
    .optional()
}

// ---------------------------------------------------------------------------
// Characters
// ---------------------------------------------------------------------------

/// A persisted character snapshot.
#[derive(Clone, Debug)]
pub struct CharacterRow {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub level: i64,
    pub xp: i64,
    /// "overworld" or "island"
    pub zone_kind: String,
    /// Guild id when `zone_kind == "island"`.
    pub zone_guild_id: Option<i64>,
    pub pos: [f32; 3],
}

/// Fetch the first character for an account, creating one with `default_name`
/// if the account has none yet.
pub fn get_or_create_character(
    conn: &Connection,
    account_id: i64,
    default_name: &str,
) -> Result<CharacterRow> {
    if let Some(row) = load_character_for_account(conn, account_id)? {
        return Ok(row);
    }
    conn.execute(
        "INSERT INTO characters (account_id, name) VALUES (?1, ?2)",
        params![account_id, default_name],
    )?;
    let id = conn.last_insert_rowid();
    Ok(CharacterRow {
        id,
        account_id,
        name: default_name.to_string(),
        level: 1,
        xp: 0,
        zone_kind: "overworld".to_string(),
        zone_guild_id: None,
        pos: [0.0, 1.0, 0.0],
    })
}

fn load_character_for_account(
    conn: &Connection,
    account_id: i64,
) -> Result<Option<CharacterRow>> {
    conn.query_row(
        "SELECT id, account_id, name, level, xp, zone_kind, zone_guild_id, x, y, z
         FROM characters WHERE account_id = ?1 ORDER BY id LIMIT 1",
        params![account_id],
        row_to_character,
    )
    .optional()
}

pub fn load_character(conn: &Connection, char_id: i64) -> Result<Option<CharacterRow>> {
    conn.query_row(
        "SELECT id, account_id, name, level, xp, zone_kind, zone_guild_id, x, y, z
         FROM characters WHERE id = ?1",
        params![char_id],
        row_to_character,
    )
    .optional()
}

fn row_to_character(row: &rusqlite::Row) -> Result<CharacterRow> {
    Ok(CharacterRow {
        id: row.get(0)?,
        account_id: row.get(1)?,
        name: row.get(2)?,
        level: row.get(3)?,
        xp: row.get(4)?,
        zone_kind: row.get(5)?,
        zone_guild_id: row.get(6)?,
        pos: [
            row.get::<_, f64>(7)? as f32,
            row.get::<_, f64>(8)? as f32,
            row.get::<_, f64>(9)? as f32,
        ],
    })
}

/// Persist only the level and xp for a character (fast path for XP awards).
pub fn save_character_xp(conn: &Connection, char_id: i64, level: i64, xp: i64) -> Result<()> {
    conn.execute(
        "UPDATE characters SET level=?2, xp=?3 WHERE id=?1",
        params![char_id, level, xp],
    )?;
    Ok(())
}

/// Persist a character's mutable fields (position, level, xp, zone).
pub fn save_character(conn: &Connection, c: &CharacterRow) -> Result<()> {
    conn.execute(
        "UPDATE characters
         SET name=?2, level=?3, xp=?4, zone_kind=?5, zone_guild_id=?6, x=?7, y=?8, z=?9
         WHERE id=?1",
        params![
            c.id,
            c.name,
            c.level,
            c.xp,
            c.zone_kind,
            c.zone_guild_id,
            c.pos[0] as f64,
            c.pos[1] as f64,
            c.pos[2] as f64,
        ],
    )?;
    Ok(())
}

/// Resolve a character id from a (case-insensitive) name.
pub fn find_character_by_name(conn: &Connection, name: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM characters WHERE name = ?1 COLLATE NOCASE",
        params![name],
        |r| r.get::<_, i64>(0),
    )
    .optional()
}

// ---------------------------------------------------------------------------
// Character inventory
// ---------------------------------------------------------------------------

/// Load a character's inventory as ordered `(item_id, qty)` stacks.
pub fn load_inventory(conn: &Connection, char_id: i64) -> Result<Vec<(u32, u32)>> {
    let mut stmt = conn
        .prepare("SELECT item_id, qty FROM char_inventory WHERE character_id = ?1 ORDER BY slot")?;
    stmt.query_map(params![char_id], |row| {
        Ok((row.get::<_, i64>(0)? as u32, row.get::<_, i64>(1)? as u32))
    })?
    .collect()
}

/// Replace a character's stored inventory with `stacks` (item_id, qty) in order.
pub fn save_inventory(conn: &Connection, char_id: i64, stacks: &[(u32, u32)]) -> Result<()> {
    conn.execute(
        "DELETE FROM char_inventory WHERE character_id = ?1",
        params![char_id],
    )?;
    for (slot, (item_id, qty)) in stacks.iter().enumerate() {
        conn.execute(
            "INSERT INTO char_inventory (character_id, slot, item_id, qty) VALUES (?1,?2,?3,?4)",
            params![char_id, slot as i64, *item_id as i64, *qty as i64],
        )?;
    }
    Ok(())
}

/// Resolve a character's display name.
pub fn character_name(conn: &Connection, char_id: i64) -> Result<Option<String>> {
    conn.query_row(
        "SELECT name FROM characters WHERE id = ?1",
        params![char_id],
        |r| r.get::<_, String>(0),
    )
    .optional()
}

// ---------------------------------------------------------------------------
// Guilds
// ---------------------------------------------------------------------------

/// A guild row joined with its settings.
#[derive(Clone, Debug)]
pub struct GuildRow {
    pub id: i64,
    pub name: String,
    pub leader_char_id: i64,
    pub motd: String,
    pub exclusive: bool,
    pub join_policy: String,
}

pub fn load_all_guilds(conn: &Connection) -> Result<Vec<GuildRow>> {
    let mut stmt = conn.prepare(
        "SELECT g.id, g.name, g.leader_char_id, g.motd,
                COALESCE(s.exclusive, 0), COALESCE(s.join_policy, 'invite')
         FROM guilds g LEFT JOIN guild_settings s ON s.guild_id = g.id
         ORDER BY g.id",
    )?;
    stmt.query_map([], |row| {
        Ok(GuildRow {
            id: row.get(0)?,
            name: row.get(1)?,
            leader_char_id: row.get(2)?,
            motd: row.get(3)?,
            exclusive: row.get::<_, i64>(4)? != 0,
            join_policy: row.get(5)?,
        })
    })?
    .collect()
}

/// Load all guild memberships as `(guild_id, char_id, rank, char_name)`.
pub fn load_guild_members_named(
    conn: &Connection,
) -> Result<Vec<(i64, i64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT m.guild_id, m.character_id, m.rank, COALESCE(c.name, '?')
         FROM guild_members m LEFT JOIN characters c ON c.id = m.character_id",
    )?;
    stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })?
    .collect()
}

/// Create a guild with `leader_char_id` as its leader, seed default settings,
/// and insert the leader as a member. Returns the new guild id.
pub fn create_guild(conn: &Connection, name: &str, leader_char_id: i64) -> Result<i64> {
    conn.execute(
        "INSERT INTO guilds (name, leader_char_id, motd) VALUES (?1, ?2, '')",
        params![name, leader_char_id],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO guild_settings (guild_id, exclusive, join_policy) VALUES (?1, 0, 'invite')",
        params![id],
    )?;
    conn.execute(
        "INSERT INTO guild_members (guild_id, character_id, rank) VALUES (?1, ?2, 'Leader')",
        params![id, leader_char_id],
    )?;
    Ok(id)
}

pub fn delete_guild(conn: &Connection, guild_id: i64) -> Result<()> {
    conn.execute("DELETE FROM guild_members WHERE guild_id = ?1", params![guild_id])?;
    conn.execute("DELETE FROM guild_settings WHERE guild_id = ?1", params![guild_id])?;
    conn.execute("DELETE FROM island_objects WHERE guild_id = ?1", params![guild_id])?;
    conn.execute("DELETE FROM guilds WHERE id = ?1", params![guild_id])?;
    Ok(())
}

pub fn insert_guild_member(
    conn: &Connection,
    guild_id: i64,
    char_id: i64,
    rank: &str,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO guild_members (guild_id, character_id, rank) VALUES (?1, ?2, ?3)",
        params![guild_id, char_id, rank],
    )?;
    Ok(())
}

pub fn delete_guild_member(conn: &Connection, guild_id: i64, char_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM guild_members WHERE guild_id = ?1 AND character_id = ?2",
        params![guild_id, char_id],
    )?;
    Ok(())
}

pub fn update_guild_member_rank(
    conn: &Connection,
    guild_id: i64,
    char_id: i64,
    rank: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE guild_members SET rank = ?3 WHERE guild_id = ?1 AND character_id = ?2",
        params![guild_id, char_id, rank],
    )?;
    Ok(())
}

pub fn update_guild_settings(
    conn: &Connection,
    guild_id: i64,
    exclusive: bool,
    join_policy: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO guild_settings (guild_id, exclusive, join_policy) VALUES (?1, ?2, ?3)
         ON CONFLICT(guild_id) DO UPDATE SET exclusive = ?2, join_policy = ?3",
        params![guild_id, exclusive as i64, join_policy],
    )?;
    Ok(())
}

pub fn update_guild_motd(conn: &Connection, guild_id: i64, motd: &str) -> Result<()> {
    conn.execute(
        "UPDATE guilds SET motd = ?2 WHERE id = ?1",
        params![guild_id, motd],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Island objects
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct IslandObjectRow {
    pub id: i64,
    pub guild_id: i64,
    pub prefab_id: u32,
    pub pos: [f32; 3],
    pub rot_y: f32,
    pub scale: f32,
}

pub fn load_island_objects(conn: &Connection, guild_id: i64) -> Result<Vec<IslandObjectRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, guild_id, prefab_id, x, y, z, rot_y, scale
         FROM island_objects WHERE guild_id = ?1 ORDER BY id",
    )?;
    stmt.query_map(params![guild_id], |row| {
        Ok(IslandObjectRow {
            id: row.get(0)?,
            guild_id: row.get(1)?,
            prefab_id: row.get::<_, i64>(2)? as u32,
            pos: [
                row.get::<_, f64>(3)? as f32,
                row.get::<_, f64>(4)? as f32,
                row.get::<_, f64>(5)? as f32,
            ],
            rot_y: row.get::<_, f64>(6)? as f32,
            scale: row.get::<_, f64>(7)? as f32,
        })
    })?
    .collect()
}

pub fn insert_island_object(
    conn: &Connection,
    guild_id: i64,
    prefab_id: u32,
    pos: [f32; 3],
    rot_y: f32,
    scale: f32,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO island_objects (guild_id, prefab_id, x, y, z, rot_y, scale)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            guild_id,
            prefab_id as i64,
            pos[0] as f64,
            pos[1] as f64,
            pos[2] as f64,
            rot_y as f64,
            scale as f64
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_island_object(
    conn: &Connection,
    object_id: i64,
    pos: [f32; 3],
    rot_y: f32,
) -> Result<()> {
    conn.execute(
        "UPDATE island_objects SET x=?2, y=?3, z=?4, rot_y=?5 WHERE id=?1",
        params![
            object_id,
            pos[0] as f64,
            pos[1] as f64,
            pos[2] as f64,
            rot_y as f64
        ],
    )?;
    Ok(())
}

pub fn delete_island_object(conn: &Connection, object_id: i64) -> Result<()> {
    conn.execute("DELETE FROM island_objects WHERE id = ?1", params![object_id])?;
    Ok(())
}

pub fn island_object_count(conn: &Connection, guild_id: i64) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM island_objects WHERE guild_id = ?1",
        params![guild_id],
        |r| r.get(0),
    )
}

/// Find the guild that owns a given island object (for permission checks).
pub fn island_object_guild(conn: &Connection, object_id: i64) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT guild_id FROM island_objects WHERE id = ?1",
        params![object_id],
        |r| r.get::<_, i64>(0),
    )
    .optional()
}

// ---------------------------------------------------------------------------
// Quests
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct QuestRow {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub reward_item: u32,
    pub reward_qty: u32,
    pub reward_xp: u64,
}

#[derive(Clone, Debug)]
pub struct QuestObjectiveRow {
    pub quest_id: u32,
    pub idx: u32,
    pub kind: String,
    pub target_id: u32,
    pub required: u32,
    pub text: String,
}

pub fn load_quests(conn: &Connection) -> Result<Vec<QuestRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, reward_item, reward_qty, reward_xp FROM quests ORDER BY id",
    )?;
    stmt.query_map([], |row| {
        Ok(QuestRow {
            id: row.get::<_, i64>(0)? as u32,
            name: row.get(1)?,
            description: row.get(2)?,
            reward_item: row.get::<_, i64>(3)? as u32,
            reward_qty: row.get::<_, i64>(4)? as u32,
            reward_xp: row.get::<_, i64>(5)? as u64,
        })
    })?
    .collect()
}

pub fn load_quest_objectives(conn: &Connection) -> Result<Vec<QuestObjectiveRow>> {
    let mut stmt = conn.prepare(
        "SELECT quest_id, idx, kind, target_id, required, text FROM quest_objectives
         ORDER BY quest_id, idx",
    )?;
    stmt.query_map([], |row| {
        Ok(QuestObjectiveRow {
            quest_id: row.get::<_, i64>(0)? as u32,
            idx: row.get::<_, i64>(1)? as u32,
            kind: row.get(2)?,
            target_id: row.get::<_, i64>(3)? as u32,
            required: row.get::<_, i64>(4)? as u32,
            text: row.get(5)?,
        })
    })?
    .collect()
}

/// Load a character's quest progress as `(quest_id, objective_idx, count, state)`.
pub fn load_quest_progress(
    conn: &Connection,
    char_id: i64,
) -> Result<Vec<(u32, u32, u32, String)>> {
    let mut stmt = conn.prepare(
        "SELECT quest_id, objective_idx, count, state FROM quest_progress WHERE character_id = ?1",
    )?;
    stmt.query_map(params![char_id], |row| {
        Ok((
            row.get::<_, i64>(0)? as u32,
            row.get::<_, i64>(1)? as u32,
            row.get::<_, i64>(2)? as u32,
            row.get(3)?,
        ))
    })?
    .collect()
}

pub fn upsert_quest_progress(
    conn: &Connection,
    char_id: i64,
    quest_id: u32,
    objective_idx: u32,
    count: u32,
    state: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO quest_progress (character_id, quest_id, objective_idx, count, state)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(character_id, quest_id, objective_idx)
         DO UPDATE SET count = ?4, state = ?5",
        params![char_id, quest_id as i64, objective_idx as i64, count as i64, state],
    )?;
    Ok(())
}

pub fn delete_quest_progress(conn: &Connection, char_id: i64, quest_id: u32) -> Result<()> {
    conn.execute(
        "DELETE FROM quest_progress WHERE character_id = ?1 AND quest_id = ?2",
        params![char_id, quest_id as i64],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Mail
// ---------------------------------------------------------------------------

pub fn insert_mail(
    conn: &Connection,
    to_char_id: i64,
    from_name: &str,
    subject: &str,
    body: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO mail (to_char_id, from_name, subject, body) VALUES (?1, ?2, ?3, ?4)",
        params![to_char_id, from_name, subject, body],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Load a character's mailbox as `(id, from_name, subject, body, read)`.
pub fn load_mail(conn: &Connection, char_id: i64) -> Result<Vec<(i64, String, String, String, bool)>> {
    let mut stmt = conn.prepare(
        "SELECT id, from_name, subject, body, read FROM mail
         WHERE to_char_id = ?1 ORDER BY id DESC",
    )?;
    stmt.query_map(params![char_id], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get::<_, i64>(4)? != 0,
        ))
    })?
    .collect()
}

pub fn mark_mail_read(conn: &Connection, mail_id: i64, char_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE mail SET read = 1 WHERE id = ?1 AND to_char_id = ?2",
        params![mail_id, char_id],
    )?;
    Ok(())
}

/// Parse a stored key string (e.g. `"F1"`) back into a [`KeyCode`].
pub fn key_code_from_str(s: &str) -> KeyCode {
    match s {
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F8" => KeyCode::F8,
        "F9" => KeyCode::F9,
        "F10" => KeyCode::F10,
        "F11" => KeyCode::F11,
        "F12" => KeyCode::F12,
        _ => KeyCode::F1,
    }
}
