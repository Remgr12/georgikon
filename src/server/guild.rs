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
    }
}