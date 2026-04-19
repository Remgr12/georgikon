use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::server::ClientOf;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;

use crate::common::social::{ChatBroadcastMessage, ChatChannel, ChatNetMessage};
use crate::net::ReliableChannel;

// ---------------------------------------------------------------------------
// Validation constants
// ---------------------------------------------------------------------------

/// Maximum UTF-8 character count for a chat message body.
const MAX_BODY_LEN: usize = 200;

/// Sliding window for rate limiting (seconds).
const RATE_WINDOW_SECS: f64 = 5.0;

/// Maximum messages allowed per player within `RATE_WINDOW_SECS`.
const RATE_MAX_MSGS: usize = 5;

// ---------------------------------------------------------------------------
// Rate limiter
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct ChatRateLimiter {
    /// player_id → timestamps (seconds) of recent sends, oldest first.
    timestamps: HashMap<u64, VecDeque<f64>>,
}

impl ChatRateLimiter {
    /// Returns `true` and records the send if the player is within the rate limit.
    fn check(&mut self, player_id: u64, now: f64) -> bool {
        let queue = self.timestamps.entry(player_id).or_default();
        // Evict timestamps that have left the window.
        while queue.front().map_or(false, |&t| now - t > RATE_WINDOW_SECS) {
            queue.pop_front();
        }
        if queue.len() >= RATE_MAX_MSGS {
            return false;
        }
        queue.push_back(now);
        true
    }
}

// ---------------------------------------------------------------------------
// Zulip relay resources (optional)
// ---------------------------------------------------------------------------

/// Sends outgoing chat lines to the Zulip background thread.
#[derive(Resource)]
struct ZulipOutSender(Mutex<Sender<String>>);

/// Receives incoming Zulip lines to broadcast back to all clients.
#[derive(Resource)]
struct ZulipInReceiver(Mutex<Receiver<String>>);

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn route_chat(
    time: Res<Time>,
    mut rate_limiter: ResMut<ChatRateLimiter>,
    mut receiver_query: Query<
        (&mut MessageReceiver<ChatNetMessage>, &crate::server::player_state::OwnedPlayer),
        With<ClientOf>,
    >,
    // Read PlayerId from game entities so we can build the sender name.
    player_query: Query<(&crate::net::PlayerId, Entity)>,
    mut sender_query: Query<&mut MessageSender<ChatBroadcastMessage>, With<ClientOf>>,
    zulip_out: Option<Res<ZulipOutSender>>,
) {
    let now = time.elapsed_secs_f64();

    // Collect validated messages before borrowing sender_query.
    let mut outgoing: Vec<ChatBroadcastMessage> = Vec::new();

    for (mut receiver, owned_player) in receiver_query.iter_mut() {
        // Resolve the player_id for the game entity linked to this connection.
        let player_id = player_query
            .iter()
            .find_map(|(pid, e)| if e == owned_player.0 { Some(pid.0) } else { None })
            .unwrap_or(0);

        for msg in receiver.receive() {
            // --- validate channel ---
            // All ChatChannel variants are currently valid. If we add restricted
            // channels (e.g. GM-only) this is where we'd reject them.
            let channel = msg.channel;

            // --- normalize body ---
            let body: String = msg.body.trim().chars().take(MAX_BODY_LEN).collect();
            if body.is_empty() {
                continue;
            }

            // --- rate limit ---
            if !rate_limiter.check(player_id, now) {
                tracing::debug!(player_id, "Chat rate-limit exceeded, message dropped");
                continue;
            }

            let sender_name = format!("Player#{}", player_id);

            tracing::debug!(
                player_id,
                channel = ?channel,
                "Chat: {}",
                body,
            );

            // Forward to Zulip (best-effort, non-authoritative).
            if let Some(ref tx) = zulip_out {
                let line = format!("[{}] {}: {}", channel_tag(channel), sender_name, body);
                let _ = tx.0.lock().unwrap().send(line);
            }

            outgoing.push(ChatBroadcastMessage {
                sender_name,
                channel,
                body,
            });
        }
    }

    // Broadcast to all connected clients.
    if outgoing.is_empty() {
        return;
    }
    for mut sender in sender_query.iter_mut() {
        for msg in &outgoing {
            sender.send::<ReliableChannel>(msg.clone());
        }
    }
}

