mod camera;
mod player;
mod world;

use bevy::prelude::*;
use camera::CameraPlugin;
use player::PlayerPlugin;
use world::WorldPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((WorldPlugin, PlayerPlugin, CameraPlugin))
        .run();
}
