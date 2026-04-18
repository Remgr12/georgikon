//! Dialogue system skeleton – ready for bevy_yarnspinner integration.
//!
//! To wire up full Yarnspinner dialogue:
//! 1. Add `bevy_yarnspinner = "0.7"` to Cargo.toml.
//! 2. Uncomment the integration block below and add your `.yarn` asset files.
//! 3. Spawn a `DialogueRunner` on an NPC / player entity and call
//!    `runner.start_node("NodeName")` to begin a conversation.
//!
//! # Example (with yarnspinner enabled)
//! ```rust,no_run
//! fn spawn_npc_dialogue(
//!     mut commands: Commands,
//!     project: Res<YarnProject>,
//!     npc: Query<Entity, With<Npc>>,
//! ) {
//!     for entity in &npc {
//!         let mut runner = project.create_dialogue_runner(&mut commands);
//!         runner.start_node("NpcGreeting");
//!         commands.entity(entity).insert(runner);
//!     }
//! }
//! ```

use bevy::prelude::*;

pub struct DialoguePlugin;

impl Plugin for DialoguePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_dialogue_started)
            .add_observer(on_dialogue_completed);

        // ── Yarnspinner integration (uncomment when dependency is added) ──────
        // app.add_plugins(
        //     YarnSpinnerPlugin::with_yarn_source(LoadYarnSource::from_folder("dialogue")),
        // )
        // .add_systems(
        //     Update,
        //     tick_cutscene_fade.run_if(any_with_component::<DialogueRunner>),
        // );
    }
}

/// Marks an entity that currently has an active dialogue session.
/// Suppress player movement / other input while this is present.
#[derive(Component)]
pub struct ActiveDialogue;

// ── Local event stubs ─────────────────────────────────────────────────────────
// These mirror bevy_yarnspinner's `DialogueStarted` / `DialogueCompleted`.
// Replace with the real types once the crate is added.

/// Fired when a dialogue session begins on an entity.
#[derive(Event)]
pub struct DialogueStarted;

/// Fired when a dialogue session ends on an entity.
#[derive(Event)]
pub struct DialogueCompleted;

// ── Observers ─────────────────────────────────────────────────────────────────

fn on_dialogue_started(_on: On<DialogueStarted>) {
    debug!("dialogue started");
}

fn on_dialogue_completed(_on: On<DialogueCompleted>) {
    debug!("dialogue completed");
}
