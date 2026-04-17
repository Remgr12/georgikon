use bevy::prelude::*;
use bevy::ecs::observer::Trigger;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};
use crate::db;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CreateGuildMessage(pub String);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildListMessage(pub Vec<String>);

pub struct GuildPlugin;

impl Plugin for GuildPlugin {
    fn build(&self, app: &mut App) {
        app.register_message::<CreateGuildMessage>();
        app.register_message::<GuildListMessage>();

        app.add_observer(handle_create_guild);
        app.add_observer(handle_guild_list);
    }
}

// Server receives CreateGuildMessage
fn handle_create_guild(
    trigger: Trigger<lightyear::prelude::RemoteEvent<CreateGuildMessage>>,
) {
    let ev = trigger.event();
    println!("Server received CreateGuildMessage: {:?}", ev.trigger.0);
    if let Ok(conn) = crate::db::open() {
        let guild_name = &ev.trigger.0;
        let _ = conn.execute(
            "INSERT INTO guilds (name) VALUES (?1)",
            [guild_name],
        );
    }
}

// Client receives GuildListMessage
fn handle_guild_list(
    trigger: Trigger<lightyear::prelude::RemoteEvent<GuildListMessage>>,
) {
    let ev = trigger.event();
    println!("Client received GuildListMessage: {:?}", ev.trigger.0);
}
