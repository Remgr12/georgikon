mod client;
mod common;
mod net;
mod server;

use bevy::prelude::*;
use clap::Parser;

use client::{CameraPlugin, ChatPlugin, UiPlugin, WorldPlugin, ClientPlayerPlugin};
use common::inventory::InventoryPlugin;
use net::{ClientNetworkPlugin, ServerNetworkPlugin, SharedPlugin};
use server::GuildPlugin;

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
        app.add_plugins(ClientNetworkPlugin);
        app.add_plugins(SharedPlugin);
        app.add_plugins(GuildPlugin);
        app.add_plugins((WorldPlugin, ClientPlayerPlugin, CameraPlugin, InventoryPlugin, UiPlugin, chat_plugin));
    } else {
        app.add_plugins(DefaultPlugins);
        app.add_plugins((ServerNetworkPlugin, ClientNetworkPlugin));
        app.add_plugins(SharedPlugin);
        app.add_plugins(GuildPlugin);
        app.add_plugins((WorldPlugin, ClientPlayerPlugin, CameraPlugin, InventoryPlugin, UiPlugin, chat_plugin));
        app.add_systems(Startup, server_setup);
    }

    app.run();
}

fn server_setup() {
    println!("Server started!");
}