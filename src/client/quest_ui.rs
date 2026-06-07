//! Quest log (toggle `L`) + level/XP HUD (top-center). Actions via chat
//! slash-commands (`/qaccept`, `/qturnin`, `/qabandon`).

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
}

impl Default for ProgressionStore {
    fn default() -> Self {
        Self {
            level: 1,
            xp: 0,
            xp_to_next: 100,
        }
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
        app.add_systems(Startup, (spawn_panel, spawn_xp_hud));
        app.add_systems(
            Update,
            (recv_log, recv_progression, toggle_panel, render_panel, render_xp),
        );
    }
}

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((
            QuestPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(60.0),
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
    mut q: Query<&mut MessageReceiver<QuestLogMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.quests = msg.quests;
        }
    }
}

fn recv_progression(
    mut store: ResMut<ProgressionStore>,
    mut q: Query<&mut MessageReceiver<ProgressionMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.level = msg.level;
            store.xp = msg.xp;
            store.xp_to_next = msg.xp_to_next;
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
    if !store.is_changed() {
        return;
    }
    if let Ok(mut text) = q.single_mut() {
        text.0 = format!("Lv {}  {}/{} XP", store.level, store.xp, store.xp_to_next);
    }
}
