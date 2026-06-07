//! Leveling / progression: awards XP, applies level-ups, and pushes the
//! level/xp HUD to the owning client.

use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::MessageSender;

use crate::common::quest::ProgressionMessage;
use crate::common::stats::{CharacterStats, Experience};
use crate::net::ReliableChannel;
use crate::server::online::OnlinePlayers;
use crate::server::player_state::OwnerConn;

/// Fired to grant a character experience (by mobs, quests, …).
#[derive(Event)]
pub struct AwardXp {
    pub char_id: u64,
    pub amount: u64,
}

/// Health gained per level.
const HEALTH_PER_LEVEL: f32 = 20.0;

pub struct ProgressionServerPlugin;

impl Plugin for ProgressionServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_award_xp);
        app.add_systems(Update, send_progression_on_spawn);
    }
}

fn on_award_xp(
    trigger: On<AwardXp>,
    online: Res<OnlinePlayers>,
    mut players: Query<(&mut Experience, &mut CharacterStats)>,
    mut tx_q: Query<&mut MessageSender<ProgressionMessage>, With<ClientOf>>,
) {
    let ev = trigger.event();
    let Some(game) = online.game_of(ev.char_id) else {
        return;
    };
    let Ok((mut exp, mut stats)) = players.get_mut(game) else {
        return;
    };
    let gained = exp.add_xp(ev.amount);
    if gained > 0 {
        stats.health.max += HEALTH_PER_LEVEL * gained as f32;
        stats.health.restore_full();
        tracing::info!("Char {} reached level {}", ev.char_id, exp.level);
    }
    if let Some(conn) = online.conn_of(ev.char_id) {
        if let Ok(mut tx) = tx_q.get_mut(conn) {
            tx.send::<ReliableChannel>(ProgressionMessage {
                level: exp.level,
                xp: exp.xp,
                xp_to_next: exp.xp_to_next(),
            });
        }
    }
}

/// Send the initial progression state when a character entity spawns.
fn send_progression_on_spawn(
    q: Query<(&Experience, &OwnerConn), Added<Experience>>,
    mut tx_q: Query<&mut MessageSender<ProgressionMessage>, With<ClientOf>>,
) {
    for (exp, owner) in q.iter() {
        if let Ok(mut tx) = tx_q.get_mut(owner.0) {
            tx.send::<ReliableChannel>(ProgressionMessage {
                level: exp.level,
                xp: exp.xp,
                xp_to_next: exp.xp_to_next(),
            });
        }
    }
}
