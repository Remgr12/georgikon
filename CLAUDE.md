# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo run          # build and launch the game
cargo check        # fast type-check without linking
cargo build        # compile without running
```

No tests or linter configured yet.

## Architecture

Single-binary Bevy 0.18.1 game (`edition = "2024"`). Each concern is a `Plugin` in its own module, all wired together in `main.rs`.

| Module | Plugin | Responsibility |
|---|---|---|
| `camera.rs` | `CameraPlugin` | Third-person orbit camera (`ThirdPersonCamera` component, yaw/pitch/distance, cursor lock) |
| `player.rs` | `PlayerPlugin` | Player capsule, WASD movement relative to camera, item registry seeding, starter inventory/spellbook |
| `world.rs` | `WorldPlugin` | Ground plane + directional light |
| `inventory.rs` | `InventoryPlugin` | `Inventory`, `Hotbar`, `SpellBook` components; observer-based event handling |
| `ui.rs` | `UiPlugin` | Hotbar UI (bottom-center) + Spell HUD (bottom-right) |

### Bevy 0.18 API notes

**Events are observer-based** — no `EventReader`/`EventWriter`/`add_event`:
```rust
// register
app.add_observer(my_handler_fn);

// trigger
commands.trigger(MyEvent { .. });

// handler signature
fn my_handler_fn(ev: On<MyEvent>, query: Query<..>) { .. }
```

**UI components** that differ from older Bevy docs:
- `Node` holds all layout fields (no separate `Style`)
- `BorderColor::all(color)` — struct with `top/right/bottom/left`, not a tuple
- `Text(pub String)` — update via `text.0 = "...".to_string()`
- `BackgroundColor(Color)`, `TextFont`, `TextColor` are separate components

### Inventory / Hotbar / Spellbook pattern

`Inventory`, `Hotbar`, and `SpellBook` are **components on the `Player` entity**, not global resources. UI update systems query them via `Query<.., With<Player>>` each frame.

`Hotbar.bindings[i] = Some(inventory_slot_index)` — when a slot is fully removed from `Inventory`, `handle_remove_item` shifts all binding indices above it automatically.

Spell cooldowns tick every frame in `tick_spell_cooldowns`; casting is handled in `handle_spell_cast` (F1–F8 keys). The spell HUD cooldown overlay is a child `Node` whose `height: Val::Percent(fraction * 100.0)` is set each frame.

UI marker components (`HotbarSwatch(usize)`, `HotbarLabel(usize)`, `SpellSlot(usize)`, `CooldownOverlay(usize)`, `SpellLabel(usize)`) are private to `ui.rs` and used as query filters to identify specific UI nodes without storing entity IDs.