/// Forward incoming Zulip lines from the background thread as broadcast messages
/// to all in-game clients.
fn relay_zulip_to_clients(
    zulip_in: Option<Res<ZulipInReceiver>>,
    mut sender_query: Query<&mut MessageSender<ChatBroadcastMessage>, With<ClientOf>>,
) {
    let Some(rx) = zulip_in else { return };
    let Ok(guard) = rx.0.lock() else { return };
    let mut lines: Vec<String> = Vec::new();
    while let Ok(line) = guard.try_recv() {
        lines.push(line);
    }
    if lines.is_empty() {
        return;
    }
    for mut sender in sender_query.iter_mut() {
        for line in &lines {
            sender.send::<ReliableChannel>(ChatBroadcastMessage {
                sender_name: "Zulip".to_string(),
                channel: ChatChannel::Local,
                body: line.clone(),
            });
        }
    }
}

fn channel_tag(channel: ChatChannel) -> &'static str {
    match channel {
        ChatChannel::Local => "local",
        ChatChannel::Party => "party",
        ChatChannel::Guild => "guild",
        ChatChannel::Trade => "trade",
    }
}

// ---------------------------------------------------------------------------
// Zulip background thread (server-side, optional)
// ---------------------------------------------------------------------------

/// Configuration for the optional Zulip bridge.
pub struct ZulipConfig {
    pub url: String,
    pub email: String,
    pub key: String,
}

fn spawn_zulip_thread(cfg: ZulipConfig) -> (Sender<String>, Receiver<String>) {
    let (out_send, out_recv) = mpsc::channel::<String>(); // game → zulip
    let (in_send, in_recv) = mpsc::channel::<String>();   // zulip → game

    thread::spawn(move || {
        poll_zulip(cfg.url, cfg.email, cfg.key, in_send, out_recv);
    });

    (out_send, in_recv)
}

fn poll_zulip(
    url: String,
    email: String,
    key: String,
    to_game: Sender<String>,
    from_game: Receiver<String>,
) {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct RegisterResponse {
        queue_id: String,
        last_event_id: i64,
    }
    #[derive(Deserialize)]
    struct EventsResponse {
        events: Vec<ZulipEvent>,
    }
    #[derive(Deserialize)]
    struct ZulipEvent {
        id: i64,
        #[serde(rename = "type")]
        event_type: String,
        message: Option<ZulipMessage>,
    }
    #[derive(Deserialize)]
    struct ZulipMessage {
        content: String,
        sender_full_name: String,
    }

    let client = reqwest::blocking::Client::new();

    let reg: RegisterResponse = match client
        .post(format!("{}/api/v1/register", url))
        .basic_auth(&email, Some(&key))
        .form(&[("event_types", "[\"message\"]")])
        .send()
        .and_then(|r| r.json())
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Zulip bridge: failed to register queue: {:?}", e);
            return;
        }
    };

    let queue_id = reg.queue_id;
    let mut last_event_id = reg.last_event_id;

    loop {
        // Forward outgoing game messages to Zulip.
        while let Ok(msg) = from_game.try_recv() {
            let _ = client
                .post(format!("{}/api/v1/messages", url))
                .basic_auth(&email, Some(&key))
                .form(&[
                    ("type", "stream"),
                    ("to", "georgikon"),
                    ("topic", "general"),
                    ("content", &msg),
                ])
                .send();
        }

        // Poll for new Zulip messages.
        match client
            .get(format!("{}/api/v1/events", url))
            .basic_auth(&email, Some(&key))
            .query(&[
                ("queue_id", queue_id.as_str()),
                ("last_event_id", &last_event_id.to_string()),
            ])
            .send()
            .and_then(|r| r.json::<EventsResponse>())
        {
            Ok(resp) => {
                for ev in resp.events {
                    if ev.event_type == "message" {
                        if let Some(msg) = ev.message {
                            let line = format!("{}: {}", msg.sender_full_name, msg.content);
                            let _ = to_game.send(line);
                        }
                    }
                    last_event_id = last_event_id.max(ev.id);
                }
            }
            Err(_) => {
                thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct ChatServerPlugin {
    pub zulip: Option<ZulipConfig>,
}

impl Plugin for ChatServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatRateLimiter>();

        if let Some(ref cfg) = self.zulip {
            let zulip_cfg = ZulipConfig {
                url: cfg.url.clone(),
                email: cfg.email.clone(),
                key: cfg.key.clone(),
            };
            let (out_send, in_recv) = spawn_zulip_thread(zulip_cfg);
            app.insert_resource(ZulipOutSender(Mutex::new(out_send)));
            app.insert_resource(ZulipInReceiver(Mutex::new(in_recv)));
        }

        app.add_systems(
            Update,
            (route_chat, relay_zulip_to_clients).chain(),
        );
    }
}
