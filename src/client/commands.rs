//! Client command dispatcher.
//!
//! Social/build/travel actions are issued as chat slash-commands. `chat` pushes
//! any non-chat `/command` line (sans slash) into [`CommandQueue`]; the systems
//! here parse them and emit the matching client→server messages. Centralizing
//! the senders here keeps the display panels free of networking.

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::MessageSender;

use crate::client::guild_ui::GuildStore;
use crate::client::party_ui::PartyStore;
use crate::common::guild::*;
use crate::common::island::{TravelRequestMessage, TravelTarget};
use crate::common::mail::{MarkMailReadMessage, RequestMailMessage, SendMailMessage};
use crate::common::party::*;
use crate::common::quest::{AbandonQuestMessage, AcceptQuestMessage, TurnInQuestMessage};
use crate::net::{
    ReliableChannel, TradeAcceptMessage, TradeDeclineMessage, TradeOfferUpdateMessage,
    TradeRequestNetMessage,
};

/// Queued command lines (leading `/` already stripped).
#[derive(Resource, Default)]
pub struct CommandQueue {
    pub lines: Vec<String>,
}

pub struct ClientCommandPlugin;

impl Plugin for ClientCommandPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandQueue>();
        app.add_systems(
            Update,
            (
                guild_commands,
                party_commands,
                quest_commands,
                mail_commands,
                clear_commands,
            )
                .chain(),
        );
    }
}

/// Split a command line into `(verb, rest)`.
fn split(line: &str) -> (&str, &str) {
    match line.split_once(char::is_whitespace) {
        Some((v, r)) => (v, r.trim()),
        None => (line, ""),
    }
}

fn guild_commands(
    queue: Res<CommandQueue>,
    mut store: ResMut<GuildStore>,
    mut create: Query<&mut MessageSender<CreateGuildMessage>, With<Client>>,
    mut invite: Query<&mut MessageSender<GuildInviteRequestMessage>, With<Client>>,
    mut respond: Query<&mut MessageSender<GuildInviteResponseMessage>, With<Client>>,
    mut leave: Query<&mut MessageSender<LeaveGuildMessage>, With<Client>>,
    mut settings: Query<&mut MessageSender<SetGuildSettingsMessage>, With<Client>>,
    mut motd: Query<&mut MessageSender<SetGuildMotdMessage>, With<Client>>,
    mut list: Query<&mut MessageSender<RequestGuildListMessage>, With<Client>>,
    mut disband: Query<&mut MessageSender<DisbandGuildMessage>, With<Client>>,
    mut travel: Query<&mut MessageSender<TravelRequestMessage>, With<Client>>,
) {
    let first_guild = store.guilds.first().map(|g| (g.id, g.exclusive, g.join_policy));
    for line in &queue.lines {
        let (verb, rest) = split(line);
        match verb {
            "gcreate" if !rest.is_empty() => {
                if let Ok(mut tx) = create.single_mut() {
                    tx.send::<ReliableChannel>(CreateGuildMessage { name: rest.into() });
                }
            }
            "ginvite" if !rest.is_empty() => {
                if let Some((gid, _, _)) = first_guild {
                    if let Ok(mut tx) = invite.single_mut() {
                        tx.send::<ReliableChannel>(GuildInviteRequestMessage {
                            guild_id: gid,
                            target_name: rest.into(),
                        });
                    }
                }
            }
            "gaccept" => {
                if let Some((gid, _)) = store.pending_invite.take() {
                    if let Ok(mut tx) = respond.single_mut() {
                        tx.send::<ReliableChannel>(GuildInviteResponseMessage {
                            guild_id: gid,
                            accept: true,
                        });
                    }
                }
            }
            "gdecline" => {
                if let Some((gid, _)) = store.pending_invite.take() {
                    if let Ok(mut tx) = respond.single_mut() {
                        tx.send::<ReliableChannel>(GuildInviteResponseMessage {
                            guild_id: gid,
                            accept: false,
                        });
                    }
                }
            }
            "gleave" => {
                if let Some((gid, _, _)) = first_guild {
                    if let Ok(mut tx) = leave.single_mut() {
                        tx.send::<ReliableChannel>(LeaveGuildMessage { guild_id: gid });
                    }
                }
            }
            "gdisband" => {
                if let Some((gid, _, _)) = first_guild {
                    if let Ok(mut tx) = disband.single_mut() {
                        tx.send::<ReliableChannel>(DisbandGuildMessage { guild_id: gid });
                    }
                }
            }
            "gexclusive" => {
                if let Some((gid, _, policy)) = first_guild {
                    let on = rest.eq_ignore_ascii_case("on");
                    if let Ok(mut tx) = settings.single_mut() {
                        tx.send::<ReliableChannel>(SetGuildSettingsMessage {
                            guild_id: gid,
                            exclusive: on,
                            join_policy: policy,
                        });
                    }
                }
            }
            "gmotd" => {
                if let Some((gid, _, _)) = first_guild {
                    if let Ok(mut tx) = motd.single_mut() {
                        tx.send::<ReliableChannel>(SetGuildMotdMessage {
                            guild_id: gid,
                            motd: rest.into(),
                        });
                    }
                }
            }
            "glist" => {
                if let Ok(mut tx) = list.single_mut() {
                    tx.send::<ReliableChannel>(RequestGuildListMessage);
                }
            }
            "visit" => {
                if let Some((gid, _, _)) = first_guild {
                    if let Ok(mut tx) = travel.single_mut() {
                        tx.send::<ReliableChannel>(TravelRequestMessage {
                            target: TravelTarget::GuildIsland(gid),
                        });
                    }
                }
            }
            "home" => {
                if let Ok(mut tx) = travel.single_mut() {
                    tx.send::<ReliableChannel>(TravelRequestMessage {
                        target: TravelTarget::Overworld,
                    });
                }
            }
            _ => {}
        }
    }
}

