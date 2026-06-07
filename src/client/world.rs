//! World/terrain rendering, made zone-aware: the overworld and guild-island
//! terrains are both spawned but only the current zone's is visible. Travel
//! ([`ZoneChangedMessage`]) flips them and updates [`CurrentZone`].

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::MessageReceiver;
use std::f32::consts::PI;

use crate::common::island::ZoneChangedMessage;
use crate::common::zone::ZoneId;

pub struct WorldPlugin;

const GROUND_SIZE: f32 = 100.0;
const ISLAND_SIZE: f32 = 250.0;
const GROUND_THICKNESS: f32 = 8.0;
pub const GROUND_TOP_Y: f32 = 0.0;

/// The zone the local client currently occupies (drives build target + terrain).
#[derive(Resource, Clone, Copy)]
pub struct CurrentZone(pub ZoneId);

impl Default for CurrentZone {
    fn default() -> Self {
        CurrentZone(ZoneId::Overworld)
    }
}

#[derive(Component)]
struct OverworldTerrain;
#[derive(Component)]
struct IslandTerrain;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentZone>();
        app.add_systems(Startup, spawn_world);
        app.add_systems(Update, recv_zone_changed);
    }
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Overworld ground platform (top stays at y=0.0).
    commands.spawn((
        OverworldTerrain,
        Mesh3d(meshes.add(Cuboid::new(GROUND_SIZE, GROUND_THICKNESS, GROUND_SIZE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.5, 0.3),
            ..default()
        })),
        Transform::from_xyz(0.0, GROUND_TOP_Y - (GROUND_THICKNESS * 0.5), 0.0),
    ));

    // Reference pillars in the overworld.
    let pillar_mesh = meshes.add(Cuboid::new(1.0, 4.0, 1.0));
    let pillar_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.5, 0.5, 0.5),
        ..default()
    });
    for x in [-20.0, 0.0, 20.0] {
        for z in [-20.0, 20.0] {
            commands.spawn((
                OverworldTerrain,
                Mesh3d(pillar_mesh.clone()),
                MeshMaterial3d(pillar_mat.clone()),
                Transform::from_xyz(x, 2.0, z),
            ));
        }
    }

    // Guild-island platform — much larger, hidden until you travel there.
    commands.spawn((
        IslandTerrain,
        Mesh3d(meshes.add(Cuboid::new(ISLAND_SIZE, GROUND_THICKNESS, ISLAND_SIZE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.45, 0.42, 0.30),
            ..default()
        })),
        Transform::from_xyz(0.0, GROUND_TOP_Y - (GROUND_THICKNESS * 0.5), 0.0),
        Visibility::Hidden,
    ));

    // Sun.
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -PI / 4.0, PI / 4.0, 0.0)),
    ));
}

fn recv_zone_changed(
    mut current: ResMut<CurrentZone>,
    mut rx: Query<&mut MessageReceiver<ZoneChangedMessage>, With<Client>>,
    mut overworld: Query<&mut Visibility, (With<OverworldTerrain>, Without<IslandTerrain>)>,
    mut island: Query<&mut Visibility, (With<IslandTerrain>, Without<OverworldTerrain>)>,
) {
    let Ok(mut receiver) = rx.single_mut() else {
        return;
    };
    let mut new_zone = None;
    for msg in receiver.receive() {
        new_zone = Some(msg.zone);
    }
    let Some(zone) = new_zone else { return };
    current.0 = zone;
    let on_island = matches!(zone, ZoneId::GuildIsland(_));
    for mut v in overworld.iter_mut() {
        *v = if on_island { Visibility::Hidden } else { Visibility::Inherited };
    }
    for mut v in island.iter_mut() {
        *v = if on_island { Visibility::Inherited } else { Visibility::Hidden };
    }
}
