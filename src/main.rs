mod audio;
mod client;
mod common;
mod game;
mod net;
mod screens;
mod server;
mod settings;

use bevy::prelude::*;
use clap::Parser;

use audio::AudioPlugin;
use client::{CameraPlugin, ChatPlugin, ClientPlayerPlugin, InputPlugin, UiPlugin, WorldPlugin};
use common::inventory::InventoryPlugin;
use game::GamePlugin;
use net::{ClientNetworkPlugin, ServerNetworkPlugin, SharedPlugin};
use screens::ScreenPlugin;
use server::GuildPlugin;
use settings::SettingsPlugin;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    server: bool,
    #[arg(short, long)]
    client: bool,

    #[arg(long)]
    zulip_url: Option<String>,
    #[arg(long)]
    zulip_email: Option<String>,
    #[arg(long)]
    zulip_key: Option<String>,
}

fn main() {
    let args = Args::parse();

    let is_server = args.server;
    let is_client = args.client || (!args.server && !args.client);

    let mut app = App::new();

    let chat_plugin = ChatPlugin {
        url: args.zulip_url.clone(),
        email: args.zulip_email.clone(),
        key: args.zulip_key.clone(),
    };

    if is_server && !is_client {
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ServerNetworkPlugin);
        app.add_plugins(SharedPlugin);
        app.add_plugins(GuildPlugin);
        app.add_systems(Startup, server_setup);
    } else if is_client && !is_server {
        app.add_plugins(DefaultPlugins);
        // Foundation: settings and screen state machine first.
        app.add_plugins((SettingsPlugin, ScreenPlugin));
        app.add_plugins(InputPlugin);
        // Audio (bevy_seedling) must come after DefaultPlugins.
        app.add_plugins(AudioPlugin);
        app.add_plugins(ClientNetworkPlugin);
        app.add_plugins(SharedPlugin);
        app.add_plugins(GuildPlugin);
        app.add_plugins((
            WorldPlugin,
            ClientPlayerPlugin,
            CameraPlugin,
            InventoryPlugin,
            UiPlugin,
            chat_plugin,
            GamePlugin,
        ));
    } else {
        app.add_plugins(DefaultPlugins);
        app.add_plugins((SettingsPlugin, ScreenPlugin));
        app.add_plugins(InputPlugin);
        app.add_plugins(AudioPlugin);
        app.add_plugins((ServerNetworkPlugin, ClientNetworkPlugin));
        app.add_plugins(SharedPlugin);
        app.add_plugins(GuildPlugin);
        app.add_plugins((
            WorldPlugin,
            ClientPlayerPlugin,
            CameraPlugin,
            InventoryPlugin,
            UiPlugin,
            chat_plugin,
            GamePlugin,
        ));
        app.add_systems(Startup, server_setup);
    }

    app.run();
}

fn server_setup() {
    println!("Server started!");
}
