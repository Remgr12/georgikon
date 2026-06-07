//! Guild-island server plugin.
//!
//! Owns: lazy-loading each guild island's placed objects, validating + persisting
//! freeform build operations (place/move/remove), zone travel, and shipping the
//! prefab catalog to clients.

use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{MessageReceiver, MessageSender, NetworkVisibility, Replicate};
use std::collections::{HashMap, HashSet};

use crate::common::island::*;
use crate::common::zone::{Zone, ZoneId};
use crate::net::{PlayerPosition, ReliableChannel};
use crate::server::accounts::Authenticated;
use crate::server::db;
use crate::server::guild::GuildRegistry;
use crate::server::online::OnlinePlayers;
use crate::server::player_state::AuthoritativePlayerState;

/// Spawn point within any zone.
const SPAWN: Vec3 = Vec3::new(0.0, 1.0, 0.0);

/// object_id → spawned entity, for move/remove.
#[derive(Resource, Default)]
struct IslandObjects {
    by_id: HashMap<u64, Entity>,
    loaded_guilds: HashSet<u64>,
}

/// Cached prefab catalog (loaded once at startup).
#[derive(Resource, Default)]
struct PrefabCatalog(PrefabCatalogMessage);

pub struct IslandServerPlugin;

impl Plugin for IslandServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IslandObjects>();
        app.init_resource::<PrefabCatalog>();
        app.add_systems(Startup, load_prefab_catalog);
        app.add_systems(
            Update,
            (send_catalog_on_login, handle_travel, handle_build_ops),
        );
    }
}

fn load_prefab_catalog(mut catalog: ResMut<PrefabCatalog>) {
    let Ok(conn) = db::open() else { return };
    let Ok(rows) = db::load_prefabs(&conn) else { return };
    catalog.0 = PrefabCatalogMessage {
        prefabs: rows
            .into_iter()
            .map(|p| PrefabInfo {
                id: p.id,
                name: p.name,
                color: p.color,
                size: p.size,
            })
            .collect(),
    };
}

/// Send the prefab catalog to each newly-authenticated connection.
fn send_catalog_on_login(
    catalog: Res<PrefabCatalog>,
    mut q: Query<&mut MessageSender<PrefabCatalogMessage>, (With<ClientOf>, Added<Authenticated>)>,
) {
    for mut tx in q.iter_mut() {
        tx.send::<ReliableChannel>(catalog.0.clone());
    }
}

/// Ensure a guild island's objects are spawned (idempotent).
fn ensure_island_loaded(
    commands: &mut Commands,
    objects: &mut IslandObjects,
    guild_id: u64,
) {
    if objects.loaded_guilds.contains(&guild_id) {
        return;
    }
    objects.loaded_guilds.insert(guild_id);
    let Ok(conn) = db::open() else { return };
    let Ok(rows) = db::load_island_objects(&conn, guild_id as i64) else {
        return;
    };
    for row in rows {
        let e = spawn_object(commands, guild_id, &row);
        objects.by_id.insert(row.id as u64, e);
    }
}

fn spawn_object(commands: &mut Commands, guild_id: u64, row: &db::IslandObjectRow) -> Entity {
    commands
        .spawn((
            IslandObjectInfo {
                object_id: row.id as u64,
                prefab_id: row.prefab_id,
                rot_y: row.rot_y,
                scale: row.scale,
            },
            PlayerPosition(Vec3::from(row.pos)),
            Zone(ZoneId::GuildIsland(guild_id)),
            NetworkVisibility::default(),
            Replicate::default(),
        ))
        .id()
}

