mod camera;
mod db;
mod inventory;
mod player;
mod ui;
mod world;

use bevy::prelude::*;
use camera::CameraPlugin;
use inventory::InventoryPlugin;
use player::PlayerPlugin;
use ui::UiPlugin;
use world::WorldPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((WorldPlugin, PlayerPlugin, CameraPlugin, InventoryPlugin, UiPlugin))
        .run();
}
