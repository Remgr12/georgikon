//! Persistent game settings: audio volumes and FOV.
//!
//! Loaded from `assets/settings.ron` at startup with a graceful fallback to defaults.
//! Call `Settings::save()` to persist changes back to disk.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::{error::Error, fs};

pub const SETTINGS_PATH: &str = "assets/settings.ron";

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Settings>()
            .init_resource::<Settings>()
            .add_systems(Startup, load_settings);
    }
}

#[derive(Resource, Reflect, Deserialize, Serialize, Debug, Clone)]
#[reflect(Resource)]
pub struct Settings {
    pub sound: SoundSettings,
    /// Vertical field of view in degrees.
    pub fov: f32,
    pub gameplay: GameplaySettings,
}

#[derive(Reflect, Deserialize, Serialize, Debug, Clone)]
pub struct SoundSettings {
    /// Master volume multiplier [0.0, 1.0].
    pub general: f32,
    /// Music volume relative to master [0.0, 1.0].
    pub music: f32,
    /// Sound-effects volume relative to master [0.0, 1.0].
    pub sfx: f32,
}

#[derive(Reflect, Deserialize, Serialize, Debug, Clone)]
pub struct GameplaySettings {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub jump_force: f32,
    pub gravity: f32,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            general: 0.8,
            music: 0.8,
            sfx: 0.8,
        }
    }
}

impl Default for GameplaySettings {
    fn default() -> Self {
        Self {
            walk_speed: 5.0,
            sprint_speed: 8.0,
            jump_force: 7.0,
            gravity: 20.0,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sound: SoundSettings::default(),
            fov: 60.0,
            gameplay: GameplaySettings::default(),
        }
    }
}

impl Settings {
    /// Combined music volume (general × music).
    pub fn music_volume(&self) -> f32 {
        self.sound.general * self.sound.music
    }

    /// Combined SFX volume (general × sfx).
    pub fn sfx_volume(&self) -> f32 {
        self.sound.general * self.sound.sfx
    }

    pub fn read() -> Result<Self, Box<dyn Error>> {
        let content = fs::read_to_string(SETTINGS_PATH)?;
        Ok(ron::from_str(&content).unwrap_or_default())
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let content = ron::ser::to_string_pretty(self, Default::default())?;
        fs::write(SETTINGS_PATH, content)?;
        Ok(())
    }
}

fn load_settings(mut commands: Commands) {
    let settings = match Settings::read() {
        Ok(s) => {
            info!("loaded settings from '{SETTINGS_PATH}'");
            s
        }
        Err(e) => {
            info!("unable to load settings from '{SETTINGS_PATH}', using defaults: {e}");
            Default::default()
        }
    };
    commands.insert_resource(settings);
}
