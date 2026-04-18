//! Client HUD, pause menu, and settings modal.
//!
//! Layout
//! ──────
//!  - Hotbar      – bottom-right inventory strip, 6 slots
//!  - Spell HUD   – right-side vertical stack, up to 8 rows
//!  - Pause menu  – full-screen overlay, toggled by Escape in Gameplay
//!    └── Settings panel – volume and FOV controls

use bevy::prelude::*;
use bevy::ui::UiTargetCamera;
use bevy_seedling::prelude::*;

use crate::client::camera::SceneCamera;
use crate::client::player::Player;
use crate::common::inventory::{HOTBAR_SLOTS, Hotbar, Inventory, ItemRegistry, SpellBook};
use crate::screens::{GoTo, Screen};
use crate::settings::Settings;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (spawn_hotbar_ui, spawn_spell_hud, spawn_pause_menu),
        )
        .add_systems(PostStartup, bind_hud_to_scene_camera)
        .add_systems(
            Update,
            (
                update_hotbar_ui,
                init_spell_hud_slots,
                update_spell_hud_state,
                toggle_pause_menu.run_if(in_state(Screen::Gameplay)),
                handle_pause_buttons,
                update_volume_labels,
                update_fov_label,
                handle_setting_controls,
                save_settings_on_click,
            ),
        );
    }
}

// ── Layout constants ──────────────────────────────────────────────────────────

const SLOT_SIZE: f32 = 64.0;
const SLOT_GAP: f32 = 4.0;
const SPELL_W: f32 = 192.0;
const SPELL_H: f32 = 46.0;
pub const MAX_SPELL_SLOTS: usize = 8;

// ── Hotbar markers ────────────────────────────────────────────────────────────

#[derive(Component)]
struct HotbarSwatch(usize);
#[derive(Component)]
struct HotbarLabel(usize);
#[derive(Component)]
struct HotbarRoot;

// ── Spell HUD markers ─────────────────────────────────────────────────────────

#[derive(Component)]
struct SpellSlot(usize);
#[derive(Component)]
struct CooldownOverlay(usize);
#[derive(Component)]
struct SpellLabel(usize);
#[derive(Component)]
struct SpellHudRoot;

// ── Pause menu markers ────────────────────────────────────────────────────────

/// Root of the pause-menu overlay.
#[derive(Component)]
struct PauseMenu;

/// Settings panel inside the pause menu.
#[derive(Component)]
struct SettingsPanel;

/// "Resume" button inside the pause menu.
#[derive(Component)]
struct ResumeButton;

/// "Settings" toggle button inside the pause menu.
#[derive(Component)]
struct SettingsButton;

/// "Quit to Title" button inside the pause menu.
#[derive(Component)]
struct QuitButton;

/// "Save settings" button.
#[derive(Component)]
struct SaveButton;

/// "Back" button inside the settings panel (returns to pause menu root).
#[derive(Component)]
struct SettingsBackButton;

// ── Settings control markers ──────────────────────────────────────────────────

#[derive(Component, Clone, Copy)]
enum SettingControl {
    GeneralUp,
    GeneralDown,
    MusicUp,
    MusicDown,
    SfxUp,
    SfxDown,
    FovUp,
    FovDown,
}

// ── Volume / FOV label markers ────────────────────────────────────────────────

#[derive(Component)]
struct GeneralVolumeLabel;
#[derive(Component)]
struct MusicVolumeLabel;
#[derive(Component)]
struct SfxVolumeLabel;
#[derive(Component)]
struct FovLabel;

// ─────────────────────────────────────────────────────────────────────────────
// Hotbar UI
// ─────────────────────────────────────────────────────────────────────────────

