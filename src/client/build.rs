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
use crate::client::iso::{ISO_FORWARD, world_to_transform};
use crate::client::player::Player;
use crate::client::sprite::{AnimatedSprite, SpriteAssets, SpriteKind};
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
                TextFont { font_size: 15.0, ..default() },
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

/// Spawn an isometric sprite for each replicated island object.
fn spawn_island_visuals(
    mut commands: Commands,
    store: Res<PrefabStore>,
    assets: Res<SpriteAssets>,
    q: Query<(Entity, &IslandObjectInfo, &PlayerPosition), Added<IslandObjectInfo>>,
) {
    for (entity, info, pos) in q.iter() {
        let (color, size) = store
            .by_id
            .get(&info.prefab_id)
            .map(|p| (p.color, p.size))
            .unwrap_or(([0.7, 0.7, 0.7], [1.0, 1.0, 1.0]));

        // Scale the visual size from world units to screen pixels.
        let screen_w = size[0] * crate::client::iso::TILE_HALF_W * 2.0;
        let screen_h = size[1] * crate::client::iso::Y_LIFT;

        commands.entity(entity).insert((
            IslandObjectVisual,
            AnimatedSprite::new(SpriteKind::IslandProp),
            // Bypass ensure_sprite_components for per-instance colour tinting.
            Sprite {
                image: assets.white.clone(),
                texture_atlas: Some(TextureAtlas {
                    layout: assets.layout(SpriteKind::IslandProp),
                    index: 0,
                }),
                custom_size: Some(Vec2::new(screen_w.max(16.0), screen_h.max(16.0))),
                color: Color::srgb(color[0], color[1], color[2]),
                ..default()
            },
            // project_iso (PostUpdate) will overwrite this each frame.
            world_to_transform(pos.0),
        ));
    }
}

fn toggle_build(
    actions: Res<ActionState>,
    chat: Res<ChatState>,
    mut build: ResMut<BuildMode>,
) {
    if chat.is_typing { return; }
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
    // Use world-space PlayerPosition (not screen-space Transform).
    player_q: Query<&PlayerPosition, With<Player>>,
    objects_q: Query<(&IslandObjectInfo, &PlayerPosition), Without<Player>>,
    mut place_tx: Query<&mut MessageSender<PlaceObjectMessage>, With<Client>>,
    mut remove_tx: Query<&mut MessageSender<RemoveObjectMessage>, With<Client>>,
) {
    if !build.active || chat.is_typing || store.ordered.is_empty() { return; }
    let ZoneId::GuildIsland(guild_id) = current.0 else { return; };
    let Ok(player_pos) = player_q.single() else { return; };

    if mouse.just_pressed(MouseButton::Left) {
        let prefab_id = store.ordered[build.selected % store.ordered.len()];
        // Place 4 world units in the iso-forward direction from the player.
        let fwd = ISO_FORWARD.normalize_or_zero();
        let target = player_pos.0 + fwd * 4.0;
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
        // Remove the nearest island object (world-space distance).
        let mut nearest: Option<(u64, f32)> = None;
        for (info, obj_pos) in objects_q.iter() {
            let d = obj_pos.0.distance(player_pos.0);
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
    if !build.is_changed() && !store.is_changed() { return; }
    let Ok(mut text) = q.single_mut() else { return; };
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
