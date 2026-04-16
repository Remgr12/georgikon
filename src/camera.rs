use crate::player::Player;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy_third_person_camera::{CameraSyncSet, Offset, ThirdPersonCamera, ThirdPersonCameraPlugin, Zoom};

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ThirdPersonCameraPlugin)
            .add_systems(Startup, spawn_camera)
            // Flip mouse Y before the library reads it in PreUpdate.
            .add_systems(First, invert_mouse_y)
            // Clamp camera above ground after the library finalises position.
            .add_systems(PostUpdate, clamp_camera_to_ground.after(CameraSyncSet));
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        ThirdPersonCamera {
            zoom: Zoom::new(3.0, 12.0),
            sensitivity: Vec2::new(2.5, 2.5),
            cursor_lock_key: KeyCode::Tab, // Space is reserved for jump
            offset_enabled: true,
            offset: Offset::new(0.5, 0.25),
            offset_toggle_enabled: true,
            offset_toggle_key: KeyCode::KeyT,
            offset_toggle_speed: 5.0,
            ..default()
        },
        Camera3d::default(),
    ));
}

/// Inverts the vertical mouse axis by mutating MouseMotion messages in place
/// before the library's orbit system consumes them.
fn invert_mouse_y(mut mouse: MessageMutator<MouseMotion>) {
    for ev in mouse.read() {
        ev.delta.y = -ev.delta.y;
    }
}

/// Prevents the camera from dipping below the ground plane by clamping its Y
/// and re-aiming at the player when the clamp activates.
fn clamp_camera_to_ground(
    player_q: Query<&Transform, (With<Player>, Without<ThirdPersonCamera>)>,
    mut cam_q: Query<&mut Transform, With<ThirdPersonCamera>>,
) {
    let Ok(player) = player_q.single() else {
        return;
    };
    let Ok(mut cam) = cam_q.single_mut() else {
        return;
    };

    const MIN_CAM_Y: f32 = 0.1;
    if cam.translation.y < MIN_CAM_Y {
        cam.translation.y = MIN_CAM_Y;
        cam.look_at(player.translation + Vec3::Y * 1.0, Vec3::Y);
    }
}