fn spawn_hotbar_ui(mut commands: Commands) {
    commands
        .spawn((
            HotbarRoot,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                bottom: Val::Px(16.0),
                width: Val::Px(
                    (HOTBAR_SLOTS as f32 * SLOT_SIZE) + ((HOTBAR_SLOTS as f32 - 1.0) * SLOT_GAP),
                ),
                height: Val::Px(SLOT_SIZE + 8.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SLOT_GAP),
                ..default()
            },
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            for i in 0..HOTBAR_SLOTS {
                let key_label = if i < 9 {
                    format!("{}", i + 1)
                } else {
                    "0".into()
                };

                root.spawn((
                    Node {
                        width: Val::Px(SLOT_SIZE),
                        height: Val::Px(SLOT_SIZE),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(4.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.88)),
                    BorderColor::all(Color::srgba(0.35, 0.35, 0.48, 1.0)),
                ))
                .with_children(|slot| {
                    slot.spawn((
                        Text::new(key_label),
                        TextFont {
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.65, 0.65, 0.65, 1.0)),
                        Node {
                            align_self: AlignSelf::FlexStart,
                            ..default()
                        },
                    ));
                    slot.spawn((
                        HotbarSwatch(i),
                        Node {
                            width: Val::Px(34.0),
                            height: Val::Px(34.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.14, 0.14, 0.14, 1.0)),
                    ));
                    slot.spawn((
                        HotbarLabel(i),
                        Text::new(""),
                        TextFont {
                            font_size: 9.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.8, 0.8, 0.8, 1.0)),
                    ));
                });
            }
        });
}

fn update_hotbar_ui(
    player_q: Query<(&Inventory, &Hotbar), With<Player>>,
    registry: Res<ItemRegistry>,
    mut swatch_q: Query<(&HotbarSwatch, &mut BackgroundColor)>,
    mut label_q: Query<(&HotbarLabel, &mut Text)>,
) {
    let Ok((inventory, hotbar)) = player_q.single() else {
        return;
    };

    for (HotbarSwatch(i), mut bg) in &mut swatch_q {
        bg.0 = match hotbar.bindings[*i].and_then(|idx| inventory.slots.get(idx)) {
            Some(stack) => registry
                .get(stack.item_id)
                .map(|def| def.color)
                .unwrap_or(Color::srgba(0.5, 0.5, 0.5, 1.0)),
            None => Color::srgba(0.14, 0.14, 0.14, 1.0),
        };
    }

    for (HotbarLabel(i), mut text) in &mut label_q {
        text.0 = match hotbar.bindings[*i].and_then(|idx| inventory.slots.get(idx)) {
            Some(stack) => registry
                .get(stack.item_id)
                .map(|def| def.name.as_str())
                .unwrap_or("???")
                .to_string(),
            None => String::new(),
        };
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Spell HUD
// ─────────────────────────────────────────────────────────────────────────────

fn spawn_spell_hud(mut commands: Commands) {
    commands
        .spawn((
            SpellHudRoot,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(0.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(6.0),
                ..default()
            },
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            for i in 0..MAX_SPELL_SLOTS {
                root.spawn((
                    SpellSlot(i),
                    Node {
                        width: Val::Px(SPELL_W),
                        height: Val::Px(SPELL_H),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.88)),
                    BorderColor::all(Color::srgba(0.35, 0.35, 0.48, 1.0)),
                    Visibility::Hidden,
                ))
                .with_children(|slot| {
                    slot.spawn((
                        SpellLabel(i),
                        Text::new(""),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.92, 0.92, 0.92, 1.0)),
                    ));
                    slot.spawn((
                        Text::new(format!("F{}", i + 1)),
                        TextFont {
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.45, 0.65, 1.0, 1.0)),
                    ));
                    slot.spawn((
                        CooldownOverlay(i),
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(0.0),
                            left: Val::Px(0.0),
                            width: Val::Percent(100.0),
                            height: Val::Percent(0.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62)),
                    ));
                });
            }
        });
}

fn bind_hud_to_scene_camera(
    mut commands: Commands,
    camera_q: Query<Entity, With<SceneCamera>>,
    hotbar_q: Query<Entity, (With<HotbarRoot>, Without<UiTargetCamera>)>,
    spell_hud_q: Query<Entity, (With<SpellHudRoot>, Without<UiTargetCamera>)>,
) {
    let Ok(camera) = camera_q.single() else {
        return;
    };

    for entity in &hotbar_q {
        commands.entity(entity).insert(UiTargetCamera(camera));
    }

    for entity in &spell_hud_q {
        commands.entity(entity).insert(UiTargetCamera(camera));
    }
}

