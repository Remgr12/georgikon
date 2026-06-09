use tracing::{error, info};

use crate::client::input::{ActionState, GameAction};
use crate::client::iso::{ISO_FORWARD, ISO_RIGHT, REST_Y};
use crate::client::login::LocalCharacter;
use crate::client::sprite::{AnimatedSprite, SpriteKind};
use crate::common::inventory::{Hotbar, Inventory, ItemRegistry, Spell, SpellBook};
use crate::common::stats::CharacterStats;
use crate::net::{CharacterId, PlayerId, PlayerPosition};
use crate::game::mood::{ChangeMood, Mood};
use crate::screens::Screen;
use crate::server::db;
use crate::settings::Settings;
use bevy::prelude::*;

pub struct ClientPlayerPlugin;

impl Plugin for ClientPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatMoodTimer>();
        app.add_systems(Startup, seed_item_registry)
            .add_systems(OnEnter(Screen::Gameplay), spawn_player)
            .add_systems(
                Update,
                (
                    move_player,
                    apply_jump,
                    handle_combat_input,
                    update_player_flash,
                    update_combat_mood,
                )
                    .run_if(in_state(Screen::Gameplay)),
            )
            .add_systems(Update, spawn_remote_players);
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
    pub roll_cooldown: f32,
}

/// Tracks time since last damage for mood switching.
#[derive(Resource, Default)]
pub struct CombatMoodTimer {
    pub since_hit: f32,
}

/// Tracks visual flash timers for the local player sprite.
#[derive(Component, Default)]
pub struct PlayerFlash {
    /// Orange flash on attack (seconds remaining).
    pub attack: f32,
    /// Red flash on taking a hit (seconds remaining).
    pub hurt: f32,
    /// Previous health for detecting damage.
    pub prev_health: f32,
}

impl Default for MovementState {
    fn default() -> Self {
        Self { velocity_y: 0.0, sprinting: false }
    }
}

/// World Y at which the player rests on the ground.
const GROUND_Y: f32 = REST_Y;

fn spawn_player(
    mut commands: Commands,
    local: Res<LocalCharacter>,
    existing: Query<(), With<Player>>,
) {
    // Only spawn once (OnEnter(Gameplay) can fire again when returning from the
    // Title screen).
    if !existing.is_empty() {
        return;
    }

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

    let start_pos = Vec3::new(0.0, GROUND_Y, 0.0);

    commands.spawn((
        Player,
        PlayerId(local.id),
        CharacterId(local.id),
        PlayerPosition(start_pos),
        MovementState::default(),
        CombatState::default(),
        CharacterStats::default(),
        PlayerFlash::default(),
        inventory,
        hotbar,
        SpellBook { spells },
        AnimatedSprite::new(SpriteKind::Player),
        crate::client::iso::world_to_transform(start_pos),
    ));
}

/// Spawn a visible sprite for every *other* player the server replicates to us.
/// Our own replicated entity (matching LocalCharacter) is skipped.
fn spawn_remote_players(
    mut commands: Commands,
    query: Query<(Entity, &CharacterId), (Added<CharacterId>, Without<Player>)>,
    local: Res<LocalCharacter>,
) {
    for (entity, char_id) in query.iter() {
        if char_id.0 == local.id {
            continue;
        }
        commands.entity(entity).insert((
            AnimatedSprite::new(SpriteKind::Player),
            // Transform will be set by project_iso in PostUpdate.
            Transform::default(),
        ));
    }
}

/// Move the local player in world space using the fixed isometric basis.
///
/// Updates [`PlayerPosition`] directly; [`project_iso`] (PostUpdate) converts
/// that to the screen-space [`Transform`] for rendering.
fn move_player(
    action_state: Res<ActionState>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut player_query: Query<(&mut PlayerPosition, &mut MovementState), With<Player>>,
) {
    let Ok((mut player_pos, mut movement)) = player_query.single_mut() else { return };

    let forward = ISO_FORWARD.normalize_or_zero();
    let right = ISO_RIGHT.normalize_or_zero();

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
        player_pos.0 += direction * speed * time.delta_secs();
    }
}

