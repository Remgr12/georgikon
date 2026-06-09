//! Client login / registration screen.
//!
//! Input uses `EventReader<KeyboardInput>` so it works on any keyboard layout.
//! Tab focuses the next field; Enter submits login; F2 submits registration.
//! The screen disappears on a successful [`LoginResultMessage`].

use bevy::ecs::message::MessageReader;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::{Connected, Disconnected, MessageReceiver, MessageSender};

use crate::common::account::{
    LoginRequestMessage, LoginResultMessage, RegisterRequestMessage, MAX_NAME_LEN,
};
use crate::net::ReliableChannel;
use crate::screens::{GoTo, Screen};

// ─── Resource ─────────────────────────────────────────────────────────────────

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
    active_field: Field,
    status: String,
    status_ok: bool,
    submitting: bool,
    submit_elapsed: f32,
}

// ─── Markers ──────────────────────────────────────────────────────────────────

#[derive(Component)] struct LoginUi;
#[derive(Component)] struct UsernameField;
#[derive(Component)] struct PasswordField;
#[derive(Component)] struct UsernameText;
#[derive(Component)] struct PasswordText;
#[derive(Component)] struct LoginButton;
#[derive(Component)] struct RegisterButton;
#[derive(Component)] struct StatusText;
#[derive(Component)] struct ConnectionDot;
#[derive(Component)] struct ConnectionStatusText;

// ─── Plugin ───────────────────────────────────────────────────────────────────

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalCharacter>()
            .init_resource::<LoginForm>()
            .add_observer(on_client_disconnect)
            .add_systems(OnEnter(Screen::Login), spawn_login_ui)
            .add_systems(OnExit(Screen::Login), despawn_login_ui)
            .add_systems(
                Update,
                (
                    handle_keyboard_input,
                    handle_field_clicks,
                    handle_button_clicks,
                    update_field_displays,
                    update_field_styles,
                )
                    .run_if(in_state(Screen::Login)),
            )
            .add_systems(
                Update,
                (
                    update_button_styles,
                    update_status_text,
                    update_connection_dot,
                    receive_login_result,
                    tick_submit_timeout,
                )
                    .run_if(in_state(Screen::Login)),
            );
    }
}

// ─── Colors ───────────────────────────────────────────────────────────────────

// Nord palette — https://www.nordtheme.com/docs/colors-and-palettes
// Polar Night: nord0–3   Snow Storm: nord4–6   Frost: nord7–10   Aurora: nord11–15
const NORD0:  Color = Color::srgb(0.180, 0.204, 0.251); // #2E3440
const NORD1:  Color = Color::srgb(0.231, 0.259, 0.322); // #3B4252
const NORD2:  Color = Color::srgb(0.263, 0.298, 0.369); // #434C5E
const NORD3:  Color = Color::srgb(0.298, 0.337, 0.416); // #4C566A
const NORD4:  Color = Color::srgb(0.847, 0.871, 0.914); // #D8DEE9
const NORD6:  Color = Color::srgb(0.925, 0.937, 0.957); // #ECEFF4
const NORD7:  Color = Color::srgb(0.561, 0.737, 0.733); // #8FBCBB
const NORD8:  Color = Color::srgb(0.533, 0.753, 0.816); // #88C0D0
const NORD9:  Color = Color::srgb(0.506, 0.631, 0.757); // #81A1C1
const NORD10: Color = Color::srgb(0.369, 0.506, 0.675); // #5E81AC
const NORD11: Color = Color::srgb(0.749, 0.380, 0.416); // #BF616A
const NORD14: Color = Color::srgb(0.639, 0.745, 0.549); // #A3BE8C

const C_BG:            Color = NORD0;
const C_CARD:          Color = NORD1;
const C_CARD_BORDER:   Color = NORD2;

const C_ACCENT:        Color = NORD8;
const C_ACCENT_BAR:    Color = NORD10;

const C_TITLE:         Color = NORD6;
const C_SUBTITLE:      Color = NORD3;
const C_LABEL:         Color = NORD3;
const C_TEXT:          Color = NORD4;
const C_HINT:          Color = NORD2;

