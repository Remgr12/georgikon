//! Game-logic plugins: mood/atmosphere and dialogue.

pub mod dialogue;
pub mod mood;

use bevy::prelude::*;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((mood::MoodPlugin, dialogue::DialoguePlugin));
    }
}
