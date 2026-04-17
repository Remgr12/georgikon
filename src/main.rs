mod camera;
mod chat;
mod db;
mod guild;
mod inventory;
mod network;
mod player;
mod ui;
mod world;

use bevy::prelude::*;
use clap::Parser;

use camera::CameraPlugin;
use chat::ChatPlugin;
use guild::GuildPlugin;
use inventory::InventoryPlugin;
use network::{ClientNetworkPlugin, ServerNetworkPlugin, SharedPlugin};
use player::PlayerPlugin;
use ui::UiPlugin;
use world::WorldPlugin;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Run as server
    #[arg(short, long)]
    server: bool,
    /// Run as client
    #[arg(short, long)]
    client: bool,

    /// Zulip API URL (e.g., https://your-server.zulipchat.com)
    #[arg(long)]
    zulip_url: Option<String>,
    /// Zulip Email
    #[arg(long)]
    zulip_email: Option<String>,
    /// Zulip API Key
    #[arg(long)]
    zulip_key: Option<String>,
}

fn main() {
    let args = Args::parse();

    let is_server = args.server || (!args.server && !args.client);
    let is_client = args.client;

    let mut app = App::new();

    let chat_plugin = ChatPlugin {
        url: args.zulip_url.clone(),
        email: args.zulip_email.clone(),
        key: args.zulip_key.clone(),
    };

    if is_server && !is_client {
        // Dedicated Server
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ServerNetworkPlugin);
        app.add_plugins(SharedPlugin);
        app.add_plugins(GuildPlugin);
        app.add_systems(Startup, server_setup);
    } else if is_client && !is_server {
        // Dedicated Client
        app.add_plugins(DefaultPlugins);
        app.add_plugins(ClientNetworkPlugin);
        app.add_plugins(SharedPlugin);
        app.add_plugins(GuildPlugin);
        app.add_plugins((WorldPlugin, PlayerPlugin, CameraPlugin, InventoryPlugin, UiPlugin, chat_plugin));
    } else {
        // Host (Server + Client)
        app.add_plugins(DefaultPlugins);
        app.add_plugins((ServerNetworkPlugin, ClientNetworkPlugin));
        app.add_plugins(SharedPlugin);
        app.add_plugins(GuildPlugin);
        app.add_plugins((WorldPlugin, PlayerPlugin, CameraPlugin, InventoryPlugin, UiPlugin, chat_plugin));
        app.add_systems(Startup, server_setup);
    }

    app.run();
}

fn server_setup() {
    println!("Server started!");
}
