use crate::client::commands::CommandQueue;
use crate::client::input::{ActionState, GameAction};
use crate::client::sprite::SpeechBubble;
use crate::common::mob::UnitVisual;
use crate::common::social::{ChatBroadcastMessage, ChatChannel, ChatNetMessage};
use crate::net::{CharacterName, PlayerPosition, ReliableChannel};
use bevy::ecs::message::MessageReader;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::client::Client;

pub struct ChatPlugin;

#[derive(Component)]
struct ChatHistoryText;

#[derive(Component)]
struct ChatInputText;

#[derive(Resource)]
pub(crate) struct ChatState {
    pub(crate) is_typing: bool,
    current_message: String,
    channel: ChatChannel,
    history: Vec<String>,
    history_cursor_from_end: usize,
}

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ChatState {
            is_typing: false,
            current_message: String::new(),
            channel: ChatChannel::Local,
            history: Vec::new(),
            history_cursor_from_end: 0,
        });

        app.add_systems(Startup, setup_chat_ui);
        app.add_systems(Update, (receive_chat_broadcast, handle_chat_input));
    }
}

fn setup_chat_ui(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(10.0),
                left: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                width: Val::Px(400.0),
                height: Val::Px(300.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.8)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                ChatHistoryText,
            ));

            parent.spawn((
                Text::new("> [Press Enter to chat]"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.5, 1.0, 0.5)),
                Node {
                    height: Val::Px(30.0),
                    ..default()
                },
                ChatInputText,
            ));
        });
}

/// Display incoming broadcast messages from the server.
/// For Local channel messages, also spawn a speech bubble above the sender.
fn receive_chat_broadcast(
    mut commands: Commands,
    mut client_query: Query<&mut MessageReceiver<ChatBroadcastMessage>, With<Client>>,
    mut history_query: Query<&mut Text, With<ChatHistoryText>>,
    // Entities with a known name for speech bubble anchoring
    named_q: Query<(Entity, &CharacterName, &PlayerPosition)>,
    visual_q: Query<(Entity, &UnitVisual, &PlayerPosition)>,
) {
    let Ok(mut receiver) = client_query.single_mut() else {
        return;
    };

    let mut new_lines: Vec<String> = Vec::new();
    for msg in receiver.receive() {
        let line = if msg.channel == ChatChannel::Whisper && !msg.target_name.is_empty() {
            format!("[w→{}] {}: {}", msg.target_name, msg.sender_name, msg.body)
        } else {
            format!("[{}] {}: {}", msg.channel.tag(), msg.sender_name, msg.body)
        };
        new_lines.push(line);

        // Spawn speech bubble above the sender for Local channel
        if msg.channel == ChatChannel::Local {
            let sender = msg.sender_name.as_str();
            // Try CharacterName first (remote players)
            let anchor = named_q.iter()
                .find(|(_, n, _)| n.0.as_str() == sender)
                .map(|(e, _, _)| e)
                .or_else(|| visual_q.iter()
                    .find(|(_, v, _)| v.name.as_str() == sender)
                    .map(|(e, _, _)| e));

            if let Some(target) = anchor {
                let display = if msg.body.len() > 40 {
                    format!("{}…", &msg.body[..40])
                } else {
                    msg.body.clone()
                };
                commands.spawn((
                    SpeechBubble { target, timer: 5.0 },
                    Text2d::new(display),
                    TextFont { font_size: 10.0, ..default() },
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 1.0)),
                    Transform::default(),
                ));
            }
        }
    }

    if new_lines.is_empty() {
        return;
    }

    for mut text in history_query.iter_mut() {
        for line in &new_lines {
            text.0.push_str(line);
            text.0.push('\n');
        }
        // Keep at most 20 lines.
        let lines: Vec<String> = text.0.lines().map(|l| l.to_string()).collect();
        if lines.len() > 20 {
            text.0 = lines[lines.len() - 20..].join("\n") + "\n";
        }
    }
}

