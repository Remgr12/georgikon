//! Authoritative two-party trade.
//!
//! Character-based (consistent with guild/party): sessions are keyed by the two
//! `CharacterId`s, inventories are reached via [`OnlinePlayers`], and a tailored
//! [`TradeStateMessage`] is pushed to each party after every change. Phases
//! advance Mutate → Review → Complete/Declined; any offer change resets accepts.

use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{MessageReceiver, MessageSender};
use std::collections::HashMap;

use crate::common::inventory::Inventory;
use crate::net::{
    ReliableChannel, TradeAcceptMessage, TradeDeclineMessage, TradeOfferUpdateMessage,
    TradePhaseNet, TradeRequestNetMessage, TradeStateMessage,
};
use crate::server::online::{OnlinePlayers, PlayerLeft};

// ---------------------------------------------------------------------------
// Trade phase + session
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradePhase {
    Mutate,
    Review,
    Complete,
    Declined,
}

impl From<TradePhase> for TradePhaseNet {
    fn from(p: TradePhase) -> Self {
        match p {
            TradePhase::Mutate => TradePhaseNet::Mutate,
            TradePhase::Review => TradePhaseNet::Review,
            TradePhase::Complete => TradePhaseNet::Complete,
            TradePhase::Declined => TradePhaseNet::Declined,
        }
    }
}

pub type OfferSlot = (u32, u32);

#[derive(Debug, Clone)]
pub struct TradeSession {
    pub phase: TradePhase,
    pub char_a: u64,
    pub char_b: u64,
    pub offer_a: Vec<OfferSlot>,
    pub offer_b: Vec<OfferSlot>,
    pub accepted_a: bool,
    pub accepted_b: bool,
}

impl TradeSession {
    fn new(char_a: u64, char_b: u64) -> Self {
        Self {
            phase: TradePhase::Mutate,
            char_a,
            char_b,
            offer_a: Vec::new(),
            offer_b: Vec::new(),
            accepted_a: false,
            accepted_b: false,
        }
    }

    fn key(a: u64, b: u64) -> u64 {
        a.min(b).wrapping_mul(0x9e3779b97f4a7c15) ^ a.max(b)
    }

    fn is_a(&self, char_id: u64) -> bool {
        self.char_a == char_id
    }

    fn offer_of(&self, char_id: u64) -> &Vec<OfferSlot> {
        if self.is_a(char_id) {
            &self.offer_a
        } else {
            &self.offer_b
        }
    }

    fn offered_qty(&self, char_id: u64, item_id: u32) -> u32 {
        self.offer_of(char_id)
            .iter()
            .find(|(id, _)| *id == item_id)
            .map(|(_, q)| *q)
            .unwrap_or(0)
    }

    fn add_to_offer(&mut self, char_id: u64, item_id: u32, qty: u32) -> bool {
        if self.phase != TradePhase::Mutate || qty == 0 {
            return false;
        }
        let offer = if self.is_a(char_id) {
            &mut self.offer_a
        } else {
            &mut self.offer_b
        };
        if let Some(slot) = offer.iter_mut().find(|(id, _)| *id == item_id) {
            slot.1 = slot.1.saturating_add(qty);
        } else {
            offer.push((item_id, qty));
        }
        self.accepted_a = false;
        self.accepted_b = false;
        true
    }

    fn remove_from_offer(&mut self, char_id: u64, item_id: u32, qty: u32) -> bool {
        if self.phase != TradePhase::Mutate || qty == 0 {
            return false;
        }
        let offer = if self.is_a(char_id) {
            &mut self.offer_a
        } else {
            &mut self.offer_b
        };
        if let Some(pos) = offer.iter().position(|(id, _)| *id == item_id) {
            if offer[pos].1 <= qty {
                offer.remove(pos);
            } else {
                offer[pos].1 -= qty;
            }
            self.accepted_a = false;
            self.accepted_b = false;
            true
        } else {
            false
        }
    }

    fn accept(&mut self, char_id: u64) {
        if !matches!(self.phase, TradePhase::Mutate | TradePhase::Review) {
            return;
        }
        if self.is_a(char_id) {
            self.accepted_a = true;
        } else {
            self.accepted_b = true;
        }
        if self.accepted_a && self.accepted_b {
            self.phase = TradePhase::Complete;
        } else if self.phase == TradePhase::Mutate {
            self.phase = TradePhase::Review;
        }
    }
}

