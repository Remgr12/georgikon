//! Quest log (toggle `L`) + level/XP HUD (top-center) + toast notifications.
//! Actions via chat slash-commands (`/qaccept`, `/qturnin`, `/qabandon`).

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::MessageReceiver;

use crate::client::chat::ChatState;
use crate::client::input::{ActionState, GameAction};
use crate::common::quest::{ProgressionMessage, QuestInfo, QuestLogMessage, QuestState};

#[derive(Resource, Default)]
pub struct QuestStore {
    pub quests: Vec<QuestInfo>,
    open: bool,
}

#[derive(Resource)]
pub struct ProgressionStore {
    pub level: u32,
    pub xp: u64,
    pub xp_to_next: u64,
    prev_level: u32,
}

impl Default for ProgressionStore {
    fn default() -> Self {
        Self { level: 1, xp: 0, xp_to_next: 100, prev_level: 0 }
    }
}

#[derive(Component)]
struct QuestPanel;
#[derive(Component)]
struct QuestPanelText;
#[derive(Component)]
struct XpHudText;

pub struct QuestClientPlugin;

impl Plugin for QuestClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<QuestStore>();
        app.init_resource::<ProgressionStore>();
        app.init_resource::<ToastQueue>();
        app.add_systems(Startup, (spawn_panel, spawn_xp_hud, spawn_toast));
        app.add_systems(
            Update,
            (recv_log, recv_progression, toggle_panel, render_panel, render_xp, update_toast),
        );
    }
}

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((
            QuestPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(230.0), // below stat bars + party frames
                left: Val::Px(10.0),
                width: Val::Px(360.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.06, 0.02, 0.9)),
            GlobalZIndex(150),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                QuestPanelText,
            ));
        });
}

fn spawn_xp_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-80.0)),
                ..default()
            },
            GlobalZIndex(120),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Lv 1  0/100 XP"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.9, 0.5)),
                XpHudText,
            ));
        });
}

fn recv_log(
    mut store: ResMut<QuestStore>,
    mut toasts: ResMut<ToastQueue>,
    mut q: Query<&mut MessageReceiver<QuestLogMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            // Detect newly completed quests
            for new_quest in &msg.quests {
                if new_quest.state == QuestState::Complete {
                    let was_active = store.quests.iter()
                        .find(|q| q.quest_id == new_quest.quest_id)
                        .map(|q| q.state == QuestState::Active)
                        .unwrap_or(false);
                    if was_active {
                        toasts.push(format!("Quest complete: {}  — type /qturnin {}", new_quest.name, new_quest.quest_id), 4.0);
                    }
                }
            }
            store.quests = msg.quests;
        }
    }
}

fn recv_progression(
    mut store: ResMut<ProgressionStore>,
    mut toasts: ResMut<ToastQueue>,
    mut q: Query<&mut MessageReceiver<ProgressionMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            let leveled_up = store.prev_level > 0 && msg.level > store.level;
            store.level = msg.level;
            store.xp = msg.xp;
            store.xp_to_next = msg.xp_to_next;
            if store.prev_level == 0 {
                // First progression message — show welcome
                toasts.push(format!("Welcome back, adventurer! You are level {}.", msg.level), 3.0);
            }
            if leveled_up {
                toasts.push(format!("★ LEVEL UP! You are now level {} ★", msg.level), 4.0);
            }
            store.prev_level = msg.level;
        }
    }
}

fn toggle_panel(
    actions: Res<ActionState>,
    chat: Res<ChatState>,
    mut store: ResMut<QuestStore>,
    mut q: Query<&mut Visibility, With<QuestPanel>>,
) {
    if chat.is_typing {
        return;
    }
    if actions.just_pressed(GameAction::ToggleQuestLog) {
        store.open = !store.open;
        if let Ok(mut vis) = q.single_mut() {
            *vis = if store.open {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn render_panel(store: Res<QuestStore>, mut q: Query<&mut Text, With<QuestPanelText>>) {
    if !store.open || !store.is_changed() {
        return;
    }
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    let mut s = String::from("=== QUESTS ===\n");
    for qst in &store.quests {
        let tag = match qst.state {
            QuestState::Available => "[available]",
            QuestState::Active => "[active]",
            QuestState::Complete => "[COMPLETE]",
            QuestState::TurnedIn => "[done]",
        };
        s.push_str(&format!("({}) {} {}\n", qst.quest_id, qst.name, tag));
        for o in &qst.objectives {
            s.push_str(&format!("   {} {}/{}\n", o.text, o.count, o.required));
        }
    }
    s.push_str("\nCmds: /qaccept <id> /qturnin <id> /qabandon <id>\n");
    text.0 = s;
}

fn render_xp(store: Res<ProgressionStore>, mut q: Query<&mut Text, With<XpHudText>>) {
    if !store.is_changed() { return; }
    if let Ok(mut text) = q.single_mut() {
        text.0 = format!("Lv {}  {}/{} XP", store.level, store.xp, store.xp_to_next);
    }
}

// ─── Toast notifications ──────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct ToastQueue {
    messages: std::collections::VecDeque<(String, f32)>,
}

impl ToastQueue {
    pub fn push(&mut self, msg: impl Into<String>, duration: f32) {
        self.messages.push_back((msg.into(), duration));
    }
}

#[derive(Component)]
struct ToastPanel;

#[derive(Component)]
struct ToastText;

#[derive(Component)]
struct ToastTimer(f32);

fn spawn_toast(mut commands: Commands) {
    commands.spawn((
        ToastPanel,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(44.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-160.0)),
            width: Val::Px(320.0),
            padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.0)),
        GlobalZIndex(200),
        Visibility::Hidden,
        ToastTimer(0.0),
    )).with_children(|p| {
        p.spawn((
            ToastText,
            Text::new(""),
            TextFont { font_size: 15.0, ..default() },
            TextColor(Color::srgb(1.0, 0.90, 0.25)),
        ));
    });
}

fn update_toast(
    time: Res<Time>,
    mut queue: ResMut<ToastQueue>,
    mut panel_q: Query<(&mut Visibility, &mut BackgroundColor, &mut ToastTimer), With<ToastPanel>>,
    mut text_q: Query<&mut Text, With<ToastText>>,
) {
    let dt = time.delta_secs();
    let Ok((mut vis, mut bg, mut timer)) = panel_q.single_mut() else { return };
    let Ok(mut text) = text_q.single_mut() else { return };

    if timer.0 > 0.0 {
        timer.0 -= dt;
        let alpha = (timer.0 * 2.0).min(1.0);
        *bg = BackgroundColor(Color::srgba(0.05, 0.05, 0.08, alpha * 0.85));
        *vis = Visibility::Inherited;
        if timer.0 <= 0.0 {
            *vis = Visibility::Hidden;
            text.0.clear();
        }
    } else if let Some((msg, dur)) = queue.messages.pop_front() {
        text.0 = msg;
        timer.0 = dur;
        *vis = Visibility::Inherited;
    }
}
