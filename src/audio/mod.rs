//! Audio system: bevy_seedling with master / music / SFX buses.
//!
//! Three buses are set up by `SeedlingPlugin`:
//!   - `MainBus`          – master volume, affects everything
//!   - `MusicPool`        – routed to MainBus; use for background music
//!   - `SoundEffectsBus`  – routed to MainBus; use for SFX
//!
//! Volumes are initialised from [`crate::settings::Settings`] at startup.
//!
//! # Playing music
//! ```rust,no_run
//! use bevy_seedling::prelude::*;
//! use georgikon::audio::MusicPool;
//!
//! fn play_music(mut commands: Commands, server: Res<AssetServer>) {
//!     commands.spawn((
//!         MusicPool,
//!         SamplePlayer::new(server.load("music/theme.ogg")).looping(),
//!     ));
//! }
//! ```
//!
//! # Playing SFX
//! ```rust,no_run
//! use bevy_seedling::prelude::*;
//!
//! fn play_sfx(mut commands: Commands, server: Res<AssetServer>) {
//!     // DefaultPool is routed to SoundEffectsBus automatically.
//!     commands.spawn(SamplePlayer::new(server.load("sfx/hit.ogg")));
//! }
//! ```

use bevy::prelude::*;
pub use bevy_seedling::prelude::*;

mod fade;
pub use fade::{FadeIn, FadeOut};

/// Perceptual ↔ linear volume converter for [0.0, 1.0] slider values.
pub const CONVERTER: PerceptualVolume = PerceptualVolume::new();

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((SeedlingPlugin::default(), fade::plugin))
            .add_systems(Startup, setup_volumes);
    }
}

/// Apply initial volumes from Settings to the audio buses.
fn setup_volumes(
    mut master: Query<&mut VolumeNode, With<MainBus>>,
    settings: Res<crate::settings::Settings>,
) {
    for mut node in &mut master {
        node.volume = Volume::Linear(settings.sound.general);
    }
}
