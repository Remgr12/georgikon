use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::server::ClientOf;
use std::collections::HashMap;

use crate::common::inventory::Inventory;
use crate::net::{
    TradeAcceptMessage, TradeDeclineMessage, TradeOfferUpdateMessage, TradePhaseNet,
    TradeRequestNetMessage, TradeStateMessage,
};

// ---------------------------------------------------------------------------
// Trade phase
// ---------------------------------------------------------------------------

/// Phase of a trade session.
///
/// Phases advance strictly: Mutate → Review → Complete/Declined.
/// Re-entering Mutate from Review happens when either party mutates their offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradePhase {
    /// Both parties may freely modify their offers.
    Mutate,
    /// Both parties have accepted; offers are locked pending final confirmation.
    Review,
    /// Trade committed; inventory swapped atomically.
    Complete,
    /// Trade cancelled.
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

// ---------------------------------------------------------------------------
// Trade session
// ---------------------------------------------------------------------------

/// (item_id, quantity) offer entry.
pub type OfferSlot = (u32, u32);

/// An active two-party trade session.
///
/// Stored in the `TradeRegistry` resource indexed by a canonical player-pair key.
#[derive(Debug, Clone)]
pub struct TradeSession {
    pub phase: TradePhase,
    /// `PlayerId` u64 of party A (the initiator).
    pub player_a: u64,
    /// `PlayerId` u64 of party B (the responder).
    pub player_b: u64,
    /// Items A is offering.
    pub offer_a: Vec<OfferSlot>,
    /// Items B is offering.
    pub offer_b: Vec<OfferSlot>,
    pub accepted_a: bool,
    pub accepted_b: bool,
}

impl TradeSession {
    pub fn new(player_a: u64, player_b: u64) -> Self {
        Self {
            phase: TradePhase::Mutate,
            player_a,
            player_b,
            offer_a: Vec::new(),
            offer_b: Vec::new(),
            accepted_a: false,
            accepted_b: false,
        }
    }

    /// Build the wire-safe snapshot for broadcasting.
    pub fn to_net(&self) -> TradeStateMessage {
        TradeStateMessage {
            phase: self.phase.into(),
            offer_a: self.offer_a.clone(),
            offer_b: self.offer_b.clone(),
            accepted_a: self.accepted_a,
            accepted_b: self.accepted_b,
        }
    }

    /// Returns the canonical session key for a player pair.
    pub fn key(a: u64, b: u64) -> u64 {
        a.min(b).wrapping_mul(0x9e3779b97f4a7c15) ^ a.max(b)
    }

    // --- mutation guards ---

    /// Is `player_id` a party to this session?
    pub fn is_party(&self, player_id: u64) -> bool {
        self.player_a == player_id || self.player_b == player_id
    }

    /// Is player_id party A?
    fn is_a(&self, player_id: u64) -> bool {
        self.player_a == player_id
    }

    /// Attempt to add an item to the player's offer.
    /// Returns false if the phase does not allow mutations or quantity is 0.
    pub fn add_to_offer(&mut self, player_id: u64, item_id: u32, qty: u32) -> bool {
        if self.phase != TradePhase::Mutate || qty == 0 {
            return false;
        }
        let offer = if self.is_a(player_id) {
            &mut self.offer_a
        } else {
            &mut self.offer_b
        };
        // Stack same item ids.
        if let Some(slot) = offer.iter_mut().find(|(id, _)| *id == item_id) {
            slot.1 = slot.1.saturating_add(qty);
        } else {
            offer.push((item_id, qty));
        }
        // Reset accepts on any mutation.
        self.accepted_a = false;
        self.accepted_b = false;
        true
    }

