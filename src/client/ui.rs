use bevy::prelude::*;
use crate::common::inventory::{Hotbar, Inventory, ItemRegistry, SpellBook, HOTBAR_SLOTS};
use crate::client::player::Player;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_hotbar_ui, spawn_spell_hud))
            .add_systems(Update, (
                // Only rebuild hotbar when inventory/hotbar data actually changes.
                update_hotbar_ui,
                // Run once when the player spawns to set static spell text.
                init_spell_hud_slots,
                // Per-frame: cooldown overlays + border tints, writes only on change.
                update_spell_hud_state,
            ));
    }
}

// ── Layout constants ──────────────────────────────────────────────────────────

const SLOT_SIZE: f32 = 64.0;
const SLOT_GAP: f32 = 4.0;
const SPELL_W: f32 = 192.0;
const SPELL_H: f32 = 46.0;
pub const MAX_SPELL_SLOTS: usize = 8;

// ── Hotbar marker components ──────────────────────────────────────────────────

/// Marks the colored item swatch inside hotbar slot `index`.
#[derive(Component)]
struct HotbarSwatch(usize);

/// Marks the item name text inside hotbar slot `index`.
#[derive(Component)]
struct HotbarLabel(usize);

// ── Spell HUD marker components ───────────────────────────────────────────────

/// Marks the spell slot container at `index`. Toggled visible/hidden.
#[derive(Component)]
struct SpellSlot(usize);

/// Marks the dark cooldown overlay inside spell slot `index`.
/// Its `Node::height` is animated between 0 % and 100 % to show remaining CD.
#[derive(Component)]
struct CooldownOverlay(usize);

/// Marks the spell name text inside spell slot `index`.
#[derive(Component)]
struct SpellLabel(usize);

// ── Hotbar UI ─────────────────────────────────────────────────────────────────

fn spawn_hotbar_ui(mut commands: Commands) {
    // Root: full-height column pinned to the right edge, slots centered vertically.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Px(SLOT_SIZE + 24.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(SLOT_GAP),
            ..default()
        })
        .with_children(|root| {
            for i in 0..HOTBAR_SLOTS {
                let key_label = if i < 9 { format!("{}", i + 1) } else { "0".into() };

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
                    // Key-binding label – top-left of the slot.
                    slot.spawn((
                        Text::new(key_label),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(Color::srgba(0.65, 0.65, 0.65, 1.0)),
                        Node {
                            align_self: AlignSelf::FlexStart,
                            ..default()
                        },
                    ));

                    // Item color swatch – center square.
                    slot.spawn((
                        HotbarSwatch(i),
                        Node {
                            width: Val::Px(34.0),
                            height: Val::Px(34.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.14, 0.14, 0.14, 1.0)),
                    ));

                    // Item name – bottom of the slot.
                    slot.spawn((
                        HotbarLabel(i),
                        Text::new(""),
                        TextFont { font_size: 9.0, ..default() },
                        TextColor(Color::srgba(0.8, 0.8, 0.8, 1.0)),
                    ));
                });
            }
        });
}

fn update_hotbar_ui(
    // Skip every frame where inventory and hotbar are unchanged.
    player_q: Query<(&Inventory, &Hotbar), (With<Player>, Or<(Changed<Inventory>, Changed<Hotbar>)>)>,
    registry: Res<ItemRegistry>,
    mut swatch_q: Query<(&HotbarSwatch, &mut BackgroundColor)>,
    mut label_q: Query<(&HotbarLabel, &mut Text)>,
) {
    let Ok((inventory, hotbar)) = player_q.single() else { return };

    for (HotbarSwatch(i), mut bg) in swatch_q.iter_mut() {
        bg.0 = match hotbar.bindings[*i].and_then(|idx| inventory.slots.get(idx)) {
            Some(stack) => registry
                .get(stack.item_id)
                .map(|def| def.color)
                .unwrap_or(Color::srgba(0.5, 0.5, 0.5, 1.0)),
            None => Color::srgba(0.14, 0.14, 0.14, 1.0),
        };
    }

    for (HotbarLabel(i), mut text) in label_q.iter_mut() {
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

// ── Spell HUD ─────────────────────────────────────────────────────────────────

fn spawn_spell_hud(mut commands: Commands) {
    // Root: column of spell rows, pinned to the bottom-right, clear of the hotbar column.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            right: Val::Px(SLOT_SIZE + 32.0), // clear of hotbar column
            bottom: Val::Px(16.0),
            flex_direction: FlexDirection::ColumnReverse,
            align_items: AlignItems::FlexEnd,
            row_gap: Val::Px(6.0),
            ..default()
        })
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
                    // Spell name – left side.
                    slot.spawn((
                        SpellLabel(i),
                        Text::new(""),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(Color::srgba(0.92, 0.92, 0.92, 1.0)),
                    ));

                    // Key-binding label – right side.
                    slot.spawn((
                        Text::new(format!("F{}", i + 1)),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(Color::srgba(0.45, 0.65, 1.0, 1.0)),
                    ));

                    // Cooldown overlay – spawned last so it renders on top.
                    // Grows from the top: height tracks remaining-CD fraction.
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

/// Runs once when `SpellBook` is first added (player spawn).
/// Sets static text that never changes at runtime.
fn init_spell_hud_slots(
    player_q: Query<&SpellBook, (With<Player>, Added<SpellBook>)>,
    mut label_q: Query<(&SpellLabel, &mut Text)>,
    mut slot_q: Query<(&SpellSlot, &mut Visibility)>,
) {
    let Ok(spellbook) = player_q.single() else { return };

    for (SpellLabel(i), mut text) in label_q.iter_mut() {
        text.0 = spellbook.spells.get(*i).map(|s| s.name.clone()).unwrap_or_default();
    }
    for (SpellSlot(i), mut vis) in slot_q.iter_mut() {
        *vis = if spellbook.spells.get(*i).is_some() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Runs every frame but only writes to ECS when values actually change,
/// avoiding spurious change-detection marks on `Node` and `BorderColor`.
fn update_spell_hud_state(
    player_q: Query<&SpellBook, With<Player>>,
    mut overlay_q: Query<(&CooldownOverlay, &mut Node)>,
    mut border_q: Query<(&SpellSlot, &mut BorderColor)>,
) {
    let Ok(spellbook) = player_q.single() else { return };

    for (CooldownOverlay(i), mut node) in overlay_q.iter_mut() {
        let fraction = spellbook.spells.get(*i).map(|s| s.cooldown_fraction()).unwrap_or(0.0);
        let new_h = Val::Percent(fraction * 100.0);
        if node.height != new_h {
            node.height = new_h;
        }
    }

    for (SpellSlot(i), mut border) in border_q.iter_mut() {
        let Some(spell) = spellbook.spells.get(*i) else { continue };
        let tint = if spell.is_ready() {
            let s = spell.color.to_srgba();
            Color::srgba(s.red, s.green, s.blue, 0.9)
        } else {
            Color::srgba(0.25, 0.25, 0.32, 1.0)
        };
        // Only write when the tint state has actually flipped.
        if border.top != tint {
            *border = BorderColor::all(tint);
        }
    }
}
