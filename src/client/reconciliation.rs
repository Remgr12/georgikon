use bevy::prelude::*;
use lightyear::prelude::*;

use crate::client::player::{CombatState, MovementState, Player};
use crate::common::stats::CharacterStats;
use crate::net::{CombatStateMessage, PlayerSnapshotMessage};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum position error (metres) before we snap to the server position.
const SNAP_THRESHOLD: f32 = 0.5;

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Receive authoritative position snapshots from the server and correct the
/// local player entity when drift exceeds the threshold.
///
/// Phase 1 uses a simple snap-on-threshold strategy.
fn reconcile_position(
    mut receiver_query: Query<&mut MessageReceiver<PlayerSnapshotMessage>, With<Client>>,
    mut player_query: Query<(&mut Transform, &mut MovementState), With<Player>>,
) {
    let Ok(mut receiver) = receiver_query.single_mut() else {
        return;
    };
    let Ok((mut transform, mut movement)) = player_query.single_mut() else {
        return;
    };

    // Keep only the highest-tick snapshot.
    let mut latest: Option<PlayerSnapshotMessage> = None;
    for snap in receiver.receive() {
        let is_newer = latest.as_ref().map_or(true, |prev| snap.tick > prev.tick);
        if is_newer {
            latest = Some(snap);
        }
    }

    if let Some(snap) = latest {
        let server_pos = Vec3::from(snap.position);
        let drift = (server_pos - transform.translation).length();
        if drift > SNAP_THRESHOLD {
            tracing::debug!(
                drift,
                tick = snap.tick,
                "Reconciling: snapping to server position"
            );
            transform.translation = server_pos;
            movement.velocity_y = snap.velocity_y;
        }
    }
}

/// Apply server-authoritative combat cooldowns and character stats.
///
/// Cooldown correction is one-directional (never reduces a locally-predicted
/// cooldown that's already higher, to avoid reordered-packet glitches).
/// Stats are authoritative and overwrite the client estimate directly.
fn reconcile_combat_state(
    mut receiver_query: Query<&mut MessageReceiver<CombatStateMessage>, With<Client>>,
    mut player_query: Query<(&mut CombatState, &mut CharacterStats), With<Player>>,
) {
    let Ok(mut receiver) = receiver_query.single_mut() else {
        return;
    };
    let Ok((mut combat, mut stats)) = player_query.single_mut() else {
        return;
    };

    let mut latest: Option<CombatStateMessage> = None;
    for msg in receiver.receive() {
        let is_newer = latest.as_ref().map_or(true, |prev| msg.tick > prev.tick);
        if is_newer {
            latest = Some(msg);
        }
    }

    if let Some(msg) = latest {
        // Cooldown: only correct upward to avoid visual glitches.
        if msg.roll_cooldown > combat.roll_cooldown {
            combat.roll_cooldown = msg.roll_cooldown;
        }
        // Stats: server is authoritative; overwrite client estimates.
        stats.health.current = msg.health;
        stats.health.max = msg.max_health;
        stats.energy.current = msg.energy;
        stats.energy.max = msg.max_energy;
        stats.stamina.current = msg.stamina;
        stats.stamina.max = msg.max_stamina;
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct ClientReconciliationPlugin;

impl Plugin for ClientReconciliationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (reconcile_position, reconcile_combat_state));
    }
}
