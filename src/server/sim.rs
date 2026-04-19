use bevy::prelude::*;

use crate::common::stats::CharacterStats;
use crate::net::MovementIntentMessage;
use crate::server::player_state::AuthoritativePlayerState;

// ---------------------------------------------------------------------------
// Physics constants (match Settings defaults so client & server agree)
// ---------------------------------------------------------------------------

pub const GROUND_Y: f32 = 1.0;
pub const WALK_SPEED: f32 = 5.0;
pub const SPRINT_SPEED: f32 = 8.0;
pub const JUMP_FORCE: f32 = 7.0;
pub const GRAVITY: f32 = 20.0;
pub const ROLL_COOLDOWN_SECS: f32 = 0.6;

// ---------------------------------------------------------------------------
// Pure simulation step (no ECS, easily unit-tested)
// ---------------------------------------------------------------------------

/// Apply one movement intent to an `AuthoritativePlayerState` over `dt` seconds.
///
/// `intent.axis` is expected to be the world-space XZ direction already
/// rotated by the client camera (so the server doesn't need camera data).
pub fn step_player_with_intent(
    state: &mut AuthoritativePlayerState,
    intent: &MovementIntentMessage,
    dt: f32,
) {
    // Horizontal movement
    state.sprinting = intent.sprinting;
    let speed = if state.sprinting { SPRINT_SPEED } else { WALK_SPEED };
    let direction = Vec3::new(intent.axis[0], 0.0, intent.axis[1]);
    if direction.length_squared() > 0.0 {
        let dir = direction.normalize_or_zero();
        state.position += dir * speed * dt;
    }

    // Jump (only when grounded)
    if intent.jump_pressed {
        let grounded = state.position.y <= GROUND_Y + f32::EPSILON;
        if grounded {
            state.velocity_y = JUMP_FORCE;
        }
    }

    // Gravity + ground clamp
    state.velocity_y -= GRAVITY * dt;
    state.position.y += state.velocity_y * dt;
    if state.position.y < GROUND_Y {
        state.position.y = GROUND_Y;
        state.velocity_y = 0.0;
    }
}

// ---------------------------------------------------------------------------
// ECS systems
// ---------------------------------------------------------------------------

/// Tick cooldowns and advance the simulation tick counter each frame.
fn tick_cooldowns(mut query: Query<&mut AuthoritativePlayerState>, time: Res<Time>) {
    let dt = time.delta_secs();
    for mut state in query.iter_mut() {
        state.roll_cooldown = (state.roll_cooldown - dt).max(0.0);
        state.tick = state.tick.wrapping_add(1);
    }
}

/// Regenerate health/energy/stamina for all server-authoritative players.
fn regen_stats(mut query: Query<&mut CharacterStats>, time: Res<Time>) {
    let dt = time.delta_secs();
    for mut stats in query.iter_mut() {
        stats.tick(dt);
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct ServerSimPlugin;

impl Plugin for ServerSimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (tick_cooldowns, regen_stats));
    }
}
