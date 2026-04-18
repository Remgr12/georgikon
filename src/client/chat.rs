use bevy::prelude::*;
use serde::Deserialize;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;

pub struct ChatPlugin {
    pub url: Option<String>,
    pub email: Option<String>,
    pub key: Option<String>,
}

#[derive(Resource)]
struct ChatReceiver(Mutex<Receiver<String>>);

#[derive(Resource)]
struct ChatSender(Mutex<Sender<String>>);

#[derive(Component)]
struct ChatHistoryText;

#[derive(Component)]
struct ChatInputText;

#[derive(Resource)]
struct ChatState {
    pub is_typing: bool,
    pub current_message: String,
}

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ChatState {
            is_typing: false,
            current_message: String::new(),
        });

        if let (Some(url), Some(email), Some(key)) = (&self.url, &self.email, &self.key) {
            let (rx_send, rx_recv) = mpsc::channel();
            let (tx_send, tx_recv) = mpsc::channel();

            app.insert_resource(ChatReceiver(Mutex::new(rx_recv)));
            app.insert_resource(ChatSender(Mutex::new(tx_send)));

            let url_clone = url.clone();
            let email_clone = email.clone();
            let key_clone = key.clone();

            thread::spawn(move || {
                poll_zulip(url_clone, email_clone, key_clone, rx_send, tx_recv);
            });
        }

        app.add_systems(Startup, setup_chat_ui);
        app.add_systems(Update, (receive_chat_messages, handle_chat_input));
    }
}

fn setup_chat_ui(mut commands: Commands) {
    commands.spawn((
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
    )).with_children(|parent| {
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
            Text::new("> "),
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

fn receive_chat_messages(
    receiver: Option<Res<ChatReceiver>>,
    mut query: Query<&mut Text, With<ChatHistoryText>>,
) {
    if let Some(rx) = receiver {
        if let Ok(msg) = rx.0.lock().unwrap().try_recv() {
            for mut text in query.iter_mut() {
                let mut current = text.0.clone();
                current.push_str(&msg);
                current.push('\n');
                
                let lines: Vec<&str> = current.lines().collect();
                if lines.len() > 15 {
                    text.0 = lines[lines.len() - 15..].join("\n") + "\n";
                } else {
                    text.0 = current;
                }
            }
        }
    }
}

fn handle_chat_input(
    mut state: ResMut<ChatState>,
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut Text, With<ChatInputText>>,
    sender: Option<Res<ChatSender>>,
) {
    let mut update_ui = false;
    let mut send_msg = None;

    if keys.just_pressed(KeyCode::Enter) {
        if state.is_typing {
            state.is_typing = false;
            if !state.current_message.is_empty() {
                send_msg = Some(state.current_message.clone());
            }
            state.current_message.clear();
            update_ui = true;
        } else {
            state.is_typing = true;
            update_ui = true;
        }
    }

    if keys.just_pressed(KeyCode::Escape) {
        state.is_typing = false;
        state.current_message.clear();
        update_ui = true;
    }

    if keys.just_pressed(KeyCode::Backspace) {
        if state.is_typing {
            state.current_message.pop();
            update_ui = true;
        }
    }

    if state.is_typing {
        for key in keys.get_just_pressed() {
            let text = match key {
                KeyCode::Space => Some(' '),
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
            };
            if let Some(c) = text {
                state.current_message.push(c);
                update_ui = true;
            }
        }
    }

    if let Some(msg) = send_msg {
        if let Some(tx) = &sender {
            let _ = tx.0.lock().unwrap().send(msg);
        } else {
            println!("Local chat (no Zulip): {}", msg);
            for mut text in query.iter_mut() {
                 let mut current = text.0.clone();
                 current.push_str(&format!("Local: {}", msg));
                 current.push('\n');
                 text.0 = current;
            }
        }
    }

    if update_ui {
        for mut text in query.iter_mut() {
            if state.is_typing {
                text.0 = format!("> {}_", state.current_message);
            } else {
                text.0 = "> [Press Enter to chat]".to_string();
            }
        }
    }
}

#[derive(Deserialize, Debug)]
struct RegisterResponse {
    queue_id: String,
    last_event_id: i64,
}

#[derive(Deserialize, Debug)]
struct EventsResponse {
    events: Vec<ZulipEvent>,
}

#[derive(Deserialize, Debug)]
struct ZulipEvent {
    id: i64,
    #[serde(rename = "type")]
    event_type: String,
    message: Option<ZulipMessage>,
}

#[derive(Deserialize, Debug)]
struct ZulipMessage {
    content: String,
    sender_full_name: String,
}

fn poll_zulip(url: String, email: String, key: String, to_bevy: Sender<String>, from_bevy: Receiver<String>) {
    let client = reqwest::blocking::Client::new();
    
    let reg_res = client.post(&format!("{}/api/v1/register", url))
        .basic_auth(&email, Some(&key))
        .form(&[("event_types", "[\"message\"]")])
        .send();

    let reg: RegisterResponse = match reg_res {
        Ok(r) => {
            if let Ok(json) = r.json() {
                json
            } else {
                return;
            }
        }
        Err(e) => {
            println!("Failed to register Zulip queue: {:?}", e);
            return;
        }
    };

    let queue_id = reg.queue_id;
    let mut last_event_id = reg.last_event_id;

    loop {
        while let Ok(msg) = from_bevy.try_recv() {
            let _ = client.post(&format!("{}/api/v1/messages", url))
                .basic_auth(&email, Some(&key))
                .form(&[
                    ("type", "stream"),
                    ("to", "georgikon"),
                    ("topic", "general"),
                    ("content", &msg),
                ])
                .send();
        }

        let ev_res = client.get(&format!("{}/api/v1/events", url))
            .basic_auth(&email, Some(&key))
            .query(&[("queue_id", &queue_id), ("last_event_id", &last_event_id.to_string())])
            .send();

        match ev_res {
            Ok(r) => {
                if let Ok(json) = r.json::<EventsResponse>() {
                    for ev in json.events {
                        if ev.event_type == "message" {
                            if let Some(msg) = ev.message {
                                let display = format!("{}: {}", msg.sender_full_name, msg.content);
                                let _ = to_bevy.send(display);
                            }
                        }
                        last_event_id = last_event_id.max(ev.id);
                    }
                } else {
                    thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            Err(_) => {
                thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }
}