//! Minimap overlay — top-right corner.
//!
//! Shows a 120×120 px square map.  The local player is always centred; other
//! entities appear as coloured dots scaled by their world-distance.
//! The map covers a 100×100 world-unit radius.  Press M to toggle.

use bevy::prelude::*;

use crate::client::input::{ActionState, GameAction};
use crate::client::login::LocalCharacter;
use crate::client::player::Player;
use crate::common::mob::{UnitKind, UnitVisual};
use crate::net::PlayerPosition;
use crate::screens::Screen;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MinimapState { visible: true });
        app.add_systems(Startup, spawn_minimap_ui);
        app.add_systems(
            Update,
            (toggle_minimap, update_minimap_dots).run_if(in_state(Screen::Gameplay)),
        );
    }
}

// ─── Map parameters ───────────────────────────────────────────────────────────

const MAP_PX: f32 = 120.0;
const MAP_RADIUS: f32 = 80.0; // world units visible in each direction

// ─── Resources & markers ─────────────────────────────────────────────────────

#[derive(Resource)]
struct MinimapState {
    visible: bool,
}

#[derive(Component)]
struct MinimapRoot;

/// A coloured dot on the minimap.
#[derive(Component)]
struct MinimapDot {
    kind: DotKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DotKind {
    LocalPlayer,
    RemotePlayer,
    Wolf,
    Boar,
}

// ─── Spawn ────────────────────────────────────────────────────────────────────

fn spawn_minimap_ui(mut commands: Commands) {
    // Outer frame
    commands
        .spawn((
            MinimapRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                right: Val::Px(12.0),
                width: Val::Px(MAP_PX + 4.0),
                height: Val::Px(MAP_PX + 4.0),
                border: UiRect::all(Val::Px(2.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.06, 0.04, 0.85)),
            BorderColor::all(Color::srgba(0.30, 0.50, 0.30, 0.80)),
            GlobalZIndex(50),
        ))
        .with_children(|map| {
            // Background fill
            map.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.06, 0.12, 0.06, 1.0)),
            ));
            // Cross-hair lines
            map.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Px(0.0),
                    width: Val::Px(1.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.3, 0.5, 0.3, 0.3)),
            ));
            map.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Percent(50.0),
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.3, 0.5, 0.3, 0.3)),
            ));

            // Pre-allocate a pool of dots (max 20 entities).
            // We reuse them by repositioning; extras stay invisible.
            for _ in 0..20 {
                map.spawn((
                    MinimapDot { kind: DotKind::LocalPlayer },
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(5.0),
                        height: Val::Px(5.0),
                        left: Val::Px(-99.0),
                        top: Val::Px(-99.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                    Visibility::Hidden,
                ));
            }
        });
}

// ─── Systems ──────────────────────────────────────────────────────────────────

fn toggle_minimap(
    actions: Res<ActionState>,
    mut state: ResMut<MinimapState>,
    mut root: Query<&mut Visibility, With<MinimapRoot>>,
) {
    if !actions.just_pressed(GameAction::ToggleMinimap) { return; }
    state.visible = !state.visible;
    if let Ok(mut vis) = root.single_mut() {
        *vis = if state.visible { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn update_minimap_dots(
    local: Res<LocalCharacter>,
    local_player_q: Query<&PlayerPosition, With<Player>>,
    // All entities with a position and optional unit visual
    entities_q: Query<(&PlayerPosition, Option<&UnitVisual>), Without<Player>>,
    mut dot_q: Query<(&mut Node, &mut BackgroundColor, &mut Visibility, &mut MinimapDot)>,
    state: Res<MinimapState>,
    root_q: Query<&Visibility, (With<MinimapRoot>, Without<MinimapDot>)>,
) {
    // Check visibility
    if !state.visible { return; }
    if let Ok(vis) = root_q.single() {
        if *vis == Visibility::Hidden { return; }
    }

    let Ok(local_pos) = local_player_q.single() else { return };
    let origin = Vec2::new(local_pos.0.x, local_pos.0.z);

    // Build list of dots to show: local player first, then others
    let mut dot_data: Vec<(Vec2, DotKind)> = vec![(Vec2::ZERO, DotKind::LocalPlayer)];

    for (pos, visual) in entities_q.iter() {
        let world_xz = Vec2::new(pos.0.x, pos.0.z) - origin;
        if world_xz.length() > MAP_RADIUS * 1.5 { continue; }
        let kind = match visual {
            Some(v) => match v.kind {
                UnitKind::Player => DotKind::RemotePlayer,
                UnitKind::Wolf | UnitKind::EliteWolf => DotKind::Wolf,
                UnitKind::Boar => DotKind::Boar,
            },
            None => DotKind::RemotePlayer,
        };
        dot_data.push((world_xz, kind));
    }

    // Map world offset → pixel offset from centre
    let scale = MAP_PX / (2.0 * MAP_RADIUS);
    let centre = MAP_PX * 0.5;

    let mut dot_iter = dot_q.iter_mut();
    let mut data_iter = dot_data.iter();

    loop {
        match (dot_iter.next(), data_iter.next()) {
            (Some((mut node, mut bg, mut vis, mut dot_marker)), Some((offset, kind))) => {
                let px_x = centre + offset.x * scale - 2.5;
                let px_y = centre - offset.y * scale - 2.5; // Z = down in world → up in screen
                node.left = Val::Px(px_x);
                node.top = Val::Px(px_y);
                let color = match kind {
                    DotKind::LocalPlayer => Color::srgb(0.20, 0.85, 0.30),
                    DotKind::RemotePlayer => Color::srgb(0.35, 0.65, 1.0),
                    DotKind::Wolf => Color::srgb(0.85, 0.20, 0.20),
                    DotKind::Boar => Color::srgb(0.85, 0.55, 0.15),
                };
                *bg = BackgroundColor(color);
                dot_marker.kind = *kind;
                *vis = Visibility::Inherited;
            }
            (Some((mut node, mut bg, mut vis, _)), None) => {
                // Hide unused dots
                *vis = Visibility::Hidden;
                *bg = BackgroundColor(Color::NONE);
                node.left = Val::Px(-99.0);
            }
            _ => break,
        }
    }
}