const C_FIELD:         Color = NORD0;
const C_FIELD_ACTIVE:  Color = NORD1;
const C_BORDER_IDLE:   Color = NORD2;
const C_BORDER_ACTIVE: Color = NORD8;

const C_BTN_PRI:       Color = NORD10;
const C_BTN_PRI_HOV:   Color = NORD9;
const C_BTN_PRI_PRE:   Color = Color::srgb(0.29, 0.40, 0.54);
const C_BTN_PRI_BORD:  Color = NORD8;

const C_BTN_SEC:       Color = NORD1;
const C_BTN_SEC_HOV:   Color = NORD2;
const C_BTN_SEC_PRE:   Color = NORD0;
const C_BTN_SEC_BORD:  Color = NORD3;

const C_STATUS_OK:     Color = NORD14;
const C_STATUS_ERR:    Color = NORD11;
const C_DOT_ON:        Color = NORD14;
const C_DOT_OFF:       Color = NORD11;

// ─── UI construction ──────────────────────────────────────────────────────────

fn spawn_login_ui(mut commands: Commands) {
    commands
        .spawn((
            LoginUi,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(C_BG),
            GlobalZIndex(300),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    min_width: Val::Px(380.0),
                    border: UiRect::all(Val::Px(1.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(C_CARD),
                BorderColor::all(C_CARD_BORDER),
            ))
            .with_children(|card| {
                // Gold accent bar at the very top of the card.
                card.spawn((
                    Node { width: Val::Percent(100.0), height: Val::Px(3.0), ..default() },
                    BackgroundColor(C_ACCENT_BAR),
                ));

                // Inner padding wrapper
                card.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(44.0), Val::Px(36.0)),
                    row_gap: Val::Px(18.0),
                    ..default()
                })
                .with_children(|inner| {
                    // Title
                    inner.spawn((
                        Text::new("GEORGIKON"),
                        TextFont { font_size: 50.0, ..default() },
                        TextColor(C_TITLE),
                    ));

                    // Subtitle
                    inner.spawn((
                        Text::new("MMORPG  ·  alpha"),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(C_SUBTITLE),
                        Node { margin: UiRect::top(Val::Px(-10.0)), ..default() },
                    ));

                    // Separator line
                    inner.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(1.0),
                            margin: UiRect::vertical(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(C_ACCENT),
                    ));

                    // Fields
                    spawn_labeled_field(inner, "USERNAME", true);
                    spawn_labeled_field(inner, "PASSWORD", false);

                    // Status line
                    inner.spawn((
                        Text::new(""),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(C_STATUS_ERR),
                        StatusText,
                        Node { min_height: Val::Px(16.0), ..default() },
                    ));

                    // Login button (primary, full width)
                    inner.spawn((
                        Button,
                        LoginButton,
                        Interaction::None,
                        Node {
                            width: Val::Percent(100.0),
                            justify_content: JustifyContent::Center,
                            padding: UiRect::axes(Val::Px(0.0), Val::Px(13.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(C_BTN_PRI),
                        BorderColor::all(C_BTN_PRI_BORD),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("LOG IN"),
                            TextFont { font_size: 15.0, ..default() },
                            TextColor(NORD6),
                        ));
                    });

                    // Register button (secondary, narrower)
                    inner.spawn((
                        Button,
                        RegisterButton,
                        Interaction::None,
                        Node {
                            width: Val::Percent(100.0),
                            justify_content: JustifyContent::Center,
                            padding: UiRect::axes(Val::Px(0.0), Val::Px(10.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(C_BTN_SEC),
                        BorderColor::all(C_BTN_SEC_BORD),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("CREATE ACCOUNT"),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(C_ACCENT),
                        ));
                    });

                    // Key hints
                    inner.spawn((
                        Text::new("Tab · switch field  ·  Enter · log in  ·  F2 · create account"),
                        TextFont { font_size: 10.5, ..default() },
                        TextColor(C_HINT),
                    ));

                    // Connection status row
                    inner.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            ConnectionDot,
                            Node {
                                width: Val::Px(8.0),
                                height: Val::Px(8.0),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(C_DOT_OFF),
                            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.3)),
                        ));
                        row.spawn((
                            ConnectionStatusText,
                            Text::new("connecting to server…"),
                            TextFont { font_size: 11.0, ..default() },
                            TextColor(C_LABEL),
                        ));
                    });
                });
            });
        });
}