// ---------------------------------------------------------------------------
// Registry + outbox
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct TradeRegistry {
    sessions: HashMap<u64, TradeSession>,
    char_index: HashMap<u64, u64>,
}

impl TradeRegistry {
    fn open(&mut self, a: u64, b: u64) -> bool {
        if a == b || self.char_index.contains_key(&a) || self.char_index.contains_key(&b) {
            return false;
        }
        let key = TradeSession::key(a, b);
        self.sessions.insert(key, TradeSession::new(a, b));
        self.char_index.insert(a, key);
        self.char_index.insert(b, key);
        true
    }

    fn session_for(&mut self, char_id: u64) -> Option<&mut TradeSession> {
        let key = *self.char_index.get(&char_id)?;
        self.sessions.get_mut(&key)
    }

    fn close(&mut self, a: u64, b: u64) {
        let key = TradeSession::key(a, b);
        if self.sessions.remove(&key).is_some() {
            self.char_index.remove(&a);
            self.char_index.remove(&b);
        }
    }
}

#[derive(Resource, Default)]
struct TradeOutbox {
    msgs: Vec<(Entity, TradeStateMessage)>,
}

/// Queue a tailored state message for each party of `session`.
fn enqueue_state(session: &TradeSession, online: &OnlinePlayers, outbox: &mut TradeOutbox) {
    for &me in &[session.char_a, session.char_b] {
        let Some(conn) = online.conn_of(me) else {
            continue;
        };
        let is_a = session.is_a(me);
        let partner = if is_a { session.char_b } else { session.char_a };
        outbox.msgs.push((
            conn,
            TradeStateMessage {
                phase: session.phase.into(),
                your_offer: session.offer_of(me).clone(),
                their_offer: session.offer_of(partner).clone(),
                you_accepted: if is_a { session.accepted_a } else { session.accepted_b },
                they_accepted: if is_a { session.accepted_b } else { session.accepted_a },
                partner_name: online
                    .name_of(partner)
                    .map(str::to_string)
                    .unwrap_or_default(),
            },
        ));
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn handle_trade_requests(
    online: Res<OnlinePlayers>,
    mut registry: ResMut<TradeRegistry>,
    mut outbox: ResMut<TradeOutbox>,
    mut conn_q: Query<(Entity, &mut MessageReceiver<TradeRequestNetMessage>), With<ClientOf>>,
) {
    for (conn, mut receiver) in conn_q.iter_mut() {
        let Some(from) = online.char_of_conn(conn) else {
            continue;
        };
        for msg in receiver.receive() {
            let Some(to) = online.char_by_name(msg.to_name.trim()) else {
                continue;
            };
            if registry.open(from, to) {
                if let Some(session) = registry.session_for(from) {
                    enqueue_state(session, &online, &mut outbox);
                }
            }
        }
    }
}

fn handle_offer_updates(
    online: Res<OnlinePlayers>,
    mut registry: ResMut<TradeRegistry>,
    mut outbox: ResMut<TradeOutbox>,
    inv_q: Query<&Inventory>,
    mut conn_q: Query<(Entity, &mut MessageReceiver<TradeOfferUpdateMessage>), With<ClientOf>>,
) {
    for (conn, mut receiver) in conn_q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn) else {
            continue;
        };
        for msg in receiver.receive() {
            let have = online
                .game_of(char_id)
                .and_then(|g| inv_q.get(g).ok())
                .map(|inv| inv.total_quantity(msg.item_id))
                .unwrap_or(0);
            let Some(session) = registry.session_for(char_id) else {
                continue;
            };
            let changed = if msg.add {
                // Don't let a player offer more than they hold.
                let already = session.offered_qty(char_id, msg.item_id);
                let addable = have.saturating_sub(already);
                let qty = msg.quantity.min(addable);
                qty > 0 && session.add_to_offer(char_id, msg.item_id, qty)
            } else {
                session.remove_from_offer(char_id, msg.item_id, msg.quantity)
            };
            if changed {
                enqueue_state(session, &online, &mut outbox);
            }
        }
    }
}

