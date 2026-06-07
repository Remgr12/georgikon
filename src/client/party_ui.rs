//! Party frames (top-left, shown only while in a party). Actions via chat
//! slash-commands (see `client::commands`).

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::MessageReceiver;

use crate::common::party::{PartyInvitePushMessage, PartyStateMessage};

#[derive(Resource, Default)]
pub struct PartyStore {
    pub state: PartyStateMessage,
    /// (inviter char_id, inviter name)
    pub pending_invite: Option<(u64, String)>,
}

impl Default for PartyStateMessage {
    fn default() -> Self {
        PartyStateMessage {
            in_party: false,
            leader_char_id: 0,
            members: Vec::new(),
        }
    }
}

#[derive(Component)]
struct PartyFrameText;

pub struct PartyClientPlugin;

impl Plugin for PartyClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartyStore>();
        app.add_systems(Startup, spawn_frames);
        app.add_systems(Update, (recv_state, recv_invite, render_frames));
    }
}

fn spawn_frames(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(10.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.08, 0.05, 0.7)),
            GlobalZIndex(120),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                PartyFrameText,
            ));
        });
}

fn recv_state(
    mut store: ResMut<PartyStore>,
    mut q: Query<&mut MessageReceiver<PartyStateMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.state = msg;
        }
    }
}

fn recv_invite(
    mut store: ResMut<PartyStore>,
    mut q: Query<&mut MessageReceiver<PartyInvitePushMessage>, With<Client>>,
) {
    if let Ok(mut rx) = q.single_mut() {
        for msg in rx.receive() {
            store.pending_invite = Some((msg.from_char_id, msg.from_name));
        }
    }
}

fn render_frames(store: Res<PartyStore>, mut q: Query<&mut Text, With<PartyFrameText>>) {
    if !store.is_changed() {
        return;
    }
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    let mut s = String::new();
    if store.state.in_party {
        s.push_str("=== PARTY ===\n");
        for m in &store.state.members {
            let leader = if m.char_id == store.state.leader_char_id {
                "★ "
            } else {
                "  "
            };
            s.push_str(&format!(
                "{}{} L{}  {:.0}/{:.0}\n",
                leader, m.name, m.level, m.health, m.max_health
            ));
        }
    }
    if let Some((_, name)) = &store.pending_invite {
        s.push_str(&format!("\n{} invites you — /paccept /pdecline\n", name));
    }
    text.0 = s;
}
