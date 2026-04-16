use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use crate::player::Player;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_camera, lock_cursor))
            .add_systems(Update, (orbit_camera, toggle_cursor_lock));
    }
}

/// Spherical-coordinate orbit camera that follows the player.
#[derive(Component)]
pub struct ThirdPersonCamera {
    /// Horizontal angle around the player (radians).
    pub yaw: f32,
    /// Vertical angle above the player (radians). Clamped to avoid gimbal flip.
    pub pitch: f32,
    /// Distance from the follow target.
    pub distance: f32,
    pub mouse_sensitivity: f32,
}

impl Default for ThirdPersonCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.35, // ~20° above horizon
            distance: 8.0,
            mouse_sensitivity: 0.003,
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        ThirdPersonCamera::default(),
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 8.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
}

fn lock_cursor(mut cursor_query: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    let Ok(mut cursor) = cursor_query.single_mut() else {
        return;
    };
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

/// Press Escape to release / re-lock the cursor.
fn toggle_cursor_lock(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor_query: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    let Ok(mut cursor) = cursor_query.single_mut() else {
        return;
    };
    match cursor.grab_mode {
        CursorGrabMode::Locked => {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
        _ => {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
    }
}

fn orbit_camera(
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut camera_query: Query<(&mut ThirdPersonCamera, &mut Transform)>,
    player_query: Query<&Transform, (With<Player>, Without<ThirdPersonCamera>)>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok((mut cam, mut cam_transform)) = camera_query.single_mut() else {
        return;
    };

    // delta.x positive = mouse right → rotate camera right (yaw decreases)
    cam.yaw -= mouse_motion.delta.x * cam.mouse_sensitivity;
    // delta.y negative = mouse up → camera rises (pitch increases)
    cam.pitch -= mouse_motion.delta.y * cam.mouse_sensitivity;

    // Prevent the camera from flipping over the top or clipping under the ground
    cam.pitch = cam.pitch.clamp(-0.15, 1.4);

    // Convert spherical → Cartesian offset from the player
    let offset = Vec3::new(
        cam.yaw.sin() * cam.pitch.cos(),
        cam.pitch.sin(),
        cam.yaw.cos() * cam.pitch.cos(),
    ) * cam.distance;

    // Target the player's chest rather than their feet
    let target = player_transform.translation + Vec3::Y * 1.0;

    cam_transform.translation = target + offset;
    cam_transform.look_at(target, Vec3::Y);
}
