//! FadeIn / FadeOut marker components and a timer-driven crossfade system.
//!
//! Attach `FadeIn` to a `SamplePlayer` entity to ramp its volume up to the
//! current music volume.  Attach `FadeOut` to ramp it down to silence then
//! pause playback.

use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;
use bevy_seedling::prelude::{
    AudioEvents, DurationSeconds, EffectsQuery, MusicPool, SampleEffects, Volume, VolumeFade,
    VolumeNode,
};
use std::time::Duration;
use tracing::debug;

use crate::settings::Settings;

const FADE_TIME: f64 = 0.5; // seconds

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        crossfade_music.run_if(on_timer(Duration::from_secs_f64(FADE_TIME))),
    );
}

/// Ramp this entity's volume up to the current music level.
#[derive(Component)]
pub struct FadeIn;

/// Ramp this entity's volume to silence, then pause playback.
#[derive(Component)]
pub struct FadeOut;

fn crossfade_music(
    settings: Res<Settings>,
    mut commands: Commands,
    mut pb_settings: Query<&mut PlaybackSettings, With<MusicPool>>,
    mut volume_nodes: Query<(&VolumeNode, &mut AudioEvents)>,
    mut fade_out: Query<(Entity, &SampleEffects), (With<FadeOut>, Without<FadeIn>)>,
    mut fade_in: Query<(Entity, &SampleEffects), (With<FadeIn>, Without<FadeOut>)>,
) {
    let fade_duration = DurationSeconds(FADE_TIME);

    for (e, effects) in &mut fade_out {
        let Ok((node, mut events)) = volume_nodes.get_effect_mut(effects) else {
            continue;
        };
        let Ok(mut pb) = pb_settings.get_mut(e) else {
            continue;
        };
        // Remove FadeIn to avoid fighting both fades on the same entity.
        commands.entity(e).remove::<FadeIn>();

        if node.volume.linear() <= 0.01 {
            commands.entity(e).remove::<FadeOut>();
            pb.paused = true;
            debug!("paused music, removed FadeOut: {e}");
            continue;
        }

        debug!("fading out: {e}");
        node.fade_to(Volume::SILENT, fade_duration, &mut events);
    }

    for (e, effects) in &mut fade_in {
        let Ok((node, mut events)) = volume_nodes.get_effect_mut(effects) else {
            continue;
        };
        let target = settings.music_volume();
        if node.volume.linear() >= target {
            commands.entity(e).remove::<FadeIn>();
            debug!("removed FadeIn: {e}");
            continue;
        }
        let Ok(mut pb) = pb_settings.get_mut(e) else {
            continue;
        };
        debug!("fading in: {e}");
        node.fade_to(Volume::Linear(target), fade_duration, &mut events);
        pb.paused = false;
    }
}
