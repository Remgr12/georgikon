//! Screen state machine: Gameplay (default) ↔ Title.
//!
//! Use `GoTo` and `Back` events to drive transitions.  The Title screen shows a
//! simple overlay; pressing Enter/Space returns to Gameplay.  ESC-in-Gameplay is
//! handled by UiPlugin (shows the pause menu) rather than here.

use bevy::prelude::*;

pub struct ScreenPlugin;

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Screen>()
            .add_observer(on_go_to)
            .add_observer(on_back)
            .add_systems(OnEnter(Screen::Title), spawn_title_ui)
            .add_systems(OnExit(Screen::Title), despawn_title_ui)
            .add_systems(Update, handle_title_input.run_if(in_state(Screen::Title)));
    }
}

/// The game's main screen states.
#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash, Reflect)]
pub enum Screen {
    /// Active gameplay – the default starting state.
    #[default]
    Gameplay,
    /// Title / main-menu screen.
    Title,
}

/// Trigger this to navigate to a specific screen.
#[derive(Event)]
pub struct GoTo(pub Screen);

/// Trigger this to return to the Title screen.
#[derive(Event)]
pub struct Back;

// ── Observers ────────────────────────────────────────────────────────────────

fn on_go_to(ev: On<GoTo>, mut next: ResMut<NextState<Screen>>) {
    next.set(ev.event().0.clone());
}

fn on_back(_: On<Back>, mut next: ResMut<NextState<Screen>>) {
    next.set(Screen::Title);
}

// ── Title screen ──────────────────────────────────────────────────────────────

/// Root of the title screen overlay.
#[derive(Component)]
struct TitleUi;

fn spawn_title_ui(mut commands: Commands) {
    commands
        .spawn((
            TitleUi,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(24.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.88)),
            GlobalZIndex(200),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("GEORGIKON"),
                TextFont {
                    font_size: 72.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            root.spawn((
                Text::new("Press Enter or Space to Play"),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::srgba(0.65, 0.65, 0.65, 1.0)),
            ));
        });
}

fn despawn_title_ui(mut commands: Commands, q: Query<Entity, With<TitleUi>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn handle_title_input(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        commands.trigger(GoTo(Screen::Gameplay));
    }
}
