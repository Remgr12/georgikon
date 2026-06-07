//! Mood / atmosphere system: crossfades music between Exploration and Combat.
//!
//! Trigger [`ChangeMood`] from anywhere (collision zones, combat events, …) to
//! switch the current mood.  Music is picked randomly from [`AudioSources`] and
//! continues as a looping playlist: when a track despawns bevy_seedling spawns
//! the next one automatically via the [`MusicPlaybacks::keep_playlist_playing`]
//! observer.
//!
//! # Populating playlists
//! ```rust,no_run
//! fn load_music(mut sources: ResMut<AudioSources>, server: Res<AssetServer>) {
//!     sources.explore.push(server.load("music/forest.ogg"));
//!     sources.explore.push(server.load("music/plains.ogg"));
//!     sources.combat.push(server.load("music/battle.ogg"));
//! }
//! ```

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_seedling::prelude::{
    AudioSample, MusicPool, SamplePlayer, Volume, VolumeNode, sample_effects,
};
use rand::prelude::IndexedRandom;
use tracing::{debug, trace};

use crate::audio::{FadeIn, FadeOut};
use crate::screens::Screen;
use crate::settings::Settings;

pub struct MoodPlugin;

impl Plugin for MoodPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Mood>()
            .init_resource::<AudioSources>()
            .init_resource::<MusicPlaybacks>()
            .add_systems(OnEnter(Screen::Gameplay), start_soundtrack)
            .add_systems(OnExit(Screen::Gameplay), stop_soundtrack)
            .add_observer(on_change_mood)
            .add_observer(MusicPlaybacks::track_entity)
            .add_observer(MusicPlaybacks::keep_playlist_playing);
    }
}

/// Current musical mood – also a state and a component on music entities.
#[derive(States, Default, Clone, Copy, Eq, PartialEq, Debug, Hash, Reflect, Component)]
#[reflect(Component)]
pub enum Mood {
    #[default]
    Exploration,
    Combat,
}

/// Trigger this event to crossfade music to a new mood.
#[derive(Event, Clone, Copy)]
pub struct ChangeMood(pub Mood);

/// Audio handles for each mood's playlist.
///
/// Populate this resource (e.g., in a loading system) before entering Gameplay.
/// The mood system silently skips spawning when a playlist is empty.
#[derive(Resource, Default)]
pub struct AudioSources {
    pub explore: Vec<Handle<AudioSample>>,
    pub combat: Vec<Handle<AudioSample>>,
}

/// Tracks which entity is currently playing music for each [`Mood`].
///
/// Used to resume / crossfade instead of spawning duplicates.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct MusicPlaybacks(HashMap<Mood, Entity>);

// ── Soundtrack start / stop ───────────────────────────────────────────────────

fn start_soundtrack(
    settings: Res<Settings>,
    mood: Res<State<Mood>>,
    music_pbs: Res<MusicPlaybacks>,
    mut commands: Commands,
    sources: Res<AudioSources>,
) {
    // Resume existing entity if we have one.
    if let Some(&pb) = music_pbs.get(mood.get()) {
        commands.entity(pb).insert(FadeIn);
        return;
    }

    // Otherwise spawn the first exploration track.
    let mut rng = rand::rng();
    let Some(handle) = sources.explore.choose(&mut rng).cloned() else {
        return; // No music files loaded yet – that's fine.
    };

    commands.spawn((
        MusicPool,
        SamplePlayer::new(handle)
            .with_volume(Volume::Linear(settings.music_volume()))
            .looping(),
        sample_effects![VolumeNode {
            volume: Volume::SILENT,
            ..default()
        }],
        Mood::default(),
        FadeIn,
    ));
}

fn stop_soundtrack(mut music: Query<&mut PlaybackSettings, With<MusicPool>>) {
    for mut pb in &mut music {
        pb.paused = true;
    }
}

// ── Mood change ───────────────────────────────────────────────────────────────

fn on_change_mood(
    on: On<ChangeMood>,
    settings: Res<Settings>,
    music_pbs: Res<MusicPlaybacks>,
    mut commands: Commands,
    sources: Res<AudioSources>,
    mut next_mood: ResMut<NextState<Mood>>,
) {
    let new_mood = on.event().0;
    let mut rng = rand::rng();

    // Fade out all currently playing tracks that are not the new mood.
    for (&mood, &track) in music_pbs.iter() {
        if mood != new_mood {
            commands.entity(track).insert(FadeOut);
        }
    }

    next_mood.set(new_mood);

    // Fade in existing track for the new mood if we have one.
    if let Some(&track) = music_pbs.get(&new_mood) {
        debug!("found existing {new_mood:?} track, fading in: {track}");
        commands.entity(track).insert(FadeIn);
        return;
    }

    // Otherwise spawn a new track.
    debug!("spawning new {new_mood:?} track");
    let handles = match new_mood {
        Mood::Exploration => &sources.explore,
        Mood::Combat => &sources.combat,
    };
    let Some(handle) = handles.choose(&mut rng).cloned() else {
        return;
    };

    commands.spawn((
        MusicPool,
        SamplePlayer::new(handle)
            .with_volume(Volume::Linear(settings.music_volume()))
            .looping(),
        sample_effects![VolumeNode {
            volume: Volume::SILENT,
            ..default()
        }],
        FadeIn,
        new_mood,
    ));
}

// ── MusicPlaybacks observers ──────────────────────────────────────────────────

impl MusicPlaybacks {
    /// When a SamplePlayer is added to an entity that also has a Mood component,
    /// register it so we can find / crossfade it later.
    fn track_entity(
        on: On<Add, SamplePlayer>,
        moods: Query<&Mood>,
        mut music_pbs: ResMut<MusicPlaybacks>,
    ) {
        if let Ok(&mood) = moods.get(on.entity) {
            trace!("tracking {mood:?} entity {}", on.entity);
            music_pbs.insert(mood, on.entity);
        }
    }

    /// When a SamplePlayer despawns (track finished), spawn the next one so the
    /// playlist continues uninterrupted.
    fn keep_playlist_playing(
        on: On<Despawn, SamplePlayer>,
        settings: Res<Settings>,
        mood: Res<State<Mood>>,
        mut commands: Commands,
        sources: Res<AudioSources>,
        mut music_pbs: ResMut<MusicPlaybacks>,
    ) {
        let current_mood = mood.get();
        let Some(&current_entity) = music_pbs.get(current_mood) else {
            return;
        };
        if current_entity != on.entity {
            return; // A different entity despawned – not our current track.
        }

        let mut rng = rand::rng();
        let handles = match current_mood {
            Mood::Exploration => &sources.explore,
            Mood::Combat => &sources.combat,
        };
        let Some(handle) = handles.choose(&mut rng).cloned() else {
            return;
        };

        debug!("continuing {current_mood:?} playlist ({current_entity} despawned)");
        let id = commands
            .spawn((
                MusicPool,
                SamplePlayer::new(handle).with_volume(Volume::Linear(settings.music_volume())),
                sample_effects![VolumeNode {
                    volume: Volume::SILENT,
                    ..default()
                }],
                FadeIn,
                *current_mood,
            ))
            .id();

        music_pbs.insert(*current_mood, id);
    }
}
