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
use tracing_subscriber::EnvFilter;

use audio::AudioPlugin;
use client::{
    BuildPlugin, CameraPlugin, ChatPlugin, ClientCommandPlugin, ClientPlayerPlugin,
    ClientPredictionPlugin, ClientReconciliationPlugin, GuildClientPlugin, InputPlugin, LoginPlugin,
    MinimapPlugin, PartyClientPlugin, QuestClientPlugin, SocialClientPlugin, SpritePlugin, UiPlugin,
    WorldPlugin,
};
use common::inventory::InventoryPlugin;
use game::GamePlugin;
use net::{ClientNetworkPlugin, ServerNetworkPlugin, SharedPlugin};
use screens::ScreenPlugin;
use server::{
    AccountServerPlugin, ChatServerPlugin, GuildServerPlugin, IslandServerPlugin, MailServerPlugin,
    MobServerPlugin, PartyServerPlugin, ProgressionServerPlugin, QuestServerPlugin,
    ServerAuthorityPlugin, ServerSimPlugin, TradePlugin, ZoneServerPlugin,
};
use settings::SettingsPlugin;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    server: bool,
    #[arg(short, long)]
    client: bool,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let is_server = args.server;
    let is_client = args.client || (!args.server && !args.client);

    let mut app = App::new();

    if is_server && !is_client {
        // --- headless server ---
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ServerNetworkPlugin);
        app.add_plugins(SharedPlugin);
        add_server_gameplay_plugins(&mut app);
        app.add_systems(Startup, server_setup);
    } else if is_client && !is_server {
        // --- client only ---
        app.add_plugins(DefaultPlugins);
        app.add_plugins((SettingsPlugin, ScreenPlugin));
        app.add_plugins(InputPlugin);
        // Audio (bevy_seedling) must come after DefaultPlugins.
        app.add_plugins(AudioPlugin);
        app.add_plugins(ClientNetworkPlugin);
        app.add_plugins(SharedPlugin);
        add_client_plugins(&mut app);
    } else {
        // --- combined (default dev mode): server + client in one process ---
        app.add_plugins(DefaultPlugins);
        app.add_plugins((SettingsPlugin, ScreenPlugin));
        app.add_plugins(InputPlugin);
        app.add_plugins(AudioPlugin);
        app.add_plugins((ServerNetworkPlugin, ClientNetworkPlugin));
        app.add_plugins(SharedPlugin);
        add_server_gameplay_plugins(&mut app);
        add_client_plugins(&mut app);
        app.add_systems(Startup, server_setup);
    }

    app.run();
}

/// All authoritative server-side gameplay plugins (server + combined modes).
fn add_server_gameplay_plugins(app: &mut App) {
    app.add_plugins((
        ZoneServerPlugin,
        AccountServerPlugin,
        GuildServerPlugin,
        PartyServerPlugin,
        IslandServerPlugin,
        MobServerPlugin,
        QuestServerPlugin,
        ProgressionServerPlugin,
        MailServerPlugin,
        ServerSimPlugin,
        ServerAuthorityPlugin,
        TradePlugin,
        ChatServerPlugin,
    ));
}

/// All client-side rendering / UI / prediction plugins (client + combined modes).
fn add_client_plugins(app: &mut App) {
    app.add_plugins((
        SpritePlugin, // must come first so SpriteAssets is ready for other plugins
        WorldPlugin,
        ClientPlayerPlugin,
        CameraPlugin,
        InventoryPlugin,
        UiPlugin,
        MinimapPlugin,
        ChatPlugin,
        GamePlugin,
        LoginPlugin,
    ));
    app.add_plugins((
        GuildClientPlugin,
        PartyClientPlugin,
        BuildPlugin,
        QuestClientPlugin,
        SocialClientPlugin,
        ClientCommandPlugin,
        ClientPredictionPlugin,
        ClientReconciliationPlugin,
    ));
}

fn server_setup() {
    println!("Server started!");
}
