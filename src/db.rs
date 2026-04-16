//! Thin SQLite helpers. All game content (items, spells) lives in `game.db`
//! next to the binary. Tables are created and seeded on first run.

use bevy::prelude::KeyCode;
use rusqlite::{Connection, Result};

const DB_PATH: &str = "game.db";

pub fn open() -> Result<Connection> {
    Connection::open(DB_PATH)
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
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            name         TEXT    NOT NULL,
            key_code     TEXT    NOT NULL,
            cooldown_secs REAL   NOT NULL,
            color_r      REAL    NOT NULL,
            color_g      REAL    NOT NULL,
            color_b      REAL    NOT NULL
        );",
    )?;

    let item_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))?;
    if item_count == 0 {
        conn.execute_batch(
            "INSERT INTO items VALUES (1, 'Iron Sword',    0.72, 0.72, 0.82);
             INSERT INTO items VALUES (2, 'Health Potion', 0.90, 0.18, 0.18);
             INSERT INTO items VALUES (3, 'Mana Potion',   0.20, 0.30, 0.95);
             INSERT INTO items VALUES (4, 'Gold Coin',     0.95, 0.82, 0.10);
             INSERT INTO items VALUES (5, 'Magic Staff',   0.62, 0.18, 0.88);",
        )?;
    }

    let spell_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM spells", [], |r| r.get(0))?;
    if spell_count == 0 {
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

    Ok(())
}

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

/// Parse a stored key string (e.g. `"F1"`) back into a [`KeyCode`].
pub fn key_code_from_str(s: &str) -> KeyCode {
    match s {
        "F1"  => KeyCode::F1,  "F2"  => KeyCode::F2,  "F3"  => KeyCode::F3,
        "F4"  => KeyCode::F4,  "F5"  => KeyCode::F5,  "F6"  => KeyCode::F6,
        "F7"  => KeyCode::F7,  "F8"  => KeyCode::F8,  "F9"  => KeyCode::F9,
        "F10" => KeyCode::F10, "F11" => KeyCode::F11, "F12" => KeyCode::F12,
        _     => KeyCode::F1,
    }
}