fn handle_chat_input(
    mut state: ResMut<ChatState>,
    actions: Res<ActionState>,
    mut key_events: MessageReader<KeyboardInput>,
    mut input_query: Query<&mut Text, (With<ChatInputText>, Without<ChatHistoryText>)>,
    mut history_query: Query<&mut Text, (With<ChatHistoryText>, Without<ChatInputText>)>,
    mut client_query: Query<&mut MessageSender<ChatNetMessage>, With<Client>>,
    mut command_queue: ResMut<CommandQueue>,
) {
    let mut update_ui = false;
    let mut send_msg: Option<ParsedChat> = None;

    if actions.just_pressed(GameAction::ChatOpenSend) {
        if state.is_typing {
            state.is_typing = false;
            if !state.current_message.is_empty() {
                let raw = state.current_message.clone();
                if raw.starts_with('/') && !is_chat_prefix(&raw) {
                    command_queue.lines.push(raw.trim_start_matches('/').to_string());
                    state.history.push(raw);
                } else {
                    let parsed = parse_chat_command(&raw, state.channel);
                    if !parsed.body.is_empty() {
                        state.history.push(raw);
                        send_msg = Some(parsed);
                    }
                }
            }
            state.current_message.clear();
            state.history_cursor_from_end = 0;
            update_ui = true;
        } else {
            state.is_typing = true;
            update_ui = true;
        }
    }

    if actions.just_pressed(GameAction::ChatCancel) {
        state.is_typing = false;
        state.current_message.clear();
        state.history_cursor_from_end = 0;
        update_ui = true;
    }

    if state.is_typing && actions.just_pressed(GameAction::ChatHistoryPrev) {
        if state.history_cursor_from_end < state.history.len() {
            state.history_cursor_from_end += 1;
            let idx = state.history.len() - state.history_cursor_from_end;
            state.current_message = state.history[idx].clone();
            update_ui = true;
        }
    }

    if state.is_typing && actions.just_pressed(GameAction::ChatHistoryNext) {
        if state.history_cursor_from_end > 0 {
            state.history_cursor_from_end -= 1;
            if state.history_cursor_from_end == 0 {
                state.current_message.clear();
            } else {
                let idx = state.history.len() - state.history_cursor_from_end;
                state.current_message = state.history[idx].clone();
            }
            update_ui = true;
        }
    }

    // Character input via keyboard events (supports unicode + key repeat for backspace)
    if state.is_typing {
        for ev in key_events.read() {
            if ev.state != ButtonState::Pressed { continue; }
            match &ev.logical_key {
                Key::Backspace => {
                    state.current_message.pop();
                    update_ui = true;
                }
                _ => {
                    let chars: Vec<char> = if let Some(text) = &ev.text {
                        text.chars().filter(|c| !c.is_control()).collect()
                    } else {
                        match &ev.logical_key {
                            Key::Character(s) => s.chars().filter(|c| !c.is_control()).collect(),
                            _ => vec![],
                        }
                    };
                    if !chars.is_empty() {
                        for c in chars {
                            if state.current_message.len() < 256 {
                                state.current_message.push(c);
                            }
                        }
                        update_ui = true;
                    }
                }
            }
        }
    } else {
        // Drain events to keep the reader fresh even when not typing
        for _ in key_events.read() {}
    }

    if let Some(parsed) = send_msg {
        // Whisper is a one-shot channel; don't make it sticky.
        if parsed.channel != ChatChannel::Whisper {
            state.channel = parsed.channel;
        }

        // Send via server. Fall back to local echo when not yet connected.
        if let Ok(mut sender) = client_query.single_mut() {
            sender.send::<ReliableChannel>(ChatNetMessage {
                channel: parsed.channel,
                body: parsed.body,
                target_name: parsed.target_name,
                guild_id: parsed.guild_id,
            });
        } else {
            let line = format!("[{}] You: {}", parsed.channel.tag(), parsed.body);
            for mut text in history_query.iter_mut() {
                text.0.push_str(&line);
                text.0.push('\n');
            }
        }
    }

    if update_ui {
        for mut text in input_query.iter_mut() {
            if state.is_typing {
                text.0 = format!("> [{}] {}_", state.channel.tag(), state.current_message);
            } else {
                text.0 = "> [Press Enter to chat]".to_string();
            }
        }
    }
}