/// Apply gravity and jump impulse to the player's Y world position.
fn apply_jump(
    action_state: Res<ActionState>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut query: Query<(&mut PlayerPosition, &mut MovementState), With<Player>>,
) {
    let Ok((mut player_pos, mut movement)) = query.single_mut() else { return };
    let dt = time.delta_secs();

    let grounded = player_pos.0.y <= GROUND_Y + f32::EPSILON;

    if action_state.just_pressed(GameAction::Jump) && grounded {
        movement.velocity_y = settings.gameplay.jump_force;
    }

    movement.velocity_y -= settings.gameplay.gravity * dt;
    player_pos.0.y += movement.velocity_y * dt;

    if player_pos.0.y < GROUND_Y {
        player_pos.0.y = GROUND_Y;
        movement.velocity_y = 0.0;
    }
}

fn handle_combat_input(
    action_state: Res<ActionState>,
    time: Res<Time>,
    mut player_q: Query<&mut CombatState, With<Player>>,
) {
    let Ok(mut combat) = player_q.single_mut() else { return };
    if combat.roll_cooldown > 0.0 {
        combat.roll_cooldown = (combat.roll_cooldown - time.delta_secs()).max(0.0);
    }
    let _ = &action_state; // keyed via prediction.rs
}

/// Tint the local player sprite orange on attack, red on being hit.
fn update_player_flash(
    action_state: Res<ActionState>,
    time: Res<Time>,
    mut q: Query<(&CharacterStats, &mut PlayerFlash, &mut Sprite), With<Player>>,
) {
    let Ok((stats, mut flash, mut sprite)) = q.single_mut() else { return };
    let dt = time.delta_secs();

    // Detect incoming damage
    if flash.prev_health > 0.0 && stats.health.current < flash.prev_health - 0.5 {
        flash.hurt = 0.25;
    }
    flash.prev_health = stats.health.current;

    // Detect attack press
    if action_state.just_pressed(GameAction::Primary) || action_state.just_pressed(GameAction::Secondary) {
        flash.attack = 0.15;
    }

    // Tick timers
    flash.attack = (flash.attack - dt).max(0.0);
    flash.hurt = (flash.hurt - dt).max(0.0);

    // Apply tint (hurt takes priority)
    sprite.color = if flash.hurt > 0.0 {
        let t = flash.hurt / 0.25;
        Color::srgb(1.0, 0.2 * (1.0 - t), 0.2 * (1.0 - t))
    } else if flash.attack > 0.0 {
        let t = flash.attack / 0.15;
        Color::srgb(1.0, 0.85 - 0.35 * t, 0.5 - 0.5 * t)
    } else {
        Color::WHITE
    };
}

/// Switch music mood based on recent combat activity.
fn update_combat_mood(
    time: Res<Time>,
    mut mood_timer: ResMut<CombatMoodTimer>,
    flash_q: Query<&PlayerFlash, With<Player>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    mood_timer.since_hit += dt;

    if let Ok(flash) = flash_q.single() {
        if flash.hurt > 0.1 || flash.attack > 0.05 {
            mood_timer.since_hit = 0.0;
        }
    }

    if mood_timer.since_hit < 0.1 {
        commands.trigger(ChangeMood(Mood::Combat));
    } else if mood_timer.since_hit > 8.0 {
        commands.trigger(ChangeMood(Mood::Exploration));
    }
}

// ─── Item registry ────────────────────────────────────────────────────────────

fn seed_item_registry(mut registry: ResMut<ItemRegistry>) {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => { error!("DB open failed: {e}"); return; }
    };
    if let Err(e) = db::init(&conn) {
        error!("DB init failed: {e}"); return;
    }
    match db::load_items(&conn) {
        Ok(rows) => {
            for (id, name, r, g, b) in rows {
                registry.register(id, name, Color::srgb(r, g, b));
            }
            info!("Loaded {} item types", registry.0.len());
        }
        Err(e) => error!("Failed to load items: {e}"),
    }
}
