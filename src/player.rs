use bevy::prelude::*;
use crate::camera::ThirdPersonCamera;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player)
            .add_systems(Update, move_player);
    }
}

#[derive(Component)]
pub struct Player;

const SPEED: f32 = 5.0;

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Player,
        Mesh3d(meshes.add(Capsule3d::new(0.5, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.2),
            ..default()
        })),
        Transform::from_xyz(0.0, 1.0, 0.0),
    ));
}

fn move_player(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    camera_query: Query<&Transform, (With<ThirdPersonCamera>, Without<Player>)>,
    mut player_query: Query<&mut Transform, With<Player>>,
) {
    let Ok(cam_transform) = camera_query.single() else {
        return;
    };
    let Ok(mut player_transform) = player_query.single_mut() else {
        return;
    };

    // Project camera axes onto the XZ plane for movement
    let cam_fwd = cam_transform.forward();
    let cam_right = cam_transform.right();
    let forward = Vec3::new(cam_fwd.x, 0.0, cam_fwd.z).normalize_or_zero();
    let right = Vec3::new(cam_right.x, 0.0, cam_right.z).normalize_or_zero();

    let mut direction = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        direction += forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        direction -= forward;
    }
    if keys.pressed(KeyCode::KeyA) {
        direction -= right;
    }
    if keys.pressed(KeyCode::KeyD) {
        direction += right;
    }

    if direction.length_squared() > 0.0 {
        direction = direction.normalize();
        player_transform.translation += direction * SPEED * time.delta_secs();

        // Rotate player to face movement direction
        let target = player_transform.translation + direction;
        player_transform.look_at(target, Vec3::Y);
    }
}