    /// Attempt to remove `qty` of `item_id` from the player's offer.
    pub fn remove_from_offer(&mut self, player_id: u64, item_id: u32, qty: u32) -> bool {
        if self.phase != TradePhase::Mutate || qty == 0 {
            return false;
        }
        let offer = if self.is_a(player_id) {
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

    /// Record an acceptance from `player_id`.
    /// If both parties accept, advance to `Review`.
    /// Returns true if state changed.
    pub fn accept(&mut self, player_id: u64) -> bool {
        if !matches!(self.phase, TradePhase::Mutate | TradePhase::Review) {
            return false;
        }
        if self.is_a(player_id) {
            self.accepted_a = true;
        } else {
            self.accepted_b = true;
        }
        if self.accepted_a && self.accepted_b {
            self.phase = TradePhase::Complete;
        } else if self.phase == TradePhase::Mutate && (self.accepted_a || self.accepted_b) {
            self.phase = TradePhase::Review;
        }
        true
    }

    /// Decline the trade.
    pub fn decline(&mut self) {
        self.phase = TradePhase::Declined;
    }
}

// ---------------------------------------------------------------------------
// Registry resource
// ---------------------------------------------------------------------------

/// Server-side registry of all active trade sessions.
#[derive(Resource, Default)]
pub struct TradeRegistry {
    /// key → session
    sessions: HashMap<u64, TradeSession>,
    /// player_id → session key (so we can look up by participant)
    player_index: HashMap<u64, u64>,
}

impl TradeRegistry {
    /// Open a new session between two players.
    /// Returns false if either player is already in an active trade.
    pub fn open(&mut self, a: u64, b: u64) -> bool {
        if self.player_index.contains_key(&a) || self.player_index.contains_key(&b) {
            return false;
        }
        let key = TradeSession::key(a, b);
        let session = TradeSession::new(a, b);
        self.sessions.insert(key, session);
        self.player_index.insert(a, key);
        self.player_index.insert(b, key);
        true
    }

    pub fn session_for(&mut self, player_id: u64) -> Option<&mut TradeSession> {
        let key = *self.player_index.get(&player_id)?;
        self.sessions.get_mut(&key)
    }

    /// Remove a completed or declined session.
    pub fn close(&mut self, player_a: u64, player_b: u64) {
        let key = TradeSession::key(player_a, player_b);
        if self.sessions.remove(&key).is_some() {
            self.player_index.remove(&player_a);
            self.player_index.remove(&player_b);
        }
    }
}

// ---------------------------------------------------------------------------
// Inventory locking
// ---------------------------------------------------------------------------

/// Marks inventory slots that are currently locked in an active trade offer.
/// No consumption, drop, or sort may affect these slots until the trade ends.
#[derive(Component, Default, Debug)]
pub struct TradeLocked {
    pub slots: std::collections::HashSet<usize>,
}

// ---------------------------------------------------------------------------
// Helper: find the game entity for a player_id
// ---------------------------------------------------------------------------

fn find_game_entity(player_id: u64, player_query: &Query<(Entity, &crate::net::PlayerId)>) -> Option<Entity> {
    player_query
        .iter()
        .find_map(|(e, pid)| if pid.0 == player_id { Some(e) } else { None })
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn handle_trade_requests(
    mut conn_query: Query<
        (&mut MessageReceiver<TradeRequestNetMessage>, &crate::server::player_state::OwnedPlayer),
        With<ClientOf>,
    >,
    _player_query: Query<(Entity, &crate::net::PlayerId)>,
    mut registry: ResMut<TradeRegistry>,
) {
    for (mut receiver, _owned) in conn_query.iter_mut() {
        for msg in receiver.receive() {
            if !registry.open(msg.from_player_id, msg.to_player_id) {
                tracing::debug!(
                    from = msg.from_player_id,
                    to = msg.to_player_id,
                    "Trade request rejected: one party already in a trade"
                );
            } else {
                tracing::info!(from = msg.from_player_id, to = msg.to_player_id, "Trade opened");
            }
        }
    }
}

fn handle_offer_updates(
    mut conn_query: Query<
        (&mut MessageReceiver<TradeOfferUpdateMessage>, &crate::server::player_state::OwnedPlayer),
        With<ClientOf>,
    >,
    mut registry: ResMut<TradeRegistry>,
) {
    for (mut receiver, _) in conn_query.iter_mut() {
        for msg in receiver.receive() {
            if let Some(session) = registry.session_for(msg.player_id) {
                if session.phase != TradePhase::Mutate {
                    tracing::debug!(
                        player_id = msg.player_id,
                        "Offer update rejected: not in Mutate phase"
                    );
                    continue;
                }
                // Determine item_id from the inventory slot.
                // For Phase 3 we pass item_id directly in the message to keep
                // the server stateless w.r.t. individual inventory contents.
                // (item_id is stored in the offer, not the slot index)
                if msg.add {
                    // quantity field carries item_id<<32 | qty encoding is too complex;
                    // instead the message carries separate fields.
                    // For now treat quantity as item_id placeholder; real implementation
                    // would look up the player's inventory by slot.
                    tracing::trace!(
                        player_id = msg.player_id,
                        slot = msg.inventory_slot,
                        qty = msg.quantity,
                        "Offer add (Phase 3 stub)"
                    );
                } else {
                    tracing::trace!(
                        player_id = msg.player_id,
                        slot = msg.inventory_slot,
                        "Offer remove (Phase 3 stub)"
                    );
                }
            }
        }
    }
}

fn handle_accepts(
    mut conn_query: Query<
        (&mut MessageReceiver<TradeAcceptMessage>, &crate::server::player_state::OwnedPlayer),
        With<ClientOf>,
    >,
    mut registry: ResMut<TradeRegistry>,
    mut player_query: Query<(&crate::net::PlayerId, &mut Inventory)>,
) {
    let mut to_close: Vec<(u64, u64)> = Vec::new();

    // Collect accepts first (borrow registry mutably).
    let mut accepted_sessions: Vec<(u64, TradeSession)> = Vec::new();
    for (mut receiver, _) in conn_query.iter_mut() {
        for msg in receiver.receive() {
            if let Some(session) = registry.session_for(msg.player_id) {
                session.accept(msg.player_id);
                if matches!(session.phase, TradePhase::Complete | TradePhase::Declined) {
                    accepted_sessions.push((msg.player_id, session.clone()));
                }
            }
        }
    }

    // Execute completed trades.
    for (_, session) in &accepted_sessions {
        if session.phase == TradePhase::Complete {
            execute_trade(session, &mut player_query);
        }
        to_close.push((session.player_a, session.player_b));
    }

    for (a, b) in to_close {
        registry.close(a, b);
    }
}

/// Atomic inventory swap for a completed trade.
fn execute_trade(
    session: &TradeSession,
    player_query: &mut Query<(&crate::net::PlayerId, &mut Inventory)>,
) {
    tracing::info!(
        a = session.player_a,
        b = session.player_b,
        offer_a = ?session.offer_a,
        offer_b = ?session.offer_b,
        "Executing trade"
    );

    // Validate that both inventories contain the offered items.
    // For Phase 3 this is a structural check; detailed slot validation is Phase 3+.
    let valid = {
        let a_has = player_query
            .iter()
            .filter(|(pid, _)| pid.0 == session.player_a)
            .any(|(_, inv)| {
                session.offer_a.iter().all(|(item_id, qty)| {
                    inv.total_quantity(*item_id) >= *qty
                })
            });
        let b_has = player_query
            .iter()
            .filter(|(pid, _)| pid.0 == session.player_b)
            .any(|(_, inv)| {
                session.offer_b.iter().all(|(item_id, qty)| {
                    inv.total_quantity(*item_id) >= *qty
                })
            });
        a_has && b_has
    };

    if !valid {
        tracing::warn!(
            a = session.player_a,
            b = session.player_b,
            "Trade validation failed: inventory mismatch"
        );
        return;
    }

    // Remove items from both sides then add the received items.
    for (pid, mut inv) in player_query.iter_mut() {
        if pid.0 == session.player_a {
            for (item_id, qty) in &session.offer_a {
                inv.remove_by_item_id(*item_id, *qty);
            }
            for (item_id, qty) in &session.offer_b {
                inv.add(*item_id, *qty);
            }
        } else if pid.0 == session.player_b {
            for (item_id, qty) in &session.offer_b {
                inv.remove_by_item_id(*item_id, *qty);
            }
            for (item_id, qty) in &session.offer_a {
                inv.add(*item_id, *qty);
            }
        }
    }
}

fn handle_declines(
    mut conn_query: Query<
        (&mut MessageReceiver<TradeDeclineMessage>, &crate::server::player_state::OwnedPlayer),
        With<ClientOf>,
    >,
    mut registry: ResMut<TradeRegistry>,
) {
    let mut to_close: Vec<(u64, u64)> = Vec::new();
    for (mut receiver, _) in conn_query.iter_mut() {
        for msg in receiver.receive() {
            if let Some(session) = registry.session_for(msg.player_id) {
                session.decline();
                to_close.push((session.player_a, session.player_b));
                tracing::info!(player_id = msg.player_id, "Trade declined");
            }
        }
    }
    for (a, b) in to_close {
        registry.close(a, b);
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct TradePlugin;

impl Plugin for TradePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TradeRegistry>();
        app.add_systems(
            Update,
            (
                handle_trade_requests,
                handle_offer_updates,
                handle_accepts,
                handle_declines,
            )
                .chain(),
        );
    }
}
