//! Client login / registration screen.
//!
//! Shown before gameplay (the initial [`Screen`] is `Login`). The player types a
//! username/password, then presses Enter to log in or F2 to register. On a
//! successful [`LoginResultMessage`] we store the [`LocalCharacter`] and switch
//! to `Screen::Gameplay`, which triggers the local player spawn.

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::common::account::{LoginRequestMessage, LoginResultMessage, RegisterRequestMessage};
use crate::net::ReliableChannel;
use crate::screens::{GoTo, Screen};

/// Identity of the character this client is controlling, set on login success.
#[derive(Resource, Default)]
pub struct LocalCharacter {
    pub id: u64,
    pub name: String,
    pub logged_in: bool,
}

#[derive(PartialEq, Default, Clone, Copy)]
enum Field {
    #[default]
    Username,
    Password,
}

#[derive(Resource, Default)]
struct LoginForm {
    username: String,
    password: String,
    field: Field,
    status: String,
}

#[derive(Component)]
struct LoginUi;

#[derive(Component)]
struct LoginInfoText;

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalCharacter>()
            .init_resource::<LoginForm>()
            .add_systems(OnEnter(Screen::Login), spawn_login_ui)
            .add_systems(OnExit(Screen::Login), despawn_login_ui)
            .add_systems(
                Update,
                (handle_login_input, refresh_login_text, receive_login_result)
                    .run_if(in_state(Screen::Login)),
            );
    }
}

fn spawn_login_ui(mut commands: Commands) {
    commands
        .spawn((
            LoginUi,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.05, 0.08)),
            GlobalZIndex(300),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("GEORGIKON"),
                TextFont {
                    font_size: 64.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.85, 0.95)),
            ));
            root.spawn((
                Text::new(""),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                LoginInfoText,
            ));
        });
}

fn despawn_login_ui(mut commands: Commands, q: Query<Entity, With<LoginUi>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn handle_login_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut form: ResMut<LoginForm>,
    mut login_tx: Query<&mut MessageSender<LoginRequestMessage>, With<Client>>,
    mut register_tx: Query<&mut MessageSender<RegisterRequestMessage>, With<Client>>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        form.field = match form.field {
            Field::Username => Field::Password,
            Field::Password => Field::Username,
        };
    }

    if keys.just_pressed(KeyCode::Backspace) {
        match form.field {
            Field::Username => {
                form.username.pop();
            }
            Field::Password => {
                form.password.pop();
            }
        }
    }

    for key in keys.get_just_pressed() {
        if let Some(c) = key_to_char(*key) {
            match form.field {
                Field::Username => form.username.push(c),
                Field::Password => form.password.push(c),
            }
        }
    }

    let submit_login = keys.just_pressed(KeyCode::Enter);
    let submit_register = keys.just_pressed(KeyCode::F2);
    if !(submit_login || submit_register) {
        return;
    }
    if form.username.is_empty() || form.password.is_empty() {
        form.status = "Enter a username and password".into();
        return;
    }

    let username = form.username.clone();
    let password = form.password.clone();
    if submit_register {
        if let Ok(mut tx) = register_tx.single_mut() {
            tx.send::<ReliableChannel>(RegisterRequestMessage { username, password });
            form.status = "Registering…".into();
        } else {
            form.status = "Not connected yet…".into();
        }
    } else if let Ok(mut tx) = login_tx.single_mut() {
        tx.send::<ReliableChannel>(LoginRequestMessage { username, password });
        form.status = "Logging in…".into();
    } else {
        form.status = "Not connected yet…".into();
    }
}

fn refresh_login_text(form: Res<LoginForm>, mut q: Query<&mut Text, With<LoginInfoText>>) {
    if !form.is_changed() {
        return;
    }
    let user_cursor = if form.field == Field::Username { "_" } else { "" };
    let pass_cursor = if form.field == Field::Password { "_" } else { "" };
    let masked: String = "*".repeat(form.password.len());
    for mut text in q.iter_mut() {
        text.0 = format!(
            "Username: {}{}\nPassword: {}{}\n\n[Tab] switch field   [Enter] login   [F2] register\n{}",
            form.username, user_cursor, masked, pass_cursor, form.status,
        );
    }
}

fn receive_login_result(
    mut commands: Commands,
    mut rx: Query<&mut MessageReceiver<LoginResultMessage>, With<Client>>,
    mut form: ResMut<LoginForm>,
    mut local: ResMut<LocalCharacter>,
) {
    let Ok(mut receiver) = rx.single_mut() else {
        return;
    };
    for result in receiver.receive() {
        if result.ok {
            local.id = result.character_id;
            local.name = result.name.clone();
            local.logged_in = true;
            form.password.clear();
            commands.trigger(GoTo(Screen::Gameplay));
        } else {
            form.status = result.reason.clone();
        }
    }
}

/// Minimal printable-key → char mapping for text entry.
fn key_to_char(key: KeyCode) -> Option<char> {
    use KeyCode::*;
    Some(match key {
        KeyA => 'a', KeyB => 'b', KeyC => 'c', KeyD => 'd', KeyE => 'e', KeyF => 'f',
        KeyG => 'g', KeyH => 'h', KeyI => 'i', KeyJ => 'j', KeyK => 'k', KeyL => 'l',
        KeyM => 'm', KeyN => 'n', KeyO => 'o', KeyP => 'p', KeyQ => 'q', KeyR => 'r',
        KeyS => 's', KeyT => 't', KeyU => 'u', KeyV => 'v', KeyW => 'w', KeyX => 'x',
        KeyY => 'y', KeyZ => 'z',
        Digit0 => '0', Digit1 => '1', Digit2 => '2', Digit3 => '3', Digit4 => '4',
        Digit5 => '5', Digit6 => '6', Digit7 => '7', Digit8 => '8', Digit9 => '9',
        Minus => '-', Period => '.',
        _ => return None,
    })
}
