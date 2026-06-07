//! Authoritative party system — runtime-only groups of up to
//! [`MAX_PARTY_SIZE`] players, used for the Party chat channel and party HUD.

use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{MessageReceiver, MessageSender};
use std::collections::{HashMap, HashSet};

use crate::common::party::*;
use crate::common::stats::{CharacterStats, Experience};
use crate::net::ReliableChannel;
use crate::server::online::{OnlinePlayers, PlayerLeft};

struct Party {
    leader: u64,
    members: Vec<u64>,
}

#[derive(Resource, Default)]
pub struct PartyRegistry {
    parties: HashMap<u64, Party>,
    by_char: HashMap<u64, u64>,
    /// invited char → (party_id, inviter_char)
    pending: HashMap<u64, (u64, u64)>,
    next_id: u64,
}

impl PartyRegistry {
    /// Other party members of `char_id` (excludes `char_id` itself).
    pub fn party_members(&self, char_id: u64) -> Vec<u64> {
        self.by_char
            .get(&char_id)
            .and_then(|pid| self.parties.get(pid))
            .map(|p| p.members.clone())
            .unwrap_or_default()
    }

    fn ensure_party(&mut self, leader: u64) -> u64 {
        if let Some(pid) = self.by_char.get(&leader) {
            return *pid;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.parties.insert(
            id,
            Party {
                leader,
                members: vec![leader],
            },
        );
        self.by_char.insert(leader, id);
        id
    }

    /// Remove a character from their party; returns the set of chars whose state
    /// must refresh (remaining members + the leaver). Disbands parties of < 2.
    fn remove_char(&mut self, char_id: u64) -> HashSet<u64> {
        let mut dirty = HashSet::new();
        let Some(pid) = self.by_char.remove(&char_id) else {
            return dirty;
        };
        dirty.insert(char_id);
        let Some(party) = self.parties.get_mut(&pid) else {
            return dirty;
        };
        party.members.retain(|c| *c != char_id);
        if party.leader == char_id {
            party.leader = party.members.first().copied().unwrap_or(0);
        }
        for c in &party.members {
            dirty.insert(*c);
        }
        if party.members.len() < 2 {
            for c in party.members.clone() {
                self.by_char.remove(&c);
                dirty.insert(c);
            }
            self.parties.remove(&pid);
        }
        dirty
    }
}

#[derive(Resource, Default)]
struct PartyOutbox {
    pushes: Vec<(u64, PartyInvitePushMessage)>,
    dirty: HashSet<u64>,
}

pub struct PartyServerPlugin;

impl Plugin for PartyServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartyRegistry>();
        app.init_resource::<PartyOutbox>();
        app.add_systems(Update, (handle_party_ops, flush_party_outbox).chain());
        app.add_observer(on_party_member_disconnect);
    }
}

