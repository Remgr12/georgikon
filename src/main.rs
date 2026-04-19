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
    CameraPlugin, ChatPlugin, ClientPlayerPlugin, ClientPredictionPlugin,
    ClientReconciliationPlugin, InputPlugin, UiPlugin, WorldPlugin,
};
use common::inventory::InventoryPlugin;
use game::GamePlugin;
use net::{ClientNetworkPlugin, ServerNetworkPlugin, SharedPlugin};
use screens::ScreenPlugin;
use server::{ChatServerPlugin, GuildPlugin, ServerAuthorityPlugin, ServerSimPlugin, TradePlugin, ZulipConfig};
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
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let is_server = args.server;
    let is_client = args.client || (!args.server && !args.client);

    let mut app = App::new();

    // Zulip credentials are used server-side only (bridge runs in ChatServerPlugin).
    let zulip_config = match (args.zulip_url, args.zulip_email, args.zulip_key) {
        (Some(url), Some(email), Some(key)) => Some(ZulipConfig { url, email, key }),
        _ => None,
    };

    if is_server && !is_client {
        // --- headless server ---
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ServerNetworkPlugin);
        app.add_plugins(SharedPlugin);
        app.add_plugins(GuildPlugin);
        // P1: authoritative simulation + authority enforcement
        app.add_plugins(ServerSimPlugin);
        app.add_plugins(ServerAuthorityPlugin);
        // P3: deterministic trade state machine
        app.add_plugins(TradePlugin);
        // P4: server-authoritative chat routing + optional Zulip bridge
        app.add_plugins(ChatServerPlugin { zulip: zulip_config });
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
        app.add_plugins(GuildPlugin);
        app.add_plugins((
            WorldPlugin,
            ClientPlayerPlugin,
            CameraPlugin,
            InventoryPlugin,
            UiPlugin,
            // Chat plugin is UI-only in pure client mode; Zulip runs server-side.
            ChatPlugin,
            GamePlugin,
        ));
        // P1: client-side prediction (sends intents) + reconciliation (applies snapshots)
        app.add_plugins(ClientPredictionPlugin);
        app.add_plugins(ClientReconciliationPlugin);
    } else {
        // --- combined (default dev mode) ---
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
            // Chat UI (sends/receives via network in combined mode).
            ChatPlugin,
            GamePlugin,
        ));
        // P1: server authority + client prediction/reconciliation in the same world
        app.add_plugins(ServerSimPlugin);
        app.add_plugins(ServerAuthorityPlugin);
        app.add_plugins(ClientPredictionPlugin);
        app.add_plugins(ClientReconciliationPlugin);
        // P3: trade state machine
        app.add_plugins(TradePlugin);
        // P4: server-authoritative chat routing + optional Zulip bridge
        app.add_plugins(ChatServerPlugin { zulip: zulip_config });
        app.add_systems(Startup, server_setup);
    }

    app.run();
}

fn server_setup() {
    println!("Server started!");
}
