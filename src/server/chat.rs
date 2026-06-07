//! Server-authoritative chat routing.
//!
//! Every incoming [`ChatNetMessage`] is validated, rate-limited, and delivered
//! only to the appropriate audience (proximity / party / guild / whisper /
//! world) — never blindly broadcast to everyone.

use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{MessageReceiver, MessageSender};
use std::collections::{HashMap, VecDeque};

use crate::common::social::{ChatBroadcastMessage, ChatChannel, ChatNetMessage};
use crate::common::zone::{Zone, ZoneId};
use crate::net::{CharacterId, PlayerPosition, ReliableChannel};
use crate::server::guild::GuildRegistry;
use crate::server::online::OnlinePlayers;
use crate::server::party::PartyRegistry;

/// Local (proximity) chat radius in metres.
const LOCAL_RADIUS: f32 = 30.0;

/// Maximum UTF-8 character count for a chat message body.
const MAX_BODY_LEN: usize = 200;

/// Sliding window for rate limiting (seconds).
const RATE_WINDOW_SECS: f64 = 5.0;

/// Maximum messages allowed per player within `RATE_WINDOW_SECS`.
const RATE_MAX_MSGS: usize = 5;

#[derive(Resource, Default)]
struct ChatRateLimiter {
    /// char_id → timestamps (seconds) of recent sends, oldest first.
    timestamps: HashMap<u64, VecDeque<f64>>,
}

impl ChatRateLimiter {
    fn check(&mut self, char_id: u64, now: f64) -> bool {
        let queue = self.timestamps.entry(char_id).or_default();
        while queue.front().is_some_and(|&t| now - t > RATE_WINDOW_SECS) {
            queue.pop_front();
        }
        if queue.len() >= RATE_MAX_MSGS {
            return false;
        }
        queue.push_back(now);
        true
    }
}

pub struct ChatServerPlugin;

impl Plugin for ChatServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatRateLimiter>();
        app.add_systems(Update, route_chat);
    }
}

/// Validate, rate-limit, and *route* incoming chat to the correct audience.
fn route_chat(
    time: Res<Time>,
    mut rate_limiter: ResMut<ChatRateLimiter>,
    online: Res<OnlinePlayers>,
    parties: Res<PartyRegistry>,
    guilds: Res<GuildRegistry>,
    mut receiver_query: Query<(Entity, &mut MessageReceiver<ChatNetMessage>), With<ClientOf>>,
    players: Query<(&CharacterId, &PlayerPosition, &Zone)>,
    mut sender_query: Query<&mut MessageSender<ChatBroadcastMessage>, With<ClientOf>>,
) {
    let now = time.elapsed_secs_f64();

    // Snapshot every player's position/zone for proximity routing.
    let mut pos_zone: HashMap<u64, (Vec3, ZoneId)> = HashMap::new();
    for (cid, pos, zone) in players.iter() {
        pos_zone.insert(cid.0, (pos.0, zone.0));
    }

    // (recipient connection entity, message) pairs to deliver.
    let mut outgoing: Vec<(Entity, ChatBroadcastMessage)> = Vec::new();

    for (conn, mut receiver) in receiver_query.iter_mut() {
        let Some(sender_char) = online.char_of_conn(conn) else {
            continue;
        };

        for msg in receiver.receive() {
            let channel = msg.channel;
            let body: String = msg.body.trim().chars().take(MAX_BODY_LEN).collect();
            if body.is_empty() {
                continue;
            }
            if !rate_limiter.check(sender_char, now) {
                continue;
            }

            let sender_name = online
                .name_of(sender_char)
                .map(str::to_string)
                .unwrap_or_else(|| format!("Char#{sender_char}"));

            // Resolve the recipient character set for this channel.
            let recipients: Vec<u64> = match channel {
                ChatChannel::Local => {
                    let Some((origin, zone)) = pos_zone.get(&sender_char).copied() else {
                        continue;
                    };
                    online
                        .iter()
                        .map(|(cid, _)| cid)
                        .filter(|cid| {
                            pos_zone.get(cid).is_some_and(|(p, z)| {
                                *z == zone && p.distance(origin) <= LOCAL_RADIUS
                            })
                        })
                        .collect()
                }
                ChatChannel::Party => {
                    let mut m = parties.party_members(sender_char);
                    if m.is_empty() {
                        m.push(sender_char);
                    }
                    m
                }
                ChatChannel::Guild => {
                    let gids = guilds.guild_ids_of(sender_char);
                    // guild_id == 0 selects the first guild; u64::MAX the second
                    // (the `/g1` and `/g2` client commands); otherwise an explicit id.
                    let gid = if msg.guild_id == 0 {
                        gids.first().copied()
                    } else if msg.guild_id == u64::MAX {
                        gids.get(1).copied()
                    } else if gids.contains(&msg.guild_id) {
                        Some(msg.guild_id)
                    } else {
                        None
                    };
                    match gid {
                        Some(g) => guilds
                            .member_ids(g)
                            .into_iter()
                            .filter(|c| online.is_online(*c))
                            .collect(),
                        None => continue,
                    }
                }
                ChatChannel::Trade | ChatChannel::World => {
                    online.iter().map(|(cid, _)| cid).collect()
                }
                ChatChannel::Whisper => {
                    let Some(target) = online.char_by_name(&msg.target_name) else {
                        if let Some(conn) = online.conn_of(sender_char) {
                            outgoing.push((
                                conn,
                                ChatBroadcastMessage {
                                    sender_name: "System".into(),
                                    channel: ChatChannel::Whisper,
                                    body: format!("{} is not online.", msg.target_name),
                                    target_name: String::new(),
                                },
                            ));
                        }
                        continue;
                    };
                    // Deliver to target and echo to sender.
                    vec![target, sender_char]
                }
            };

            let target_name = if channel == ChatChannel::Whisper {
                online
                    .char_by_name(&msg.target_name)
                    .and_then(|c| online.name_of(c).map(str::to_string))
                    .unwrap_or_else(|| msg.target_name.clone())
            } else {
                String::new()
            };

            let broadcast = ChatBroadcastMessage {
                sender_name,
                channel,
                body,
                target_name,
            };

            for cid in recipients {
                if let Some(conn) = online.conn_of(cid) {
                    outgoing.push((conn, broadcast.clone()));
                }
            }
        }
    }

    for (conn, msg) in outgoing {
        if let Ok(mut sender) = sender_query.get_mut(conn) {
            sender.send::<ReliableChannel>(msg);
        }
    }
}