/// Returns true if the line is a chat-channel command (handled as chat) rather
/// than a UI/social command (dispatched via the command queue).
fn is_chat_prefix(line: &str) -> bool {
    const CHAT_VERBS: &[&str] = &[
        "local", "party", "p", "trade", "world", "guild", "g", "g1", "g2", "w", "whisper",
        "tell", "help",
    ];
    let verb = line
        .trim_start_matches('/')
        .split_whitespace()
        .next()
        .unwrap_or("");
    CHAT_VERBS.contains(&verb)
}

/// Result of parsing a chat input line.
struct ParsedChat {
    channel: ChatChannel,
    body: String,
    target_name: String,
    guild_id: u64,
}

impl ParsedChat {
    fn new(channel: ChatChannel, body: &str) -> Self {
        Self {
            channel,
            body: body.trim().to_string(),
            target_name: String::new(),
            guild_id: 0,
        }
    }
}

/// Parse `/channel body` prefix commands. With no prefix, keeps `current`.
fn parse_chat_command(input: &str, current: ChatChannel) -> ParsedChat {
    let prefixes: &[(&str, ChatChannel)] = &[
        ("/local ", ChatChannel::Local),
        ("/party ", ChatChannel::Party),
        ("/p ", ChatChannel::Party),
        ("/trade ", ChatChannel::Trade),
        ("/world ", ChatChannel::World),
        ("/guild ", ChatChannel::Guild),
        ("/g ", ChatChannel::Guild),
    ];
    for (prefix, channel) in prefixes {
        if let Some(rest) = input.strip_prefix(prefix) {
            return ParsedChat::new(*channel, rest);
        }
    }

    // Guild slots: /g1 and /g2 select the first/second guild.
    if let Some(rest) = input.strip_prefix("/g1 ") {
        let mut p = ParsedChat::new(ChatChannel::Guild, rest);
        p.guild_id = 0; // server resolves 0 → first guild
        return p;
    }
    if let Some(rest) = input.strip_prefix("/g2 ") {
        let mut p = ParsedChat::new(ChatChannel::Guild, rest);
        // We don't know the id client-side; sentinel u64::MAX = "second guild".
        p.guild_id = u64::MAX;
        return p;
    }

    // Whisper: `/w <name> <message>`.
    for prefix in ["/w ", "/whisper ", "/tell "] {
        if let Some(rest) = input.strip_prefix(prefix) {
            let rest = rest.trim_start();
            if let Some((name, body)) = rest.split_once(char::is_whitespace) {
                let mut p = ParsedChat::new(ChatChannel::Whisper, body);
                p.target_name = name.to_string();
                return p;
            }
            return ParsedChat::new(current, "");
        }
    }

    if input.trim() == "/help" {
        return ParsedChat::new(current,
            "=== GEORGIKON HELP ===\n\
             Movement: WASD  Jump: Space  Sprint: Shift\n\
             Attack: Z (primary)  X (secondary)  Q (roll)\n\
             Camera: V (cycle)  C/F (zoom)  M (minimap)\n\
             Social: G (guild)  L (quests)  O (mail)  Esc (pause)\n\
             Interact: E (near NPCs)\n\
             Guild: /gcreate <n> /ginvite <n> /gaccept /gleave /glist /visit /home\n\
             Party: /pinvite <n> /paccept /pdecline /pleave\n\
             Quest: /qaccept <id> /qturnin <id> /qabandon <id>\n\
             Mail: /mailbox /mail <n> <msg> /mailread <id>\n\
             Trade: /tradewith <n> /offer <id> <qty> /tradeok /tradeno\n\
             Chat: /local /party /world /trade /g1 /g2 /w <name> <msg>",
        );
    }

    ParsedChat::new(current, input.trim())
}