fn party_commands(
    queue: Res<CommandQueue>,
    mut store: ResMut<PartyStore>,
    mut invite: Query<&mut MessageSender<PartyInviteMessage>, With<Client>>,
    mut respond: Query<&mut MessageSender<PartyInviteResponseMessage>, With<Client>>,
    mut leave: Query<&mut MessageSender<LeavePartyMessage>, With<Client>>,
) {
    for line in &queue.lines {
        let (verb, rest) = split(line);
        match verb {
            "pinvite" if !rest.is_empty() => {
                if let Ok(mut tx) = invite.single_mut() {
                    tx.send::<ReliableChannel>(PartyInviteMessage {
                        target_name: rest.into(),
                    });
                }
            }
            "paccept" | "pdecline" => {
                store.pending_invite = None;
                if let Ok(mut tx) = respond.single_mut() {
                    tx.send::<ReliableChannel>(PartyInviteResponseMessage {
                        accept: verb == "paccept",
                    });
                }
            }
            "pleave" => {
                if let Ok(mut tx) = leave.single_mut() {
                    tx.send::<ReliableChannel>(LeavePartyMessage);
                }
            }
            _ => {}
        }
    }
}

fn quest_commands(
    queue: Res<CommandQueue>,
    mut accept: Query<&mut MessageSender<AcceptQuestMessage>, With<Client>>,
    mut abandon: Query<&mut MessageSender<AbandonQuestMessage>, With<Client>>,
    mut turnin: Query<&mut MessageSender<TurnInQuestMessage>, With<Client>>,
) {
    for line in &queue.lines {
        let (verb, rest) = split(line);
        let id: Option<u32> = rest.trim().parse().ok();
        match verb {
            "qaccept" => {
                if let (Some(id), Ok(mut tx)) = (id, accept.single_mut()) {
                    tx.send::<ReliableChannel>(AcceptQuestMessage { quest_id: id });
                }
            }
            "qturnin" => {
                if let (Some(id), Ok(mut tx)) = (id, turnin.single_mut()) {
                    tx.send::<ReliableChannel>(TurnInQuestMessage { quest_id: id });
                }
            }
            "qabandon" => {
                if let (Some(id), Ok(mut tx)) = (id, abandon.single_mut()) {
                    tx.send::<ReliableChannel>(AbandonQuestMessage { quest_id: id });
                }
            }
            _ => {}
        }
    }
}

fn mail_commands(
    queue: Res<CommandQueue>,
    mut send: Query<&mut MessageSender<SendMailMessage>, With<Client>>,
    mut request: Query<&mut MessageSender<RequestMailMessage>, With<Client>>,
    mut read: Query<&mut MessageSender<MarkMailReadMessage>, With<Client>>,
) {
    for line in &queue.lines {
        let (verb, rest) = split(line);
        match verb {
            "mailbox" => {
                if let Ok(mut tx) = request.single_mut() {
                    tx.send::<ReliableChannel>(RequestMailMessage);
                }
            }
            "mail" => {
                if let Some((to, body)) = rest.split_once(char::is_whitespace) {
                    if let Ok(mut tx) = send.single_mut() {
                        tx.send::<ReliableChannel>(SendMailMessage {
                            to_name: to.into(),
                            subject: "Message".into(),
                            body: body.trim().into(),
                        });
                    }
                }
            }
            "mailread" => {
                if let (Ok(id), Ok(mut tx)) = (rest.trim().parse::<u64>(), read.single_mut()) {
                    tx.send::<ReliableChannel>(MarkMailReadMessage { mail_id: id });
                }
            }
            _ => {}
        }
    }
}

fn clear_commands(mut queue: ResMut<CommandQueue>) {
    queue.lines.clear();
}