fn spawn_labeled_field(parent: &mut ChildSpawnerCommands<'_>, label: &str, is_username: bool) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            width: Val::Percent(100.0),
            ..default()
        })
        .with_children(|col| {
            col.spawn((
                Text::new(label),
                TextFont { font_size: 11.0, ..default() },
                TextColor(C_LABEL),
            ));

            let mut field = col.spawn((
                Button,
                Interaction::None,
                Node {
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(11.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    width: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(C_FIELD),
                BorderColor::all(C_BORDER_IDLE),
            ));

            if is_username { field.insert(UsernameField); } else { field.insert(PasswordField); }

            field.with_children(|box_| {
                let mut ts = box_.spawn((
                    Text::new(""),
                    TextFont { font_size: 16.0, ..default() },
                    TextColor(C_TEXT),
                ));
                if is_username { ts.insert(UsernameText); } else { ts.insert(PasswordText); }
            });
        });
}

fn despawn_login_ui(mut commands: Commands, q: Query<Entity, With<LoginUi>>) {
    for e in &q { commands.entity(e).despawn(); }
}

// ─── Input ────────────────────────────────────────────────────────────────────

fn handle_keyboard_input(
    mut key_events: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut form: ResMut<LoginForm>,
    mut login_tx: Query<&mut MessageSender<LoginRequestMessage>, With<Client>>,
    mut register_tx: Query<&mut MessageSender<RegisterRequestMessage>, With<Client>>,
    connected: Query<(), (With<Client>, With<Connected>)>,
) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);

    for ev in key_events.read() {
        if ev.state != ButtonState::Pressed { continue; }

        match &ev.logical_key {
            Key::Tab => {
                form.active_field = match form.active_field {
                    Field::Username => Field::Password,
                    Field::Password => Field::Username,
                };
            }
            Key::Enter => {
                if !form.submitting {
                    try_submit(&mut form, false, &mut login_tx, &mut register_tx, &connected);
                }
            }
            Key::F2 => {
                if !form.submitting {
                    try_submit(&mut form, true, &mut login_tx, &mut register_tx, &connected);
                }
            }
            Key::Backspace => {
                if ctrl {
                    match form.active_field {
                        Field::Username => form.username.clear(),
                        Field::Password => form.password.clear(),
                    }
                } else {
                    match form.active_field {
                        Field::Username => { form.username.pop(); }
                        Field::Password => { form.password.pop(); }
                    }
                }
            }
            _ => {
                // Primary: ev.text (populated by the OS text-input protocol).
                // Fallback: ev.logical_key covers Wayland setups where the text
                // input protocol is unavailable and ev.text is always None.
                let input_chars: Vec<char> = if let Some(text) = &ev.text {
                    text.chars().filter(|c| !c.is_control()).collect()
                } else {
                    match &ev.logical_key {
                        Key::Character(s) => s.chars().filter(|c| !c.is_control()).collect(),
                        _ => vec![],
                    }
                };
                for c in input_chars {
                    match form.active_field {
                        Field::Username => {
                            if form.username.len() < MAX_NAME_LEN { form.username.push(c); }
                        }
                        Field::Password => {
                            if form.password.len() < 64 { form.password.push(c); }
                        }
                    }
                }
            }
        }
    }
}

fn handle_field_clicks(
    username_q: Query<&Interaction, (With<UsernameField>, Changed<Interaction>)>,
    password_q: Query<&Interaction, (With<PasswordField>, Changed<Interaction>)>,
    mut form: ResMut<LoginForm>,
) {
    if let Ok(Interaction::Pressed) = username_q.single() { form.active_field = Field::Username; }
    if let Ok(Interaction::Pressed) = password_q.single() { form.active_field = Field::Password; }
}

