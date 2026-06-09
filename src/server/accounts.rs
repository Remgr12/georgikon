//! Account registration / login and the post-login character spawn.
//!
//! Connection lifecycle: lightyear brings the netcode connection up first, but
//! we deliberately do **not** spawn a game entity on connect. The client must
//! send a [`LoginRequestMessage`] / [`RegisterRequestMessage`]; on success this
//! plugin spawns the authoritative, replicated character entity and records the
//! player in [`OnlinePlayers`].

use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{Connected, Disconnected, MessageReceiver, MessageSender, NetworkVisibility, Replicate};

use crate::common::account::{
    LoginRequestMessage, LoginResultMessage, RegisterRequestMessage, MAX_NAME_LEN,
};
use crate::common::inventory::Inventory;
use crate::common::mob::{UnitKind, UnitVisual};
use crate::common::stats::{CharacterStats, Experience};
use crate::common::zone::{Zone, ZoneId};
use crate::net::{CharacterId, CharacterName, PlayerId, PlayerPosition, ReliableChannel};
use crate::server::db;
use crate::server::online::{OnlineEntry, OnlinePlayers};
use crate::server::player_state::{AuthoritativePlayerState, OwnedPlayer, OwnerConn};

/// Marker on a connection entity once it has authenticated.
#[derive(Component)]
pub struct Authenticated;

#[derive(Resource)]
struct AutoSaveTimer(Timer);

impl Default for AutoSaveTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(60.0, TimerMode::Repeating))
    }
}

pub struct AccountServerPlugin;

impl Plugin for AccountServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OnlinePlayers>()
            .init_resource::<AutoSaveTimer>()
            .add_observer(on_disconnect)
            .add_systems(Startup, init_db)
            .add_systems(Update, (log_connections, handle_auth, periodic_save));
    }
}

fn init_db() {
    match db::open().and_then(|c| db::init(&c).map(|_| c)) {
        Ok(_) => tracing::info!("Database initialized"),
        Err(e) => tracing::error!("Failed to initialize database: {e}"),
    }
}

fn log_connections(query: Query<Entity, (Added<Connected>, With<ClientOf>)>) {
    for conn in query.iter() {
        tracing::info!("Connection up (awaiting login): {:?}", conn);
    }
}

/// Drain register/login requests on each unauthenticated connection.
fn handle_auth(
    mut commands: Commands,
    mut conn_query: Query<
        (
            Entity,
            &mut MessageReceiver<RegisterRequestMessage>,
            &mut MessageReceiver<LoginRequestMessage>,
            &mut MessageSender<LoginResultMessage>,
        ),
        (With<ClientOf>, Without<Authenticated>),
    >,
    mut online: ResMut<OnlinePlayers>,
) {
    let db_conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("DB unavailable for auth: {e}");
            return;
        }
    };

    for (conn_entity, mut reg_rx, mut login_rx, mut result_tx) in conn_query.iter_mut() {
        let mut attempts: Vec<(String, String, bool)> = Vec::new();
        for m in reg_rx.receive() {
            attempts.push((m.username, m.password, true));
        }
        for m in login_rx.receive() {
            attempts.push((m.username, m.password, false));
        }

        for (username, password, is_register) in attempts {
            let result = authenticate(&db_conn, &username, &password, is_register);
            match result {
                Ok(character) => {
                    let char_id = character.id as u64;
                    if online.is_online(char_id) {
                        result_tx.send::<ReliableChannel>(LoginResultMessage {
                            ok: false,
                            reason: "Character already online".into(),
                            character_id: 0,
                            name: String::new(),
                        });
                        continue;
                    }

                    let game = spawn_character(&mut commands, &db_conn, conn_entity, &character);

                    online.insert(
                        char_id,
                        character.name.clone(),
                        OnlineEntry {
                            conn: conn_entity,
                            game,
                            account_id: character.account_id,
                        },
                    );

                    commands
                        .entity(conn_entity)
                        .insert((Authenticated, OwnedPlayer(game)));

                    result_tx.send::<ReliableChannel>(LoginResultMessage {
                        ok: true,
                        reason: String::new(),
                        character_id: char_id,
                        name: character.name.clone(),
                    });
                    tracing::info!("Login ok: {} (char {})", character.name, char_id);
                    break;
                }
                Err(reason) => {
                    result_tx.send::<ReliableChannel>(LoginResultMessage {
                        ok: false,
                        reason,
                        character_id: 0,
                        name: String::new(),
                    });
                }
            }
        }
    }
}

/// Validate credentials (optionally creating the account) and return the
/// account's character row.
fn authenticate(
    conn: &rusqlite::Connection,
    username: &str,
    password: &str,
    is_register: bool,
) -> Result<db::CharacterRow, String> {
    let username = username.trim();
    if username.is_empty() || username.len() > MAX_NAME_LEN {
        return Err("Invalid username".into());
    }
    if password.is_empty() {
        return Err("Password required".into());
    }

    let account_id = if is_register {
        match db::find_account(conn, username) {
            Ok(Some(_)) => return Err("Username already taken".into()),
            Ok(None) => {}
            Err(e) => return Err(format!("DB error: {e}")),
        }
        let hash = hash_password(password).ok_or("Failed to hash password")?;
        db::create_account(conn, username, &hash).map_err(|e| format!("DB error: {e}"))?
    } else {
        match db::find_account(conn, username) {
            Ok(Some((id, hash))) => {
                if !verify_password(password, &hash) {
                    return Err("Wrong password".into());
                }
                id
            }
            Ok(None) => return Err("No such account".into()),
            Err(e) => return Err(format!("DB error: {e}")),
        }
    };

    db::get_or_create_character(conn, account_id, username).map_err(|e| format!("DB error: {e}"))
}

