mod camera;
mod db;
mod inventory;
mod network;
mod player;
mod ui;
mod world;

use bevy::prelude::*;
use clap::Parser;

use camera::CameraPlugin;
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
}

fn main() {
    let args = Args::parse();

    let is_server = args.server || (!args.server && !args.client); // Default to server if neither specified? Or maybe default to client. Let's default to single-player if possible, but the plan says "single binary modes".
    let is_client = args.client;

    let mut app = App::new();

    if is_server && !is_client {
        // Dedicated Server
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ServerNetworkPlugin);
        app.add_plugins(SharedPlugin);
        // Add server-specific logic here (e.g., db initialization)
        app.add_systems(Startup, server_setup);
    } else if is_client && !is_server {
        // Dedicated Client
        app.add_plugins(DefaultPlugins);
        app.add_plugins(ClientNetworkPlugin);
        app.add_plugins(SharedPlugin);
        app.add_plugins((WorldPlugin, PlayerPlugin, CameraPlugin, InventoryPlugin, UiPlugin));
    } else {
        // Host (Server + Client) - For easy testing
        app.add_plugins(DefaultPlugins);
        app.add_plugins((ServerNetworkPlugin, ClientNetworkPlugin));
        app.add_plugins(SharedPlugin);
        app.add_plugins((WorldPlugin, PlayerPlugin, CameraPlugin, InventoryPlugin, UiPlugin));
        app.add_systems(Startup, server_setup);
    }

    app.run();
}

fn server_setup() {
    // Initialize DB here later
    println!("Server started!");
}