fn handle_button_clicks(
    login_q: Query<&Interaction, (With<LoginButton>, Changed<Interaction>)>,
    register_q: Query<&Interaction, (With<RegisterButton>, Changed<Interaction>)>,
    mut form: ResMut<LoginForm>,
    mut login_tx: Query<&mut MessageSender<LoginRequestMessage>, With<Client>>,
    mut register_tx: Query<&mut MessageSender<RegisterRequestMessage>, With<Client>>,
    connected: Query<(), (With<Client>, With<Connected>)>,
) {
    let login_pressed  = login_q.iter().any(|i| *i == Interaction::Pressed);
    let reg_pressed    = register_q.iter().any(|i| *i == Interaction::Pressed);
    if (login_pressed || reg_pressed) && !form.submitting {
        try_submit(&mut form, reg_pressed, &mut login_tx, &mut register_tx, &connected);
    }
}

fn try_submit(
    form: &mut LoginForm,
    is_register: bool,
    login_tx: &mut Query<&mut MessageSender<LoginRequestMessage>, With<Client>>,
    register_tx: &mut Query<&mut MessageSender<RegisterRequestMessage>, With<Client>>,
    connected: &Query<(), (With<Client>, With<Connected>)>,
) {
    if form.username.is_empty() || form.password.is_empty() {
        form.status = "Enter a username and password.".into();
        form.status_ok = false;
        return;
    }
    if connected.is_empty() {
        form.status = "Not connected to the server — please wait.".into();
        form.status_ok = false;
        return;
    }

    let username = form.username.clone();
    let password = form.password.clone();

    if is_register {
        if let Ok(mut tx) = register_tx.single_mut() {
            tx.send::<ReliableChannel>(RegisterRequestMessage { username, password });
            form.status = "Creating account…".into();
            form.status_ok = true;
            form.submitting = true;
            form.submit_elapsed = 0.0;
        } else {
            form.status = "Not connected yet — is the server running?".into();
            form.status_ok = false;
        }
    } else if let Ok(mut tx) = login_tx.single_mut() {
        tx.send::<ReliableChannel>(LoginRequestMessage { username, password });
        form.status = "Logging in…".into();
        form.status_ok = true;
        form.submitting = true;
        form.submit_elapsed = 0.0;
    } else {
        form.status = "Not connected yet — is the server running?".into();
        form.status_ok = false;
    }
}

fn receive_login_result(
    mut commands: Commands,
    mut rx: Query<&mut MessageReceiver<LoginResultMessage>, With<Client>>,
    mut form: ResMut<LoginForm>,
    mut local: ResMut<LocalCharacter>,
) {
    let Ok(mut receiver) = rx.single_mut() else { return };
    for result in receiver.receive() {
        if result.ok {
            local.id   = result.character_id;
            local.name = result.name.clone();
            local.logged_in = true;
            form.password.clear();
            form.submitting = false;
            commands.trigger(GoTo(Screen::Gameplay));
        } else {
            form.status    = result.reason.clone();
            form.status_ok = false;
            form.submitting = false;
        }
    }
}

fn tick_submit_timeout(time: Res<Time>, mut form: ResMut<LoginForm>) {
    if !form.submitting { return; }
    form.submit_elapsed += time.delta_secs();
    if form.submit_elapsed > 15.0 {
        form.submitting = false;
        form.submit_elapsed = 0.0;
        form.status = "Server did not respond. Please try again.".into();
        form.status_ok = false;
    }
}

fn on_client_disconnect(
    trigger: On<Add, Disconnected>,
    client_q: Query<(), With<Client>>,
    mut local: ResMut<LocalCharacter>,
    mut form: ResMut<LoginForm>,
    mut commands: Commands,
) {
    if client_q.get(trigger.entity).is_err() { return; }
    let was_logged_in = local.logged_in;
    *local = LocalCharacter::default();
    form.submitting = false;
    form.submit_elapsed = 0.0;
    form.password.clear();
    if was_logged_in {
        form.status    = "Disconnected from server.".into();
        form.status_ok = false;
        commands.trigger(GoTo(Screen::Login));
    } else {
        form.status    = "Could not connect — is the server running?".into();
        form.status_ok = false;
    }
}

