use bevy::prelude::*;
use bevy_third_person_camera::{
    ThirdPersonCamera, ThirdPersonCameraPlugin, Zoom,
};

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ThirdPersonCameraPlugin)
            .add_systems(Startup, spawn_camera);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        ThirdPersonCamera {
            zoom: Zoom::new(3.0, 12.0),
            sensitivity: Vec2::new(2.5, 2.5),
            cursor_lock_key: KeyCode::Tab,
            offset_enabled: false,
            ..default()
        },
        Camera3d::default(),
    ));
}