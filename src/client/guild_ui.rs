//! Guild panel (toggle with `G`). Display-only; actions are issued via chat
//! slash-commands handled in `client::commands`.

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::MessageReceiver;

use crate::client::chat::ChatState;
use crate::client::input::{ActionState, GameAction};
use crate::common::guild::*;

/// Client-side mirror of the player's guild state + transient feedback.
#[derive(Resource, Default)]
pub struct GuildStore {
    pub guilds: Vec<GuildInfo>,
    pub browse: Vec<GuildBrowseEntry>,
    /// Most recent unanswered invite: (guild_id, guild_name).
    pub pending_invite: Option<(u64, String)>,
    pub last_message: String,
    open: bool,
}

#[derive(Component)]
struct GuildPanel;
#[derive(Component)]
struct GuildPanelText;

pub struct GuildClientPlugin;

impl Plugin for GuildClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuildStore>();
        app.add_systems(Startup, spawn_panel);
        app.add_systems(
            Update,
            (
                recv_state,
                recv_list,
                recv_invite,
                recv_result,
                toggle_panel,
                render_panel,
            ),
        );
    }
}

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((
            GuildPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(60.0),
                right: Val::Px(10.0),
                width: Val::Px(360.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.1, 0.9)),
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
                GuildPanelText,
            ));
        });
}

fn recv_state(
    mut store: ResMut<GuildStore>,
    mut q: Query<&mut MessageReceiver<GuildStateMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.guilds = msg.guilds;
        }
    }
}

fn recv_list(
    mut store: ResMut<GuildStore>,
    mut q: Query<&mut MessageReceiver<GuildListMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.browse = msg.guilds;
        }
    }
}

fn recv_invite(
    mut store: ResMut<GuildStore>,
    mut q: Query<&mut MessageReceiver<GuildInvitePushMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.last_message = format!("{} invited you to {}", msg.from_name, msg.guild_name);
            store.pending_invite = Some((msg.guild_id, msg.guild_name));
        }
    }
}

fn recv_result(
    mut store: ResMut<GuildStore>,
    mut q: Query<&mut MessageReceiver<GuildActionResultMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.last_message = msg.message;
        }
    }
}

fn toggle_panel(
    actions: Res<ActionState>,
    chat: Res<ChatState>,
    mut store: ResMut<GuildStore>,
    mut q: Query<&mut Visibility, With<GuildPanel>>,
) {
    if chat.is_typing {
        return;
    }
    if actions.just_pressed(GameAction::ToggleGuild) {
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

fn render_panel(store: Res<GuildStore>, mut q: Query<&mut Text, With<GuildPanelText>>) {
    if !store.open || !store.is_changed() {
        return;
    }
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    let mut s = String::from("=== GUILDS ===\n");
    if store.guilds.is_empty() {
        s.push_str("(none)\n");
    }
    for g in &store.guilds {
        s.push_str(&format!(
            "[{}] {}  ({}{})\n",
            g.id,
            g.name,
            g.my_rank.as_str(),
            if g.exclusive { ", EXCLUSIVE" } else { "" },
        ));
        if !g.motd.is_empty() {
            s.push_str(&format!("  MOTD: {}\n", g.motd));
        }
        for m in &g.members {
            s.push_str(&format!(
                "   - {} ({}){}\n",
                m.name,
                m.rank.as_str(),
                if m.online { "" } else { " [offline]" },
            ));
        }
    }
    if let Some((_, name)) = &store.pending_invite {
        s.push_str(&format!("\nInvite pending: {} — /gaccept or /gdecline\n", name));
    }
    if !store.browse.is_empty() {
        s.push_str("\n--- Browse ---\n");
        for b in &store.browse {
            s.push_str(&format!(
                "[{}] {} ({} members){}\n",
                b.id,
                b.name,
                b.member_count,
                if b.exclusive { " EXCLUSIVE" } else { "" },
            ));
        }
    }
    s.push_str(
        "\nCmds: /gcreate <name> /ginvite <name> /gaccept /gleave\n      /gexclusive on|off /gmotd <text> /glist\n",
    );
    if !store.last_message.is_empty() {
        s.push_str(&format!("\n> {}\n", store.last_message));
    }
    text.0 = s;
}