fn handle_travel(
    mut commands: Commands,
    online: Res<OnlinePlayers>,
    guilds: Res<GuildRegistry>,
    mut objects: ResMut<IslandObjects>,
    mut conn_q: Query<
        (
            Entity,
            &mut MessageReceiver<TravelRequestMessage>,
            &mut MessageSender<ZoneChangedMessage>,
        ),
        With<ClientOf>,
    >,
    mut players: Query<(&mut Zone, &mut PlayerPosition, &mut AuthoritativePlayerState)>,
) {
    for (conn, mut rx, mut zone_tx) in conn_q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn) else {
            continue;
        };
        for msg in rx.receive() {
            let target = match msg.target {
                TravelTarget::Overworld => ZoneId::Overworld,
                TravelTarget::GuildIsland(g) => {
                    if !guilds.is_member(char_id, g) {
                        continue;
                    }
                    ensure_island_loaded(&mut commands, &mut objects, g);
                    ZoneId::GuildIsland(g)
                }
            };
            if let Some(game) = online.game_of(char_id) {
                if let Ok((mut zone, mut pos, mut state)) = players.get_mut(game) {
                    zone.0 = target;
                    pos.0 = SPAWN;
                    state.position = SPAWN;
                }
            }
            zone_tx.send::<ReliableChannel>(ZoneChangedMessage { zone: target });
        }
    }
}

fn handle_build_ops(
    mut commands: Commands,
    online: Res<OnlinePlayers>,
    guilds: Res<GuildRegistry>,
    mut objects: ResMut<IslandObjects>,
    mut conn_q: Query<
        (
            Entity,
            &mut MessageReceiver<PlaceObjectMessage>,
            &mut MessageReceiver<MoveObjectMessage>,
            &mut MessageReceiver<RemoveObjectMessage>,
        ),
        With<ClientOf>,
    >,
    mut obj_q: Query<(&mut PlayerPosition, &mut IslandObjectInfo)>,
) {
    let Ok(conn) = db::open() else { return };
    for (conn_e, mut place_rx, mut move_rx, mut remove_rx) in conn_q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn_e) else {
            continue;
        };

        for msg in place_rx.receive() {
            if !guilds.can_build(char_id, msg.guild_id) {
                continue;
            }
            if db::island_object_count(&conn, msg.guild_id as i64).unwrap_or(0) as usize
                >= MAX_ISLAND_OBJECTS
            {
                continue;
            }
            let scale = msg.scale.clamp(0.25, 8.0);
            let pos = clamp_to_island(Vec3::from(msg.pos));
            match db::insert_island_object(&conn, msg.guild_id as i64, msg.prefab_id, pos.into(), msg.rot_y, scale)
            {
                Ok(id) => {
                    let row = db::IslandObjectRow {
                        id,
                        guild_id: msg.guild_id as i64,
                        prefab_id: msg.prefab_id,
                        pos: pos.into(),
                        rot_y: msg.rot_y,
                        scale,
                    };
                    let e = spawn_object(&mut commands, msg.guild_id, &row);
                    objects.by_id.insert(id as u64, e);
                }
                Err(e) => tracing::warn!("place object failed: {e}"),
            }
        }

        for msg in move_rx.receive() {
            // Verify the object exists and the player may build on its guild.
            let Ok(Some(guild_id)) = db::island_object_guild(&conn, msg.object_id as i64) else {
                continue;
            };
            if !guilds.can_build(char_id, guild_id as u64) {
                continue;
            }
            let pos = clamp_to_island(Vec3::from(msg.pos));
            let _ = db::update_island_object(&conn, msg.object_id as i64, pos.into(), msg.rot_y);
            if let Some(&e) = objects.by_id.get(&msg.object_id) {
                if let Ok((mut p, mut info)) = obj_q.get_mut(e) {
                    p.0 = pos;
                    info.rot_y = msg.rot_y;
                }
            }
        }

        for msg in remove_rx.receive() {
            let Ok(Some(guild_id)) = db::island_object_guild(&conn, msg.object_id as i64) else {
                continue;
            };
            if !guilds.can_build(char_id, guild_id as u64) {
                continue;
            }
            let _ = db::delete_island_object(&conn, msg.object_id as i64);
            if let Some(e) = objects.by_id.remove(&msg.object_id) {
                commands.entity(e).despawn();
            }
        }
    }
}

/// Keep placed objects within the island platform bounds.
fn clamp_to_island(p: Vec3) -> Vec3 {
    const HALF: f32 = 120.0;
    Vec3::new(p.x.clamp(-HALF, HALF), p.y.max(0.0), p.z.clamp(-HALF, HALF))
}