// ─── Display update systems ───────────────────────────────────────────────────

fn update_field_displays(
    form: Res<LoginForm>,
    mut uname_q: Query<&mut Text, (With<UsernameText>, Without<PasswordText>)>,
    mut pass_q:  Query<&mut Text, (With<PasswordText>, Without<UsernameText>)>,
) {
    if !form.is_changed() { return; }
    let cu = if form.active_field == Field::Username { "│" } else { "" };
    let cp = if form.active_field == Field::Password { "│" } else { "" };
    if let Ok(mut t) = uname_q.single_mut() { t.0 = format!("{}{}", form.username, cu); }
    if let Ok(mut t) = pass_q.single_mut()  { t.0 = format!("{}{}", "●".repeat(form.password.len()), cp); }
}

fn update_field_styles(
    form: Res<LoginForm>,
    mut uname_q: Query<(&mut BackgroundColor, &mut BorderColor), (With<UsernameField>, Without<PasswordField>)>,
    mut pass_q:  Query<(&mut BackgroundColor, &mut BorderColor), (With<PasswordField>, Without<UsernameField>)>,
) {
    if !form.is_changed() { return; }
    let set = |active: bool, bg: &mut BackgroundColor, border: &mut BorderColor| {
        *bg     = BackgroundColor(if active { C_FIELD_ACTIVE } else { C_FIELD });
        *border = BorderColor::all(if active { C_BORDER_ACTIVE } else { C_BORDER_IDLE });
    };
    if let Ok((mut bg, mut b)) = uname_q.single_mut() { set(form.active_field == Field::Username, &mut bg, &mut b); }
    if let Ok((mut bg, mut b)) = pass_q.single_mut()  { set(form.active_field == Field::Password, &mut bg, &mut b); }
}

fn update_button_styles(
    mut login_q: Query<(&Interaction, &mut BackgroundColor, &mut BorderColor), (With<LoginButton>, Changed<Interaction>)>,
    mut reg_q:   Query<(&Interaction, &mut BackgroundColor, &mut BorderColor), (With<RegisterButton>, Changed<Interaction>)>,
) {
    for (i, mut bg, mut border) in login_q.iter_mut() {
        *bg     = BackgroundColor(match i { Interaction::Pressed => C_BTN_PRI_PRE, Interaction::Hovered => C_BTN_PRI_HOV, _ => C_BTN_PRI });
        *border = BorderColor::all(C_BTN_PRI_BORD);
    }
    for (i, mut bg, mut border) in reg_q.iter_mut() {
        *bg     = BackgroundColor(match i { Interaction::Pressed => C_BTN_SEC_PRE, Interaction::Hovered => C_BTN_SEC_HOV, _ => C_BTN_SEC });
        *border = BorderColor::all(match i { Interaction::None => C_BTN_SEC_BORD, _ => C_ACCENT });
    }
}

fn update_status_text(
    form: Res<LoginForm>,
    mut q: Query<(&mut Text, &mut TextColor), With<StatusText>>,
) {
    if !form.is_changed() { return; }
    if let Ok((mut text, mut color)) = q.single_mut() {
        text.0 = form.status.clone();
        *color = TextColor(if form.status_ok { C_STATUS_OK } else { C_STATUS_ERR });
    }
}

fn update_connection_dot(
    connected: Query<(), (With<Client>, With<Connected>)>,
    mut dot_q: Query<&mut BackgroundColor, With<ConnectionDot>>,
    mut status_q: Query<(&mut Text, &mut TextColor), With<ConnectionStatusText>>,
) {
    let is_connected = !connected.is_empty();
    if let Ok(mut bg) = dot_q.single_mut() {
        *bg = BackgroundColor(if is_connected { C_DOT_ON } else { C_DOT_OFF });
    }
    if let Ok((mut text, mut color)) = status_q.single_mut() {
        if is_connected {
            text.0 = "connected to server".into();
            *color = TextColor(C_DOT_ON);
        } else {
            text.0 = "connecting to server…".into();
            *color = TextColor(C_LABEL);
        }
    }
}
