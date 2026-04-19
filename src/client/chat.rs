use crate::client::input::{ActionState, GameAction};
use crate::common::social::{ChatBroadcastMessage, ChatChannel, ChatNetMessage};
use crate::net::ReliableChannel;
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

fn channel_tag(channel: ChatChannel) -> &'static str {
    match channel {
        ChatChannel::Local => "local",
        ChatChannel::Party => "party",
        ChatChannel::Guild => "guild",
        ChatChannel::Trade => "trade",
    }
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
fn receive_chat_broadcast(
    mut client_query: Query<&mut MessageReceiver<ChatBroadcastMessage>, With<Client>>,
    mut history_query: Query<&mut Text, With<ChatHistoryText>>,
) {
    let Ok(mut receiver) = client_query.single_mut() else {
        return;
    };

    let mut new_lines: Vec<String> = Vec::new();
    for msg in receiver.receive() {
        new_lines.push(format!(
            "[{}] {}: {}",
            channel_tag(msg.channel),
            msg.sender_name,
            msg.body,
        ));
    }

    if new_lines.is_empty() {
        return;
    }

    for mut text in history_query.iter_mut() {
        for line in &new_lines {
            text.0.push_str(line);
            text.0.push('\n');
        }
        // Keep at most 15 lines.
        let lines: Vec<String> = text.0.lines().map(|l| l.to_string()).collect();
        if lines.len() > 15 {
            text.0 = lines[lines.len() - 15..].join("\n") + "\n";
        }
    }
}

fn handle_chat_input(
    mut state: ResMut<ChatState>,
    actions: Res<ActionState>,
    keys: Res<ButtonInput<KeyCode>>,
    mut input_query: Query<&mut Text, (With<ChatInputText>, Without<ChatHistoryText>)>,
    mut history_query: Query<&mut Text, (With<ChatHistoryText>, Without<ChatInputText>)>,
    mut client_query: Query<&mut MessageSender<ChatNetMessage>, With<Client>>,
) {
    let mut update_ui = false;
    let mut send_msg: Option<(ChatChannel, String)> = None;

    if actions.just_pressed(GameAction::ChatOpenSend) {
        if state.is_typing {
            state.is_typing = false;
            if !state.current_message.is_empty() {
                let raw = state.current_message.clone();
                let (channel, body) = parse_chat_command(&raw, state.channel);
                if !body.is_empty() {
                    state.history.push(raw);
                    send_msg = Some((channel, body));
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

    if state.is_typing && actions.just_pressed(GameAction::ChatBackspace) {
        state.current_message.pop();
        update_ui = true;
    }

    if state.is_typing {
        for key in keys.get_just_pressed() {
            if let Some(c) = key_to_char(*key) {
                state.current_message.push(c);
                update_ui = true;
            } else if *key == KeyCode::Space {
                state.current_message.push(' ');
                update_ui = true;
            } else if *key == KeyCode::Slash {
                state.current_message.push('/');
                update_ui = true;
            } else if *key == KeyCode::Minus {
                state.current_message.push('-');
                update_ui = true;
            } else if *key == KeyCode::Period {
                state.current_message.push('.');
                update_ui = true;
            } else if *key == KeyCode::Comma {
                state.current_message.push(',');
                update_ui = true;
            }
        }
    }

    if let Some((channel, body)) = send_msg {
        // Update channel state for future messages.
        state.channel = channel;

        // Send via server. Fall back to local echo when not yet connected.
        if let Ok(mut sender) = client_query.single_mut() {
            sender.send::<ReliableChannel>(ChatNetMessage { channel, body });
        } else {
            // Local fallback (no server connection yet, e.g. startup race).
            let line = format!("[{}] You: {}", channel_tag(channel), body);
            for mut text in history_query.iter_mut() {
                text.0.push_str(&line);
                text.0.push('\n');
            }
        }
    }

    if update_ui {
        for mut text in input_query.iter_mut() {
            if state.is_typing {
                text.0 = format!(
                    "> [{}] {}_",
                    channel_tag(state.channel),
                    state.current_message
                );
            } else {
                text.0 = "> [Press Enter to chat]".to_string();
            }
        }
    }
}

/// Parse `/channel body` prefix commands. Returns `(channel, body)`.
/// If no prefix matches, returns the current channel and original text.
fn parse_chat_command(input: &str, current: ChatChannel) -> (ChatChannel, String) {
    if let Some(msg) = input.strip_prefix("/party ") {
        return (ChatChannel::Party, msg.trim().to_string());
    }
    if let Some(msg) = input.strip_prefix("/guild ") {
        return (ChatChannel::Guild, msg.trim().to_string());
    }
    if let Some(msg) = input.strip_prefix("/trade ") {
        return (ChatChannel::Trade, msg.trim().to_string());
    }
    if let Some(msg) = input.strip_prefix("/local ") {
        return (ChatChannel::Local, msg.trim().to_string());
    }
    if input.trim() == "/help" {
        return (
            ChatChannel::Local,
            "Commands: /local /party /guild /trade /help".to_string(),
        );
    }
    // No prefix: keep current channel.
    (current, input.trim().to_string())
}

fn key_to_char(key: KeyCode) -> Option<char> {
    match key {
        KeyCode::KeyA => Some('a'),
        KeyCode::KeyB => Some('b'),
        KeyCode::KeyC => Some('c'),
        KeyCode::KeyD => Some('d'),
        KeyCode::KeyE => Some('e'),
        KeyCode::KeyF => Some('f'),
        KeyCode::KeyG => Some('g'),
        KeyCode::KeyH => Some('h'),
        KeyCode::KeyI => Some('i'),
        KeyCode::KeyJ => Some('j'),
        KeyCode::KeyK => Some('k'),
        KeyCode::KeyL => Some('l'),
        KeyCode::KeyM => Some('m'),
        KeyCode::KeyN => Some('n'),
        KeyCode::KeyO => Some('o'),
        KeyCode::KeyP => Some('p'),
        KeyCode::KeyQ => Some('q'),
        KeyCode::KeyR => Some('r'),
        KeyCode::KeyS => Some('s'),
        KeyCode::KeyT => Some('t'),
        KeyCode::KeyU => Some('u'),
        KeyCode::KeyV => Some('v'),
        KeyCode::KeyW => Some('w'),
        KeyCode::KeyX => Some('x'),
        KeyCode::KeyY => Some('y'),
        KeyCode::KeyZ => Some('z'),
        KeyCode::Digit0 => Some('0'),
        KeyCode::Digit1 => Some('1'),
        KeyCode::Digit2 => Some('2'),
        KeyCode::Digit3 => Some('3'),
        KeyCode::Digit4 => Some('4'),
        KeyCode::Digit5 => Some('5'),
        KeyCode::Digit6 => Some('6'),
        KeyCode::Digit7 => Some('7'),
        KeyCode::Digit8 => Some('8'),
        KeyCode::Digit9 => Some('9'),
        _ => None,
    }
}
