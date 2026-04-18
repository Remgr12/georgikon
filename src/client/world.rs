use bevy::prelude::*;
use std::f32::consts::PI;

pub struct WorldPlugin;

const GROUND_SIZE: f32 = 100.0;
const GROUND_THICKNESS: f32 = 8.0;
pub const GROUND_TOP_Y: f32 = 0.0;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_world);
    }
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Deep ground platform (top stays at y=0.0).
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(GROUND_SIZE, GROUND_THICKNESS, GROUND_SIZE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.5, 0.3),
            ..default()
        })),
        Transform::from_xyz(0.0, GROUND_TOP_Y - (GROUND_THICKNESS * 0.5), 0.0),
    ));

    // Reference pillars
    let pillar_mesh = meshes.add(Cuboid::new(1.0, 4.0, 1.0));
    let pillar_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.5, 0.5, 0.5),
        ..default()
    });

    for x in [-20.0, 0.0, 20.0] {
        for z in [-20.0, 20.0] {
            commands.spawn((
                Mesh3d(pillar_mesh.clone()),
                MeshMaterial3d(pillar_mat.clone()),
                Transform::from_xyz(x, 2.0, z),
            ));
        }
    }

    // Sun
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -PI / 4.0, PI / 4.0, 0.0)),
    ));
}
