use bevy::prelude::*;
use lightyear::prelude::*;

use crate::client::input::{ActionState, GameAction};
use crate::client::iso::{ISO_FORWARD, ISO_RIGHT};
use crate::client::player::{CombatState, Player};
use crate::net::{
    CombatIntentKind, CombatIntentMessage, MovementIntentMessage, ReliableChannel,
    UnreliableChannel,
};
use crate::server::sim::ROLL_COOLDOWN_SECS;

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Read local input, compute world-space movement direction using the fixed
/// isometric basis (no camera involved), and send a
/// [`MovementIntentMessage`] to the server every frame.
///
/// The axis values `[n.x, n.z]` are identical to what the 3-D camera code
/// used to send, so the server needs no changes.
fn send_movement_intent(
    action_state: Res<ActionState>,
    mut sender_query: Query<&mut MessageSender<MovementIntentMessage>, With<Client>>,
) {
    let Ok(mut sender) = sender_query.single_mut() else { return };

    let forward = ISO_FORWARD.normalize_or_zero();
    let right = ISO_RIGHT.normalize_or_zero();

    let raw = action_state.movement_axis();
    let world_dir = forward * raw.y + right * raw.x;

    let axis: [f32; 2] = if world_dir.length_squared() > 0.0 {
        let n = world_dir.normalize_or_zero();
        [n.x, n.z]
    } else {
        [0.0_f32, 0.0]
    };

    sender.send::<UnreliableChannel>(MovementIntentMessage {
        player_id: 0,
        axis,
        jump_pressed: action_state.just_pressed(GameAction::Jump),
        sprinting: action_state.pressed(GameAction::Sprint),
    });
}

/// Send combat intents when the player presses a combat action key.
fn send_combat_intent(
    action_state: Res<ActionState>,
    mut sender_query: Query<&mut MessageSender<CombatIntentMessage>, With<Client>>,
    mut combat_q: Query<&mut CombatState, With<Player>>,
) {
    let Ok(mut sender) = sender_query.single_mut() else { return };
    let Ok(mut state) = combat_q.single_mut() else { return };

    // Roll: local cooldown gates intent to avoid spamming the server.
    if action_state.just_pressed(GameAction::Roll) && state.roll_cooldown <= 0.0 {
        state.roll_cooldown = ROLL_COOLDOWN_SECS;
        sender.send::<ReliableChannel>(CombatIntentMessage {
            player_id: 0,
            kind: CombatIntentKind::Roll,
        });
    }

    let primaries: &[(GameAction, CombatIntentKind)] = &[
        (GameAction::Primary, CombatIntentKind::Primary),
        (GameAction::Secondary, CombatIntentKind::Secondary),
        (GameAction::Block, CombatIntentKind::Block),
    ];
    for (action, kind) in primaries {
        if action_state.just_pressed(*action) {
            sender.send::<ReliableChannel>(CombatIntentMessage {
                player_id: 0,
                kind: *kind,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct ClientPredictionPlugin;

impl Plugin for ClientPredictionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (send_movement_intent, send_combat_intent));
    }
}
