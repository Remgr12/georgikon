use crate::client::input::{ActionState, GameAction};
use crate::common::inventory::{Hotbar, Inventory, ItemRegistry, Spell, SpellBook};
use crate::net::{PlayerId, PlayerPosition};
use crate::server::db;
use crate::settings::Settings;
use bevy::prelude::*;
use bevy_third_person_camera::{ThirdPersonCamera, ThirdPersonCameraTarget};

pub struct ClientPlayerPlugin;

impl Plugin for ClientPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (seed_item_registry, spawn_player).chain())
            .add_systems(
                Update,
                (
                    move_player,
                    apply_jump,
                    handle_combat_input,
                    sync_local_player_position,
                    sync_remote_player_position,
                    spawn_remote_players,
                ),
            );
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct MovementState {
    pub velocity_y: f32,
    pub sprinting: bool,
}

#[derive(Component, Default)]
pub struct CombatState {
    roll_cooldown: f32,
}

impl Default for MovementState {
    fn default() -> Self {
        Self {
            velocity_y: 0.0,
            sprinting: false,
        }
    }
}

const GROUND_Y: f32 = 1.0;

fn seed_item_registry(mut registry: ResMut<ItemRegistry>) {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            error!("DB open failed: {e}");
            return;
        }
    };
    if let Err(e) = db::init(&conn) {
        error!("DB init failed: {e}");
        return;
    }
    match db::load_items(&conn) {
        Ok(rows) => {
            for (id, name, r, g, b) in rows {
                registry.register(id, name, Color::srgb(r, g, b));
            }
        }
        Err(e) => error!("Failed to load items: {e}"),
    }
}

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let spells: Vec<Spell> = db::open()
        .and_then(|conn| db::load_spells(&conn))
        .map(|rows| {
            rows.into_iter()
                .map(|(name, key_str, cooldown_secs, r, g, b)| Spell {
                    name,
                    key: db::key_code_from_str(&key_str),
                    cooldown_secs,
                    remaining_cooldown: 0.0,
                    color: Color::srgb(r, g, b),
                })
                .collect()
        })
        .unwrap_or_else(|e| {
            error!("Failed to load spells: {e}");
            vec![]
        });

    let mut inventory = Inventory::default();
    inventory.add(1, 1);
    inventory.add(2, 5);
    inventory.add(3, 3);

    let mut hotbar = Hotbar::default();
    hotbar.bindings[0] = Some(0);
    hotbar.bindings[1] = Some(1);
    hotbar.bindings[2] = Some(2);

    commands.spawn((
        Player,
        PlayerId(0),
        PlayerPosition(Vec3::new(0.0, 1.0, 0.0)),
        ThirdPersonCameraTarget,
        MovementState::default(),
        CombatState::default(),
        inventory,
        hotbar,
        SpellBook { spells },
        Mesh3d(meshes.add(Capsule3d::new(0.5, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.2),
            ..default()
        })),
        Transform::from_xyz(0.0, 1.0, 0.0),
    ));
}

fn sync_local_player_position(
    mut query: Query<(&Transform, &mut PlayerPosition), (With<Player>, Changed<Transform>)>,
) {
    for (transform, mut player_pos) in query.iter_mut() {
        player_pos.0 = transform.translation;
    }
}

fn sync_remote_player_position(
    mut query: Query<(&PlayerPosition, &mut Transform), (Without<Player>, Changed<PlayerPosition>)>,
) {
    for (player_pos, mut transform) in query.iter_mut() {
        transform.translation = player_pos.0;
    }
}

fn spawn_remote_players(
    mut commands: Commands,
    query: Query<Entity, (Added<PlayerId>, Without<Player>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in query.iter() {
        commands.entity(entity).insert((
            Mesh3d(meshes.add(Capsule3d::new(0.5, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.2, 0.8),
                ..default()
            })),
            Transform::from_xyz(0.0, 1.0, 0.0),
        ));
    }
}

fn move_player(
    action_state: Res<ActionState>,
    settings: Res<Settings>,
    time: Res<Time>,
    camera_query: Query<&Transform, (With<ThirdPersonCamera>, Without<Player>)>,
    mut player_query: Query<(&mut Transform, &mut MovementState), With<Player>>,
) {
    let Ok(cam_transform) = camera_query.single() else {
        return;
    };
    let Ok((mut player_transform, mut movement)) = player_query.single_mut() else {
        return;
    };

    let cam_fwd = cam_transform.forward();
    let cam_right = cam_transform.right();
    let forward = Vec3::new(cam_fwd.x, 0.0, cam_fwd.z).normalize_or_zero();
    let right = Vec3::new(cam_right.x, 0.0, cam_right.z).normalize_or_zero();

    let axis = action_state.movement_axis();
    let mut direction = forward * axis.y + right * axis.x;
    movement.sprinting = action_state.pressed(GameAction::Sprint);
    let speed = if movement.sprinting {
        settings.gameplay.sprint_speed
    } else {
        settings.gameplay.walk_speed
    };

    if direction.length_squared() > 0.0 {
        direction = direction.normalize_or_zero();
        if let Ok(dir) = Dir3::new(direction) {
            player_transform.translation += dir.as_vec3() * speed * time.delta_secs();
            player_transform.look_to(dir, Dir3::Y);
        }
    }
}

fn apply_jump(
    action_state: Res<ActionState>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut MovementState), With<Player>>,
) {
    let Ok((mut transform, mut movement)) = query.single_mut() else {
        return;
    };
    let dt = time.delta_secs();

    let grounded = transform.translation.y <= GROUND_Y + f32::EPSILON;

    if action_state.just_pressed(GameAction::Jump) && grounded {
        movement.velocity_y = settings.gameplay.jump_force;
    }

    movement.velocity_y -= settings.gameplay.gravity * dt;
    transform.translation.y += movement.velocity_y * dt;

    if transform.translation.y < GROUND_Y {
        transform.translation.y = GROUND_Y;
        movement.velocity_y = 0.0;
    }
}

fn handle_combat_input(
    action_state: Res<ActionState>,
    time: Res<Time>,
    mut player_q: Query<&mut CombatState, With<Player>>,
) {
    let Ok(mut combat_state) = player_q.single_mut() else {
        return;
    };

    combat_state.roll_cooldown = (combat_state.roll_cooldown - time.delta_secs()).max(0.0);

    if action_state.just_pressed(GameAction::Primary) {
        info!("Primary input fired");
    }
    if action_state.just_pressed(GameAction::Secondary) {
        info!("Secondary input fired");
    }
    if action_state.just_pressed(GameAction::Block) {
        info!("Block input fired");
    }
    if action_state.just_pressed(GameAction::Roll) && combat_state.roll_cooldown <= 0.0 {
        combat_state.roll_cooldown = 0.6;
        info!("Roll input fired");
    }
}
