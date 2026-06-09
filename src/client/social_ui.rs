//! Social client UI: mailbox (toggle `O`) and mob rendering.

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::MessageReceiver;

use crate::client::chat::ChatState;
use crate::client::input::{ActionState, GameAction};
use crate::common::mail::{MailActionResultMessage, MailEntry, MailListMessage};
use crate::common::mob::{UnitKind, UnitVisual};
use crate::net::{TradePhaseNet, TradeStateMessage};

#[derive(Resource, Default)]
pub struct MailStore {
    pub entries: Vec<MailEntry>,
    pub last_message: String,
    open: bool,
}

/// Latest trade state from the server (None = not trading).
#[derive(Resource, Default)]
pub struct TradeStore {
    pub state: Option<TradeStateMessage>,
}

#[derive(Component)]
struct MailPanel;
#[derive(Component)]
struct MailPanelText;
#[derive(Component)]
struct MobVisual;
#[derive(Component)]
struct TradePanel;
#[derive(Component)]
struct TradePanelText;

pub struct SocialClientPlugin;

impl Plugin for SocialClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MailStore>();
        app.init_resource::<TradeStore>();
        app.add_systems(Startup, (spawn_mail_panel, spawn_trade_panel));
        app.add_systems(
            Update,
            (
                recv_mail_list,
                recv_mail_result,
                toggle_mail,
                render_mail,
                spawn_mob_visuals,
                recv_trade,
                render_trade,
            ),
        );
    }
}

fn spawn_mail_panel(mut commands: Commands) {
    commands
        .spawn((
            MailPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(60.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-180.0)),
                width: Val::Px(360.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.06, 0.06, 0.12, 0.92)),
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
                MailPanelText,
            ));
        });
}

fn recv_mail_list(
    mut store: ResMut<MailStore>,
    mut q: Query<&mut MessageReceiver<MailListMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.entries = msg.entries;
        }
    }
}

fn recv_mail_result(
    mut store: ResMut<MailStore>,
    mut q: Query<&mut MessageReceiver<MailActionResultMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.last_message = msg.message;
        }
    }
}

fn toggle_mail(
    actions: Res<ActionState>,
    chat: Res<ChatState>,
    mut store: ResMut<MailStore>,
    mut q: Query<&mut Visibility, With<MailPanel>>,
) {
    if chat.is_typing {
        return;
    }
    if actions.just_pressed(GameAction::ToggleSocial) {
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

fn render_mail(store: Res<MailStore>, mut q: Query<&mut Text, With<MailPanelText>>) {
    if !store.open || !store.is_changed() {
        return;
    }
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    let mut s = String::from("=== MAILBOX ===\n");
    if store.entries.is_empty() {
        s.push_str("(empty — try /mailbox)\n");
    }
    for e in &store.entries {
        s.push_str(&format!(
            "[{}]{} {} — {}\n   {}\n",
            e.id,
            if e.read { "" } else { " *" },
            e.from_name,
            e.subject,
            e.body,
        ));
    }
    s.push_str("\nCmds: /mailbox  /mail <name> <message>  /mailread <id>\n");
    if !store.last_message.is_empty() {
        s.push_str(&format!("> {}\n", store.last_message));
    }
    text.0 = s;
}

fn spawn_trade_panel(mut commands: Commands) {
    commands
        .spawn((
            TradePanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(60.0),
                right: Val::Px(10.0),
                width: Val::Px(320.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.10, 0.06, 0.06, 0.92)),
            GlobalZIndex(160),
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
                TradePanelText,
            ));
        });
}

fn recv_trade(
    mut store: ResMut<TradeStore>,
    mut q: Query<&mut MessageReceiver<TradeStateMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            // A Complete/Declined phase stays shown as the result until the next
            // trade replaces it.
            store.state = Some(msg);
        }
    }
}

fn render_trade(
    store: Res<TradeStore>,
    mut panel: Query<&mut Visibility, With<TradePanel>>,
    mut text_q: Query<&mut Text, With<TradePanelText>>,
) {
    if !store.is_changed() {
        return;
    }
    let Ok(mut vis) = panel.single_mut() else {
        return;
    };
    let Ok(mut text) = text_q.single_mut() else {
        return;
    };
    match &store.state {
        Some(s) => {
            *vis = Visibility::Inherited;
            let phase = match s.phase {
                TradePhaseNet::Mutate => "Editing",
                TradePhaseNet::Review => "Review — both must accept",
                TradePhaseNet::Complete => "COMPLETE ✓",
                TradePhaseNet::Declined => "Cancelled",
            };
            let fmt = |offer: &[(u32, u32)]| {
                if offer.is_empty() {
                    "  (nothing)\n".to_string()
                } else {
                    offer
                        .iter()
                        .map(|(id, q)| format!("  item {id} x{q}\n"))
                        .collect()
                }
            };
            text.0 = format!(
                "=== TRADE with {} ===\n[{}]\n\nYour offer {}:\n{}Their offer {}:\n{}\nCmds: /offer <id> <n> /unoffer <id> <n>\n      /tradeok /tradeno",
                s.partner_name,
                phase,
                if s.you_accepted { "(accepted)" } else { "" },
                fmt(&s.your_offer),
                if s.they_accepted { "(accepted)" } else { "" },
                fmt(&s.their_offer),
            );
        }
        None => {
            *vis = Visibility::Hidden;
            text.0.clear();
        }
    }
}

/// Spawn an isometric sprite for each replicated mob (non-player unit).
fn spawn_mob_visuals(
    mut commands: Commands,
    q: Query<(Entity, &UnitVisual), Added<UnitVisual>>,
) {
    for (entity, visual) in q.iter() {
        if visual.kind == UnitKind::Player {
            continue;
        }
        let kind = match visual.kind {
            UnitKind::Wolf => crate::client::sprite::SpriteKind::Wolf,
            UnitKind::Boar => crate::client::sprite::SpriteKind::Boar,
            UnitKind::EliteWolf => crate::client::sprite::SpriteKind::EliteWolf,
            UnitKind::Player => unreachable!(),
        };
        // AnimatedSprite tag → SpritePlugin::ensure_sprite_components adds Sprite.
        // project_iso (PostUpdate) sets Transform from PlayerPosition.
        commands.entity(entity).insert((
            MobVisual,
            crate::client::sprite::AnimatedSprite::new(kind),
            Transform::default(),
        ));
    }
}