fn handle_accepts(
    online: Res<OnlinePlayers>,
    mut registry: ResMut<TradeRegistry>,
    mut outbox: ResMut<TradeOutbox>,
    mut inv_q: Query<&mut Inventory>,
    mut conn_q: Query<(Entity, &mut MessageReceiver<TradeAcceptMessage>), With<ClientOf>>,
) {
    let mut finished: Vec<TradeSession> = Vec::new();
    for (conn, mut receiver) in conn_q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn) else {
            continue;
        };
        for _ in receiver.receive() {
            if let Some(session) = registry.session_for(char_id) {
                session.accept(char_id);
                enqueue_state(session, &online, &mut outbox);
                if session.phase == TradePhase::Complete {
                    finished.push(session.clone());
                }
            }
        }
    }
    for session in finished {
        execute_trade(&session, &online, &mut inv_q);
        registry.close(session.char_a, session.char_b);
    }
}

fn handle_declines(
    online: Res<OnlinePlayers>,
    mut registry: ResMut<TradeRegistry>,
    mut outbox: ResMut<TradeOutbox>,
    mut conn_q: Query<(Entity, &mut MessageReceiver<TradeDeclineMessage>), With<ClientOf>>,
) {
    let mut to_close: Vec<(u64, u64)> = Vec::new();
    for (conn, mut receiver) in conn_q.iter_mut() {
        let Some(char_id) = online.char_of_conn(conn) else {
            continue;
        };
        for _ in receiver.receive() {
            if let Some(session) = registry.session_for(char_id) {
                session.phase = TradePhase::Declined;
                enqueue_state(session, &online, &mut outbox);
                to_close.push((session.char_a, session.char_b));
            }
        }
    }
    for (a, b) in to_close {
        registry.close(a, b);
    }
}

/// Cancel any trade a disconnecting player was in.
fn on_trader_left(
    trigger: On<PlayerLeft>,
    online: Res<OnlinePlayers>,
    mut registry: ResMut<TradeRegistry>,
    mut outbox: ResMut<TradeOutbox>,
) {
    let char_id = trigger.event().0;
    if let Some(session) = registry.session_for(char_id) {
        session.phase = TradePhase::Declined;
        let (a, b) = (session.char_a, session.char_b);
        enqueue_state(session, &online, &mut outbox);
        registry.close(a, b);
    }
}

fn flush_trade_outbox(
    mut outbox: ResMut<TradeOutbox>,
    mut tx_q: Query<&mut MessageSender<TradeStateMessage>, With<ClientOf>>,
) {
    for (conn, msg) in std::mem::take(&mut outbox.msgs) {
        if let Ok(mut tx) = tx_q.get_mut(conn) {
            tx.send::<ReliableChannel>(msg);
        }
    }
}

/// Atomic inventory swap for a completed trade (validated first).
fn execute_trade(
    session: &TradeSession,
    online: &OnlinePlayers,
    inv_q: &mut Query<&mut Inventory>,
) {
    let (Some(game_a), Some(game_b)) =
        (online.game_of(session.char_a), online.game_of(session.char_b))
    else {
        return;
    };

    // Validate both sides still hold what they offered.
    let valid = inv_q
        .get(game_a)
        .map(|inv| session.offer_a.iter().all(|(id, q)| inv.total_quantity(*id) >= *q))
        .unwrap_or(false)
        && inv_q
            .get(game_b)
            .map(|inv| session.offer_b.iter().all(|(id, q)| inv.total_quantity(*id) >= *q))
            .unwrap_or(false);
    if !valid {
        tracing::warn!("Trade validation failed; aborting swap");
        return;
    }

    if let Ok(mut inv_a) = inv_q.get_mut(game_a) {
        for (id, q) in &session.offer_a {
            inv_a.remove_by_item_id(*id, *q);
        }
        for (id, q) in &session.offer_b {
            inv_a.add(*id, *q);
        }
    }
    if let Ok(mut inv_b) = inv_q.get_mut(game_b) {
        for (id, q) in &session.offer_b {
            inv_b.remove_by_item_id(*id, *q);
        }
        for (id, q) in &session.offer_a {
            inv_b.add(*id, *q);
        }
    }
    tracing::info!(a = session.char_a, b = session.char_b, "Trade completed");
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct TradePlugin;

impl Plugin for TradePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TradeRegistry>();
        app.init_resource::<TradeOutbox>();
        app.add_observer(on_trader_left);
        app.add_systems(
            Update,
            (
                handle_trade_requests,
                handle_offer_updates,
                handle_accepts,
                handle_declines,
                flush_trade_outbox,
            )
                .chain(),
        );
    }
}