fn init_spell_hud_slots(
    player_q: Query<&SpellBook, With<Player>>,
    mut label_q: Query<(&SpellLabel, &mut Text)>,
    mut slot_q: Query<(&SpellSlot, &mut Visibility)>,
) {
    let Ok(spellbook) = player_q.single() else {
        return;
    };
    for (SpellLabel(i), mut text) in &mut label_q {
        text.0 = spellbook
            .spells
            .get(*i)
            .map(|s| s.name.clone())
            .unwrap_or_default();
    }
    for (SpellSlot(i), mut vis) in &mut slot_q {
        *vis = if spellbook.spells.get(*i).is_some() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn update_spell_hud_state(
    player_q: Query<&SpellBook, With<Player>>,
    mut overlay_q: Query<(&CooldownOverlay, &mut Node)>,
    mut border_q: Query<(&SpellSlot, &mut BorderColor)>,
) {
    let Ok(spellbook) = player_q.single() else {
        return;
    };
    for (CooldownOverlay(i), mut node) in &mut overlay_q {
        let fraction = spellbook
            .spells
            .get(*i)
            .map(|s| s.cooldown_fraction())
            .unwrap_or(0.0);
        let new_h = Val::Percent(fraction * 100.0);
        if node.height != new_h {
            node.height = new_h;
        }
    }
    for (SpellSlot(i), mut border) in &mut border_q {
        let Some(spell) = spellbook.spells.get(*i) else {
            continue;
        };
        let tint = if spell.is_ready() {
            let s = spell.color.to_srgba();
            Color::srgba(s.red, s.green, s.blue, 0.9)
        } else {
            Color::srgba(0.25, 0.25, 0.32, 1.0)
        };
        if border.top != tint {
            *border = BorderColor::all(tint);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pause menu
// ─────────────────────────────────────────────────────────────────────────────

fn spawn_pause_menu(mut commands: Commands, settings: Res<Settings>) {
    commands
        .spawn((
            PauseMenu,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
            GlobalZIndex(100),
        ))
        .with_children(|root| {
            // ── Main panel ────────────────────────────────────────────────────
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(32.0)),
                    row_gap: Val::Px(16.0),
                    min_width: Val::Px(280.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.97)),
                BorderColor::all(Color::srgba(0.35, 0.35, 0.48, 1.0)),
            ))
            .with_children(|panel| {
                // Title
                panel.spawn((
                    Text::new("PAUSED"),
                    TextFont {
                        font_size: 36.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));

                // Resume button
                panel.spawn(menu_button("Resume", ResumeButton));

                // Settings button
                panel.spawn(menu_button("Settings", SettingsButton));

                // Quit to Title button
                panel.spawn(menu_button("Quit to Title", QuitButton));

                // ── Settings sub-panel (hidden until "Settings" is clicked) ──
                panel
                    .spawn((
                        SettingsPanel,
                        Visibility::Hidden,
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Stretch,
                            row_gap: Val::Px(10.0),
                            padding: UiRect::all(Val::Px(16.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            width: Val::Px(340.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 1.0)),
                        BorderColor::all(Color::srgba(0.25, 0.25, 0.38, 1.0)),
                    ))
                    .with_children(|s| {
                        s.spawn((
                            Text::new("Settings"),
                            TextFont {
                                font_size: 20.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));

                        s.spawn(setting_row(
                            "General",
                            GeneralVolumeLabel,
                            SettingControl::GeneralDown,
                            SettingControl::GeneralUp,
                            &format!("{:.0}%", settings.sound.general * 100.0),
                        ));
                        s.spawn(setting_row(
                            "Music",
                            MusicVolumeLabel,
                            SettingControl::MusicDown,
                            SettingControl::MusicUp,
                            &format!("{:.0}%", settings.sound.music * 100.0),
                        ));
                        s.spawn(setting_row(
                            "SFX",
                            SfxVolumeLabel,
                            SettingControl::SfxDown,
                            SettingControl::SfxUp,
                            &format!("{:.0}%", settings.sound.sfx * 100.0),
                        ));
                        s.spawn(setting_row(
                            "FOV",
                            FovLabel,
                            SettingControl::FovDown,
                            SettingControl::FovUp,
                            &format!("{:.0}", settings.fov),
                        ));

                        // Save + Back row
                        s.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(8.0),
                            justify_content: JustifyContent::FlexEnd,
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn(small_button("Save", SaveButton));
                            row.spawn(small_button("Back", SettingsBackButton));
                        });
                    });
            });
        });
}

// ── Button builders ───────────────────────────────────────────────────────────

fn menu_button(label: &str, marker: impl Bundle) -> impl Bundle {
    (
        marker,
        Button,
        Node {
            width: Val::Px(220.0),
            height: Val::Px(42.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.15, 0.15, 0.22, 1.0)),
        BorderColor::all(Color::srgba(0.30, 0.30, 0.45, 1.0)),
        children![(
            Text::new(label),
            TextFont {
                font_size: 18.0,
                ..default()
            },
            TextColor(Color::WHITE),
        )],
    )
}

fn small_button(label: &str, marker: impl Bundle) -> impl Bundle {
    (
        marker,
        Button,
        Node {
            width: Val::Px(80.0),
            height: Val::Px(32.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.15, 0.15, 0.22, 1.0)),
        BorderColor::all(Color::srgba(0.30, 0.30, 0.45, 1.0)),
        children![(
            Text::new(label),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::WHITE),
        )],
    )
}

fn control_button(label: &str, control: SettingControl) -> impl Bundle {
    (
        control,
        Button,
        Node {
            width: Val::Px(28.0),
            height: Val::Px(28.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.18, 0.18, 0.26, 1.0)),
        BorderColor::all(Color::srgba(0.30, 0.30, 0.45, 1.0)),
        children![(
            Text::new(label),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::WHITE),
        )],
    )
}