/// Spawn the authoritative, replicated game entity for a logged-in character.
fn spawn_character(
    commands: &mut Commands,
    conn: &rusqlite::Connection,
    conn_entity: Entity,
    character: &db::CharacterRow,
) -> Entity {
    let zone = ZoneId::Overworld;
    let pos = Vec3::new(character.pos[0], 1.0, character.pos[2]);

    let mut inventory = Inventory::default();
    match db::load_inventory(conn, character.id) {
        Ok(stacks) if !stacks.is_empty() => {
            for (item_id, qty) in stacks {
                inventory.add(item_id, qty);
            }
        }
        _ => {
            inventory.add(1, 1);
            inventory.add(2, 5);
            inventory.add(3, 3);
        }
    }

    let mut stats = CharacterStats::default();
    let level = character.level.max(1) as u32;
    stats.health.max = 100.0 + (level - 1) as f32 * 20.0;
    stats.health.restore_full();

    let experience = Experience {
        level,
        xp: character.xp.max(0) as u64,
    };

    commands
        .spawn((
            PlayerId(conn_entity.to_bits()),
            CharacterId(character.id as u64),
            CharacterName(character.name.clone()),
            PlayerPosition(pos),
            Zone(zone),
            AuthoritativePlayerState {
                position: pos,
                ..Default::default()
            },
            stats.clone(),
            experience,
            inventory,
            UnitVisual {
                kind: UnitKind::Player,
                name: character.name.clone(),
                level,
                health: stats.health.current,
                max_health: stats.health.max,
            },
            OwnerConn(conn_entity),
            Replicate::default(),
            NetworkVisibility::default(),
        ))
        .id()
}

/// On disconnect: persist the character, despawn its game entity, and drop it
/// from the online index.
fn on_disconnect(
    trigger: On<Add, Disconnected>,
    mut commands: Commands,
    mut online: ResMut<OnlinePlayers>,
    query: Query<(
        &CharacterId,
        &AuthoritativePlayerState,
        &Experience,
        &Inventory,
        &Zone,
    )>,
) {
    let conn_entity = trigger.entity;
    let Some(char_id) = online.char_of_conn(conn_entity) else {
        return;
    };
    if let Some(game) = online.game_of(char_id) {
        if let Ok((cid, state, exp, inv, zone)) = query.get(game) {
            persist_character(cid.0, state, exp, inv, zone);
        }
        commands.entity(game).despawn();
    }
    online.remove_by_conn(conn_entity);
    commands.trigger(crate::server::online::PlayerLeft(char_id));
    tracing::info!("Player {} disconnected and saved", char_id);
}

/// Periodically flush all online players to the DB so a crash loses at most 60 s of progress.
fn periodic_save(
    time: Res<Time>,
    mut timer: ResMut<AutoSaveTimer>,
    online: Res<OnlinePlayers>,
    query: Query<(&CharacterId, &AuthoritativePlayerState, &Experience, &Inventory, &Zone)>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    let mut count = 0u32;
    for (_, entry) in online.iter() {
        if let Ok((cid, state, exp, inv, zone)) = query.get(entry.game) {
            persist_character(cid.0, state, exp, inv, zone);
            count += 1;
        }
    }
    if count > 0 {
        tracing::info!("Auto-saved {count} online character(s)");
    }
}

fn persist_character(
    char_id: u64,
    state: &AuthoritativePlayerState,
    exp: &Experience,
    inv: &Inventory,
    zone: &Zone,
) {
    let Ok(conn) = db::open() else { return };
    let (zone_kind, zone_guild_id) = zone.0.to_db();
    let mut row = match db::load_character(&conn, char_id as i64) {
        Ok(Some(r)) => r,
        _ => return,
    };
    row.level = exp.level as i64;
    row.xp = exp.xp as i64;
    row.pos = [state.position.x, state.position.y, state.position.z];
    row.zone_kind = zone_kind.to_string();
    row.zone_guild_id = zone_guild_id;
    let _ = db::save_character(&conn, &row);

    let stacks: Vec<(u32, u32)> = inv.slots.iter().map(|s| (s.item_id, s.quantity)).collect();
    let _ = db::save_inventory(&conn, char_id as i64, &stacks);
}

// ---------------------------------------------------------------------------
// Password hashing (Argon2id)
// ---------------------------------------------------------------------------

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};

fn hash_password(pw: &str) -> Option<String> {
    let salt = SaltString::generate(&mut OsRng);
    // Fast params (m=64KB t=1 p=1) for dev: ~3ms vs ~800ms for defaults.
    // For a production game with real user data, increase m to ≥65536.
    let params = Params::new(64, 1, 1, None).expect("valid argon2 params");
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon
        .hash_password(pw.as_bytes(), &salt)
        .ok()
        .map(|h| h.to_string())
}

fn verify_password(pw: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    // Argon2 reads the params from the stored hash, so verification always uses
    // the correct params regardless of what's set here.
    Argon2::default()
        .verify_password(pw.as_bytes(), &parsed)
        .is_ok()
}
