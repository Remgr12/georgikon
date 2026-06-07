use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::server::ClientOf;

use crate::common::combat::{apply_cost, can_perform};
use crate::common::stats::CharacterStats;
use crate::net::{
    CharacterId, CombatIntentKind, CombatIntentMessage, CombatStateMessage, MovementIntentMessage,
    PlayerPosition, PlayerSnapshotMessage, ReliableChannel, UnreliableChannel,
};
use crate::server::mobs::MeleeAttack;
use crate::server::player_state::{AuthoritativePlayerState, OwnedPlayer};
use crate::server::sim::{step_player_with_intent, ROLL_COOLDOWN_SECS};

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Drain movement intents from each connected client and advance the sim.
fn receive_movement_intents(
    mut conn_query: Query<
        (&OwnedPlayer, &mut MessageReceiver<MovementIntentMessage>),
        With<ClientOf>,
    >,
    mut player_query: Query<&mut AuthoritativePlayerState>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (owned, mut receiver) in conn_query.iter_mut() {
        for intent in receiver.receive() {
            if let Ok(mut state) = player_query.get_mut(owned.0) {
                step_player_with_intent(&mut state, &intent, dt);
            }
        }
    }
}

/// Validate combat intents server-side:
/// - enforce cooldowns
/// - enforce stat costs (stamina)
/// - apply authoritative state changes
fn receive_combat_intents(
    mut commands: Commands,
    mut conn_query: Query<
        (&OwnedPlayer, &mut MessageReceiver<CombatIntentMessage>),
        With<ClientOf>,
    >,
    mut player_query: Query<(&mut AuthoritativePlayerState, &mut CharacterStats, &CharacterId)>,
) {
    for (owned, mut receiver) in conn_query.iter_mut() {
        for intent in receiver.receive() {
            if let Ok((mut state, mut stats, char_id)) = player_query.get_mut(owned.0) {
                match intent.kind {
                    CombatIntentKind::Roll => {
                        if state.roll_cooldown <= 0.0 && can_perform(intent.kind, &stats) {
                            state.roll_cooldown = ROLL_COOLDOWN_SECS;
                            apply_cost(intent.kind, &mut stats);
                        }
                    }
                    other => {
                        if can_perform(other, &stats) {
                            apply_cost(other, &mut stats);
                            // Primary / Secondary swings resolve as melee against mobs.
                            if matches!(
                                other,
                                CombatIntentKind::Primary | CombatIntentKind::Secondary
                            ) {
                                commands.trigger(MeleeAttack {
                                    attacker_char: char_id.0,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Sync authoritative state → replicated `PlayerPosition`, then send snapshots.
fn send_snapshots(
    mut conn_query: Query<
        (
            &OwnedPlayer,
            &mut MessageSender<PlayerSnapshotMessage>,
            &mut MessageSender<CombatStateMessage>,
        ),
        With<ClientOf>,
    >,
    mut player_query: Query<(
        &AuthoritativePlayerState,
        &mut PlayerPosition,
        &CharacterStats,
    )>,
) {
    for (owned, mut snap_sender, mut combat_sender) in conn_query.iter_mut() {
        if let Ok((state, mut pos, stats)) = player_query.get_mut(owned.0) {
            // Update replicated position so all clients see this player's movement.
            pos.0 = state.position;

            // Send authoritative position snapshot to the owning client.
            snap_sender.send::<UnreliableChannel>(PlayerSnapshotMessage {
                tick: state.tick,
                position: state.position.into(),
                velocity_y: state.velocity_y,
            });

            // Send authoritative combat + stat state.
            combat_sender.send::<ReliableChannel>(CombatStateMessage {
                tick: state.tick,
                roll_cooldown: state.roll_cooldown,
                health: stats.health.current,
                max_health: stats.health.max,
                energy: stats.energy.current,
                max_energy: stats.energy.max,
                stamina: stats.stamina.current,
                max_stamina: stats.stamina.max,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct ServerAuthorityPlugin;

impl Plugin for ServerAuthorityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                receive_movement_intents,
                receive_combat_intents,
                send_snapshots,
            )
                .chain(),
        );
    }
}
