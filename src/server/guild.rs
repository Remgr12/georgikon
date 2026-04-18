use crate::common::social::{
    ChatNetMessage, GroupInviteMessage, GroupInviteResponseMessage, TradeRequestMessage,
};
use bevy::prelude::*;
use lightyear::prelude::AppMessageExt;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CreateGuildMessage(pub String);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GuildListMessage(pub Vec<String>);

pub struct GuildPlugin;

impl Plugin for GuildPlugin {
    fn build(&self, app: &mut App) {
        app.register_message::<CreateGuildMessage>();
        app.register_message::<GuildListMessage>();
        app.register_message::<ChatNetMessage>();
        app.register_message::<GroupInviteMessage>();
        app.register_message::<GroupInviteResponseMessage>();
        app.register_message::<TradeRequestMessage>();
    }
}
