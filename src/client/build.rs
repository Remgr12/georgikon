//! Client build mode for guild islands + rendering of replicated island props.
//!
//! Toggle build mode with `B` (only meaningful on your guild island). Left-click
//! places the selected prefab in front of you; right-click removes the nearest
//! prop. `Tab` cycles the prefab; `T` rotates the next placement by 45°.

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::{MessageReceiver, MessageSender};
use std::collections::HashMap;
use std::f32::consts::PI;

use crate::client::chat::ChatState;
use crate::client::input::{ActionState, GameAction};
use crate::client::player::Player;
use crate::client::world::CurrentZone;
use crate::common::island::{
    IslandObjectInfo, PlaceObjectMessage, PrefabCatalogMessage, PrefabInfo, RemoveObjectMessage,
};
use crate::common::zone::ZoneId;
use crate::net::{PlayerPosition, ReliableChannel};

#[derive(Resource, Default)]
struct PrefabStore {
    by_id: HashMap<u32, PrefabInfo>,
    ordered: Vec<u32>,
}

#[derive(Resource, Default)]
struct BuildMode {
    active: bool,
    selected: usize,
    rot_y: f32,
}

/// Marks a client-spawned visual for a replicated island object.
#[derive(Component)]
struct IslandObjectVisual;

#[derive(Component)]
struct BuildHudText;

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PrefabStore>();
        app.init_resource::<BuildMode>();
        app.add_systems(Startup, spawn_build_hud);
        app.add_systems(
            Update,
            (
                recv_catalog,
                spawn_island_visuals,
                toggle_build,
                build_actions,
                render_build_hud,
            ),
        );
    }
}

fn spawn_build_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(320.0),
                left: Val::Px(10.0),
                ..default()
            },
            GlobalZIndex(120),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.9, 1.0)),
                BuildHudText,
            ));
        });
}

fn recv_catalog(
    mut store: ResMut<PrefabStore>,
    mut q: Query<&mut MessageReceiver<PrefabCatalogMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.ordered.clear();
            store.by_id.clear();
            for p in msg.prefabs {
                store.ordered.push(p.id);
                store.by_id.insert(p.id, p);
            }
        }
    }
}

/// Spawn a colored mesh for each replicated island object.
fn spawn_island_visuals(
    mut commands: Commands,
    store: Res<PrefabStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<(Entity, &IslandObjectInfo, &PlayerPosition), Added<IslandObjectInfo>>,
) {
    for (entity, info, pos) in q.iter() {
        let (color, size) = store
            .by_id
            .get(&info.prefab_id)
            .map(|p| (p.color, p.size))
            .unwrap_or(([0.7, 0.7, 0.7], [1.0, 1.0, 1.0]));
        let mesh = meshes.add(Cuboid::new(size[0], size[1], size[2]));
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgb(color[0], color[1], color[2]),
            ..default()
        });
        let mut transform =
            Transform::from_translation(pos.0 + Vec3::new(0.0, size[1] * 0.5, 0.0));
        transform.rotation = Quat::from_rotation_y(info.rot_y);
        transform.scale = Vec3::splat(info.scale);
        commands.entity(entity).insert((
            IslandObjectVisual,
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            transform,
        ));
    }
}

fn toggle_build(
    actions: Res<ActionState>,
    chat: Res<ChatState>,
    mut build: ResMut<BuildMode>,
) {
    if chat.is_typing {
        return;
    }
    if actions.just_pressed(GameAction::ToggleBuild) {
        build.active = !build.active;
    }
    if build.active && actions.just_pressed(GameAction::BuildCycle) {
        build.selected += 1;
    }
    if build.active && actions.just_pressed(GameAction::BuildRotate) {
        build.rot_y = (build.rot_y + PI / 4.0) % (2.0 * PI);
    }
}

fn build_actions(
    build: Res<BuildMode>,
    chat: Res<ChatState>,
    mouse: Res<ButtonInput<MouseButton>>,
    store: Res<PrefabStore>,
    current: Res<CurrentZone>,
    player_q: Query<&Transform, With<Player>>,
    objects_q: Query<(&IslandObjectInfo, &Transform)>,
    mut place_tx: Query<&mut MessageSender<PlaceObjectMessage>, With<Client>>,
    mut remove_tx: Query<&mut MessageSender<RemoveObjectMessage>, With<Client>>,
) {
    if !build.active || chat.is_typing || store.ordered.is_empty() {
        return;
    }
    let ZoneId::GuildIsland(guild_id) = current.0 else {
        return;
    };
    let Ok(player) = player_q.single() else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        let prefab_id = store.ordered[build.selected % store.ordered.len()];
        let fwd = player.forward();
        let target = player.translation + Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero() * 4.0;
        let pos = [target.x, 0.0, target.z];
        if let Ok(mut tx) = place_tx.single_mut() {
            tx.send::<ReliableChannel>(PlaceObjectMessage {
                guild_id,
                prefab_id,
                pos,
                rot_y: build.rot_y,
                scale: 1.0,
            });
        }
    }

    if mouse.just_pressed(MouseButton::Right) {
        // Remove the nearest island object to the player.
        let mut nearest: Option<(u64, f32)> = None;
        for (info, t) in objects_q.iter() {
            let d = t.translation.distance(player.translation);
            if nearest.map_or(true, |(_, bd)| d < bd) {
                nearest = Some((info.object_id, d));
            }
        }
        if let Some((object_id, d)) = nearest {
            if d < 8.0 {
                if let Ok(mut tx) = remove_tx.single_mut() {
                    tx.send::<ReliableChannel>(RemoveObjectMessage { object_id });
                }
            }
        }
    }
}

fn render_build_hud(
    build: Res<BuildMode>,
    store: Res<PrefabStore>,
    mut q: Query<&mut Text, With<BuildHudText>>,
) {
    if !build.is_changed() && !store.is_changed() {
        return;
    }
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    if build.active {
        let name = store
            .ordered
            .get(build.selected % store.ordered.len().max(1))
            .and_then(|id| store.by_id.get(id))
            .map(|p| p.name.as_str())
            .unwrap_or("?");
        text.0 = format!(
            "BUILD MODE  prefab: {}  [LMB place] [RMB remove] [Tab cycle] [T rotate]",
            name
        );
    } else {
        text.0 = String::new();
    }
}