fn handle_party_ops(
    mut reg: ResMut<PartyRegistry>,
    mut outbox: ResMut<PartyOutbox>,
    online: Res<OnlinePlayers>,
    mut q: Query<
        (
            Entity,
            &mut MessageReceiver<PartyInviteMessage>,
            &mut MessageReceiver<PartyInviteResponseMessage>,
            &mut MessageReceiver<LeavePartyMessage>,
            &mut MessageReceiver<KickPartyMemberMessage>,
            &mut MessageReceiver<PromotePartyLeaderMessage>,
        ),
        With<ClientOf>,
    >,
) {
    for (conn_e, mut inv_rx, mut resp_rx, mut leave_rx, mut kick_rx, mut promote_rx) in q.iter_mut()
    {
        let Some(char_id) = online.char_of_conn(conn_e) else {
            continue;
        };

        for msg in inv_rx.receive() {
            let Some(target) = online.char_by_name(&msg.target_name) else {
                continue;
            };
            if target == char_id || reg.by_char.contains_key(&target) {
                continue;
            }
            let pid = reg.ensure_party(char_id);
            if let Some(p) = reg.parties.get(&pid) {
                if p.members.len() >= MAX_PARTY_SIZE {
                    continue;
                }
            }
            reg.pending.insert(target, (pid, char_id));
            outbox.pushes.push((
                target,
                PartyInvitePushMessage {
                    from_char_id: char_id,
                    from_name: online
                        .name_of(char_id)
                        .map(str::to_string)
                        .unwrap_or_default(),
                },
            ));
            outbox.dirty.insert(char_id);
        }

        for msg in resp_rx.receive() {
            let Some((pid, _inviter)) = reg.pending.remove(&char_id) else {
                continue;
            };
            if !msg.accept {
                continue;
            }
            if reg.by_char.contains_key(&char_id) {
                continue;
            }
            let ok = match reg.parties.get_mut(&pid) {
                Some(p) if p.members.len() < MAX_PARTY_SIZE => {
                    p.members.push(char_id);
                    true
                }
                _ => false,
            };
            if ok {
                reg.by_char.insert(char_id, pid);
                if let Some(p) = reg.parties.get(&pid) {
                    for c in p.members.clone() {
                        outbox.dirty.insert(c);
                    }
                }
            }
        }

        for _ in leave_rx.receive() {
            for c in reg.remove_char(char_id) {
                outbox.dirty.insert(c);
            }
        }

        for msg in kick_rx.receive() {
            let is_leader = reg
                .by_char
                .get(&char_id)
                .and_then(|pid| reg.parties.get(pid))
                .map(|p| p.leader == char_id)
                .unwrap_or(false);
            if is_leader {
                for c in reg.remove_char(msg.target_char_id) {
                    outbox.dirty.insert(c);
                }
            }
        }

        for msg in promote_rx.receive() {
            if let Some(pid) = reg.by_char.get(&char_id).copied() {
                if let Some(p) = reg.parties.get_mut(&pid) {
                    if p.leader == char_id && p.members.contains(&msg.target_char_id) {
                        p.leader = msg.target_char_id;
                        for c in p.members.clone() {
                            outbox.dirty.insert(c);
                        }
                    }
                }
            }
        }
    }
}

fn flush_party_outbox(
    mut outbox: ResMut<PartyOutbox>,
    reg: Res<PartyRegistry>,
    online: Res<OnlinePlayers>,
    member_stats: Query<(&CharacterStats, &Experience)>,
    mut push_tx: Query<&mut MessageSender<PartyInvitePushMessage>, With<ClientOf>>,
    mut state_tx: Query<&mut MessageSender<PartyStateMessage>, With<ClientOf>>,
) {
    let pushes = std::mem::take(&mut outbox.pushes);
    for (char_id, msg) in pushes {
        if let Some(conn) = online.conn_of(char_id) {
            if let Ok(mut tx) = push_tx.get_mut(conn) {
                tx.send::<ReliableChannel>(msg);
            }
        }
    }

    let dirty = std::mem::take(&mut outbox.dirty);
    for char_id in dirty {
        let Some(conn) = online.conn_of(char_id) else {
            continue;
        };
        let state = build_state(char_id, &reg, &online, &member_stats);
        if let Ok(mut tx) = state_tx.get_mut(conn) {
            tx.send::<ReliableChannel>(state);
        }
    }
}

fn build_state(
    char_id: u64,
    reg: &PartyRegistry,
    online: &OnlinePlayers,
    member_stats: &Query<(&CharacterStats, &Experience)>,
) -> PartyStateMessage {
    let Some(pid) = reg.by_char.get(&char_id) else {
        return PartyStateMessage {
            in_party: false,
            leader_char_id: 0,
            members: Vec::new(),
        };
    };
    let Some(party) = reg.parties.get(pid) else {
        return PartyStateMessage {
            in_party: false,
            leader_char_id: 0,
            members: Vec::new(),
        };
    };
    let members = party
        .members
        .iter()
        .map(|cid| {
            let (health, max_health, level) = online
                .game_of(*cid)
                .and_then(|g| member_stats.get(g).ok())
                .map(|(s, e)| (s.health.current, s.health.max, e.level))
                .unwrap_or((0.0, 1.0, 1));
            PartyMemberInfo {
                char_id: *cid,
                name: online.name_of(*cid).map(str::to_string).unwrap_or_default(),
                level,
                health,
                max_health,
            }
        })
        .collect();
    PartyStateMessage {
        in_party: true,
        leader_char_id: party.leader,
        members,
    }
}

/// Clean a disconnecting player out of their party.
fn on_party_member_disconnect(
    trigger: On<PlayerLeft>,
    mut reg: ResMut<PartyRegistry>,
    mut outbox: ResMut<PartyOutbox>,
) {
    let char_id = trigger.event().0;
    for c in reg.remove_char(char_id) {
        outbox.dirty.insert(c);
    }
    reg.pending.remove(&char_id);
}
