//! 2-D isometric follow camera.
//!
//! A single [`Camera2d`] tracks the local player's [`PlayerPosition`].
//! Zoom is driven by the same `ZoomIn`/`ZoomOut` actions that used to control
//! 3-D FOV; the `settings.fov` value is now repurposed as the initial
//! orthographic scale (lower = closer).

use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

use crate::client::input::{ActionState, GameAction};
use crate::client::player::Player;
use crate::net::PlayerPosition;
use crate::screens::Screen;
use crate::settings::Settings;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(OnEnter(Screen::Gameplay), init_zoom)
            .add_systems(
                Update,
                (follow_player, handle_zoom).run_if(in_state(Screen::Gameplay)),
            );
    }
}

/// Marker for the single scene camera.
#[derive(Component)]
pub struct SceneCamera;

const ZOOM_SPEED: f32 = 0.5;
const ZOOM_MIN: f32 = 0.25;
const ZOOM_MAX: f32 = 4.0;

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        SceneCamera,
        Camera2d,
        IsDefaultUiCamera,
        // Z = 0 is fine; Bevy 2D camera renders from -1000 to 1000 by default.
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}

/// Set the initial zoom level from `Settings.fov` (repurposed as zoom factor).
fn init_zoom(
    settings: Res<Settings>,
    mut cam_q: Query<&mut Projection, With<SceneCamera>>,
) {
    let Ok(mut proj) = cam_q.single_mut() else { return };
    let Projection::Orthographic(ref mut ortho) = *proj else { return };
    // Map the FOV value (typically 60–120°) to a zoom scale in [0.25, 2.0].
    ortho.scale = (settings.fov / 60.0).clamp(ZOOM_MIN, ZOOM_MAX);
}

/// Smoothly track the local player's screen position.
fn follow_player(
    player_q: Query<&PlayerPosition, With<Player>>,
    mut camera_q: Query<&mut Transform, (With<SceneCamera>, Without<Player>)>,
) {
    let Ok(pos) = player_q.single() else { return };
    let Ok(mut cam_tf) = camera_q.single_mut() else { return };
    let screen = crate::client::iso::world_to_screen(pos.0);
    cam_tf.translation.x = screen.x;
    cam_tf.translation.y = screen.y;
}

/// Adjust orthographic zoom with ZoomIn / ZoomOut actions.
fn handle_zoom(
    action_state: Res<ActionState>,
    time: Res<Time>,
    mut cam_q: Query<&mut Projection, With<SceneCamera>>,
) {
    let Ok(mut proj) = cam_q.single_mut() else { return };
    let Projection::Orthographic(ref mut ortho) = *proj else { return };
    let dt = time.delta_secs();
    if action_state.pressed(GameAction::ZoomIn) {
        ortho.scale = (ortho.scale - ZOOM_SPEED * dt).max(ZOOM_MIN);
    }
    if action_state.pressed(GameAction::ZoomOut) {
        ortho.scale = (ortho.scale + ZOOM_SPEED * dt).min(ZOOM_MAX);
    }
}