/// Returns a label + [−] value [+] row bundle for use inside the settings panel.
fn setting_row<M: Bundle>(
    name: &'static str,
    label_marker: M,
    down: SettingControl,
    up: SettingControl,
    initial: &str,
) -> impl Bundle {
    let initial = initial.to_string();
    (
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: Val::Px(8.0),
            ..default()
        },
        children![
            (
                Text::new(name),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::srgba(0.75, 0.75, 0.75, 1.0)),
                Node {
                    width: Val::Px(80.0),
                    ..default()
                },
            ),
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                children![
                    control_button("−", down),
                    (
                        label_marker,
                        Text::new(initial),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        Node {
                            width: Val::Px(52.0),
                            justify_self: JustifySelf::Center,
                            ..default()
                        },
                    ),
                    control_button("+", up),
                ],
            ),
        ],
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Pause menu systems
// ─────────────────────────────────────────────────────────────────────────────

fn toggle_pause_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut pause_menu: Query<&mut Visibility, With<PauseMenu>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    let Ok(mut vis) = pause_menu.single_mut() else {
        return;
    };
    *vis = match *vis {
        Visibility::Hidden => Visibility::Visible,
        _ => Visibility::Hidden,
    };
}

fn handle_pause_buttons(
    resume_q: Query<&Interaction, (With<ResumeButton>, Changed<Interaction>)>,
    settings_q: Query<&Interaction, (With<SettingsButton>, Changed<Interaction>)>,
    quit_q: Query<&Interaction, (With<QuitButton>, Changed<Interaction>)>,
    back_q: Query<&Interaction, (With<SettingsBackButton>, Changed<Interaction>)>,
    mut pause_menu: Query<&mut Visibility, (With<PauseMenu>, Without<SettingsPanel>)>,
    mut settings_panel: Query<&mut Visibility, With<SettingsPanel>>,
    mut commands: Commands,
) {
    // Resume
    if resume_q.iter().any(|i| *i == Interaction::Pressed) {
        if let Ok(mut vis) = pause_menu.single_mut() {
            *vis = Visibility::Hidden;
        }
    }

    // Open settings sub-panel
    if settings_q.iter().any(|i| *i == Interaction::Pressed) {
        if let Ok(mut vis) = settings_panel.single_mut() {
            *vis = Visibility::Visible;
        }
    }

    // Close settings sub-panel (Back button inside settings)
    if back_q.iter().any(|i| *i == Interaction::Pressed) {
        if let Ok(mut vis) = settings_panel.single_mut() {
            *vis = Visibility::Hidden;
        }
    }

    // Quit to Title
    if quit_q.iter().any(|i| *i == Interaction::Pressed) {
        if let Ok(mut vis) = pause_menu.single_mut() {
            *vis = Visibility::Hidden;
        }
        commands.trigger(GoTo(Screen::Title));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Settings controls
// ─────────────────────────────────────────────────────────────────────────────

const VOLUME_STEP: f32 = 0.05;
const FOV_STEP: f32 = 5.0;
const FOV_MIN: f32 = 20.0;
const FOV_MAX: f32 = 120.0;

fn handle_setting_controls(
    q: Query<(&Interaction, &SettingControl), Changed<Interaction>>,
    mut settings: ResMut<Settings>,
    mut master_bus: Query<&mut VolumeNode, With<MainBus>>,
    mut music_bus: Query<&mut VolumeNode, (With<SamplerPool<MusicPool>>, Without<MainBus>)>,
    mut sfx_bus: Query<
        &mut VolumeNode,
        (
            With<SoundEffectsBus>,
            Without<MainBus>,
            Without<SamplerPool<MusicPool>>,
        ),
    >,
    mut camera: Query<&mut Projection, With<crate::client::camera::SceneCamera>>,
) {
    for (interaction, control) in &q {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match control {
            SettingControl::GeneralUp => {
                settings.sound.general = (settings.sound.general + VOLUME_STEP).min(1.0);
                if let Ok(mut node) = master_bus.single_mut() {
                    node.volume = Volume::Linear(settings.sound.general);
                }
            }
            SettingControl::GeneralDown => {
                settings.sound.general = (settings.sound.general - VOLUME_STEP).max(0.0);
                if let Ok(mut node) = master_bus.single_mut() {
                    node.volume = Volume::Linear(settings.sound.general);
                }
            }
            SettingControl::MusicUp => {
                settings.sound.music = (settings.sound.music + VOLUME_STEP).min(1.0);
                if let Ok(mut node) = music_bus.single_mut() {
                    node.volume = Volume::Linear(settings.music_volume());
                }
            }
            SettingControl::MusicDown => {
                settings.sound.music = (settings.sound.music - VOLUME_STEP).max(0.0);
                if let Ok(mut node) = music_bus.single_mut() {
                    node.volume = Volume::Linear(settings.music_volume());
                }
            }
            SettingControl::SfxUp => {
                settings.sound.sfx = (settings.sound.sfx + VOLUME_STEP).min(1.0);
                if let Ok(mut node) = sfx_bus.single_mut() {
                    node.volume = Volume::Linear(settings.sfx_volume());
                }
            }
            SettingControl::SfxDown => {
                settings.sound.sfx = (settings.sound.sfx - VOLUME_STEP).max(0.0);
                if let Ok(mut node) = sfx_bus.single_mut() {
                    node.volume = Volume::Linear(settings.sfx_volume());
                }
            }
            SettingControl::FovUp => {
                settings.fov = (settings.fov + FOV_STEP).min(FOV_MAX);
                apply_fov(&settings, &mut camera);
            }
            SettingControl::FovDown => {
                settings.fov = (settings.fov - FOV_STEP).max(FOV_MIN);
                apply_fov(&settings, &mut camera);
            }
        }
    }
}

fn apply_fov(
    settings: &Settings,
    camera: &mut Query<&mut Projection, With<crate::client::camera::SceneCamera>>,
) {
    if let Ok(mut proj) = camera.single_mut() {
        if let Projection::Perspective(ref mut p) = *proj {
            p.fov = settings.fov.to_radians();
        }
    }
}

fn save_settings_on_click(
    q: Query<&Interaction, (With<SaveButton>, Changed<Interaction>)>,
    settings: Res<Settings>,
) {
    if q.iter().any(|i| *i == Interaction::Pressed) {
        match settings.save() {
            Ok(()) => info!("settings saved to '{}'", crate::settings::SETTINGS_PATH),
            Err(e) => error!("failed to save settings: {e}"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Label update systems
// ─────────────────────────────────────────────────────────────────────────────

fn update_volume_labels(
    settings: Res<Settings>,
    mut general: Query<
        &mut Text,
        (
            With<GeneralVolumeLabel>,
            Without<MusicVolumeLabel>,
            Without<SfxVolumeLabel>,
        ),
    >,
    mut music: Query<
        &mut Text,
        (
            With<MusicVolumeLabel>,
            Without<GeneralVolumeLabel>,
            Without<SfxVolumeLabel>,
        ),
    >,
    mut sfx: Query<
        &mut Text,
        (
            With<SfxVolumeLabel>,
            Without<GeneralVolumeLabel>,
            Without<MusicVolumeLabel>,
        ),
    >,
) {
    if !settings.is_changed() {
        return;
    }
    if let Ok(mut t) = general.single_mut() {
        t.0 = format!("{:.0}%", settings.sound.general * 100.0);
    }
    if let Ok(mut t) = music.single_mut() {
        t.0 = format!("{:.0}%", settings.sound.music * 100.0);
    }
    if let Ok(mut t) = sfx.single_mut() {
        t.0 = format!("{:.0}%", settings.sound.sfx * 100.0);
    }
}

fn update_fov_label(settings: Res<Settings>, mut label: Query<&mut Text, With<FovLabel>>) {
    if !settings.is_changed() {
        return;
    }
    if let Ok(mut t) = label.single_mut() {
        t.0 = format!("{:.0}", settings.fov);
    }
}
