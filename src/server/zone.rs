//! Server-side zone → lightyear room management.
//!
//! Every room-scoped entity carries a [`Zone`] + `NetworkVisibility`. This
//! plugin keeps a [`Room`] per [`ZoneId`] and, whenever an entity's `Zone`
//! changes, moves it (and, if it's a player, its connection *sender*) from the
//! old room to the new one. Clients therefore only ever receive the entities in
//! the zone they currently occupy.

use bevy::prelude::*;
use lightyear::prelude::{Room, RoomEvent, RoomTarget};
use std::collections::HashMap;

use crate::common::zone::{Zone, ZoneId};
use crate::server::player_state::OwnerConn;

/// Maps each active zone to its backing [`Room`] entity.
#[derive(Resource, Default)]
pub struct ZoneRooms {
    map: HashMap<ZoneId, Entity>,
}

/// Tracks which room an entity currently belongs to, so we can detect changes.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Roomed(pub ZoneId);

pub struct ZoneServerPlugin;

impl Plugin for ZoneServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ZoneRooms>();
        app.add_systems(Update, manage_zone_rooms);
    }
}

/// Move any entity whose `Zone` differs from its tracked `Roomed` into the
/// correct room. Players (entities with `OwnerConn`) also have their connection
/// added/removed as a room *sender*.
fn manage_zone_rooms(
    mut commands: Commands,
    mut rooms: ResMut<ZoneRooms>,
    query: Query<(Entity, &Zone, Option<&Roomed>, Option<&OwnerConn>)>,
) {
    for (entity, zone, roomed, owner_conn) in query.iter() {
        let desired = zone.0;
        if roomed.map(|r| r.0) == Some(desired) {
            continue;
        }

        // Leave the previous room, if any.
        if let Some(Roomed(old)) = roomed {
            if let Some(&old_room) = rooms.map.get(old) {
                commands.trigger(RoomEvent {
                    room: old_room,
                    target: RoomTarget::RemoveEntity(entity),
                });
                if let Some(conn) = owner_conn {
                    commands.trigger(RoomEvent {
                        room: old_room,
                        target: RoomTarget::RemoveSender(conn.0),
                    });
                }
            }
        }

        // Ensure the destination room exists.
        let new_room = match rooms.map.get(&desired) {
            Some(&r) => r,
            None => {
                let r = commands.spawn(Room::default()).id();
                rooms.map.insert(desired, r);
                r
            }
        };

        commands.trigger(RoomEvent {
            room: new_room,
            target: RoomTarget::AddEntity(entity),
        });
        if let Some(conn) = owner_conn {
            commands.trigger(RoomEvent {
                room: new_room,
                target: RoomTarget::AddSender(conn.0),
            });
        }

        commands.entity(entity).insert(Roomed(desired));
    }
}
