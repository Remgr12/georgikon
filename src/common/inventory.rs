use bevy::prelude::*;
use std::collections::HashMap;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemRegistry>()
            // Observers handle the three inventory-mutation events.
            .add_observer(handle_add_item)
            .add_observer(handle_remove_item)
            .add_observer(handle_equip_to_hotbar)
            // Spell cooldowns and casting are pure per-frame logic.
            .add_systems(Update, (tick_spell_cooldowns, handle_spell_cast));
    }
}

// ── Item Registry ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub name: String,
    pub color: Color,
}

/// Global registry mapping numeric item IDs to display definitions.
/// Populate in a startup system before any items are added.
#[derive(Resource, Default)]
pub struct ItemRegistry(pub HashMap<u32, ItemDef>);

impl ItemRegistry {
    pub fn register(&mut self, id: u32, name: impl Into<String>, color: Color) {
        self.0.insert(id, ItemDef { name: name.into(), color });
    }

    pub fn get(&self, id: u32) -> Option<&ItemDef> {
        self.0.get(&id)
    }
}

// ── Inventory ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ItemStack {
    pub item_id: u32,
    pub quantity: u32,
}

/// Unbounded flat inventory. Items with matching IDs are stacked automatically.
#[derive(Component, Default, Debug)]
pub struct Inventory {
    pub slots: Vec<ItemStack>,
}

impl Inventory {
    /// Add `quantity` of `item_id`, merging into an existing stack if present.
    pub fn add(&mut self, item_id: u32, quantity: u32) {
        if let Some(stack) = self.slots.iter_mut().find(|s| s.item_id == item_id) {
            stack.quantity += quantity;
        } else {
            self.slots.push(ItemStack { item_id, quantity });
        }
    }

    /// Remove `quantity` from slot `index`. Deletes the slot if fully depleted.
    /// Returns the amount actually removed.
    pub fn remove_from_slot(&mut self, slot_index: usize, quantity: u32) -> u32 {
        let Some(stack) = self.slots.get_mut(slot_index) else {
            return 0;
        };
        let removed = quantity.min(stack.quantity);
        stack.quantity -= removed;
        if stack.quantity == 0 {
            self.slots.remove(slot_index);
        }
        removed
    }
}

// ── Hotbar ────────────────────────────────────────────────────────────────────

pub const HOTBAR_SLOTS: usize = 6;

/// Maps hotbar positions to inventory slot indices.
/// `bindings[i] = Some(inv_idx)` → hotbar slot i displays `inventory.slots[inv_idx]`.
#[derive(Component, Debug)]
pub struct Hotbar {
    pub bindings: [Option<usize>; HOTBAR_SLOTS],
}

impl Default for Hotbar {
    fn default() -> Self {
        Self { bindings: [None; HOTBAR_SLOTS] }
    }
}

// ── Spells ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Spell {
    pub name: String,
    /// Keyboard key that triggers this spell.
    pub key: KeyCode,
    /// Full cooldown duration in seconds.
    pub cooldown_secs: f32,
    /// Remaining cooldown in seconds; 0 = ready to cast.
    pub remaining_cooldown: f32,
    /// Tint color displayed in the spell HUD slot.
    pub color: Color,
}

impl Spell {
    pub fn is_ready(&self) -> bool {
        self.remaining_cooldown <= 0.0
    }

    /// Returns the fraction of cooldown remaining: 1.0 = just cast, 0.0 = ready.
    pub fn cooldown_fraction(&self) -> f32 {
        if self.cooldown_secs <= 0.0 {
            return 0.0;
        }
        (self.remaining_cooldown / self.cooldown_secs).clamp(0.0, 1.0)
    }
}

/// Ordered list of spells attached to the player entity.
#[derive(Component, Default, Debug)]
pub struct SpellBook {
    pub spells: Vec<Spell>,
}

// ── Events ────────────────────────────────────────────────────────────────────
//
// In Bevy 0.18 events are observer-based.
// Trigger with:  commands.trigger(AddItemEvent { item_id: 2, quantity: 5 })
// Observe with:  app.add_observer(handler_fn)

/// Add items to the player's inventory at runtime.
#[derive(Event)]
pub struct AddItemEvent {
    pub item_id: u32,
    pub quantity: u32,
}

/// Remove items from a specific inventory slot.
#[derive(Event)]
pub struct RemoveItemEvent {
    pub slot_index: usize,
    pub quantity: u32,
}

/// Bind an inventory slot to a hotbar position.
#[derive(Event)]
pub struct EquipToHotbarEvent {
    pub inventory_slot: usize,
    pub hotbar_slot: usize,
}

// ── Observer handlers ─────────────────────────────────────────────────────────

fn handle_add_item(
    ev: On<AddItemEvent>,
    mut query: Query<&mut Inventory, With<crate::client::player::Player>>,
) {
    let Ok(mut inventory) = query.single_mut() else { return };
    inventory.add(ev.item_id, ev.quantity);
    info!("Inventory: added {}x item #{}", ev.quantity, ev.item_id);
}

fn handle_remove_item(
    ev: On<RemoveItemEvent>,
    mut query: Query<(&mut Inventory, &mut Hotbar), With<crate::client::player::Player>>,
) {
    let Ok((mut inventory, mut hotbar)) = query.single_mut() else { return };

    let prev_len = inventory.slots.len();
    let removed = inventory.remove_from_slot(ev.slot_index, ev.quantity);

    // If the slot was fully consumed, fix up hotbar bindings pointing past it.
    if inventory.slots.len() < prev_len {
        for binding in hotbar.bindings.iter_mut() {
            match *binding {
                Some(idx) if idx == ev.slot_index => *binding = None,
                Some(idx) if idx > ev.slot_index => *binding = Some(idx - 1),
                _ => {}
            }
        }
    }

    if removed > 0 {
        info!("Inventory: removed {}x from slot {}", removed, ev.slot_index);
    }
}

fn handle_equip_to_hotbar(
    ev: On<EquipToHotbarEvent>,
    mut query: Query<(&Inventory, &mut Hotbar), With<crate::client::player::Player>>,
) {
    let Ok((inventory, mut hotbar)) = query.single_mut() else { return };
    if ev.hotbar_slot < HOTBAR_SLOTS && ev.inventory_slot < inventory.slots.len() {
        hotbar.bindings[ev.hotbar_slot] = Some(ev.inventory_slot);
        info!(
            "Hotbar: position {} → inventory slot {}",
            ev.hotbar_slot, ev.inventory_slot
        );
    }
}

// ── Update systems ────────────────────────────────────────────────────────────

fn tick_spell_cooldowns(
    time: Res<Time>,
    mut query: Query<&mut SpellBook, With<crate::client::player::Player>>,
) {
    let Ok(mut spellbook) = query.single_mut() else { return };
    let dt = time.delta_secs();
    for spell in spellbook.spells.iter_mut() {
        if spell.remaining_cooldown > 0.0 {
            spell.remaining_cooldown = (spell.remaining_cooldown - dt).max(0.0);
        }
    }
}

fn handle_spell_cast(
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut SpellBook, With<crate::client::player::Player>>,
) {
    let Ok(mut spellbook) = query.single_mut() else { return };
    for spell in spellbook.spells.iter_mut() {
        if keys.just_pressed(spell.key) && spell.is_ready() {
            spell.remaining_cooldown = spell.cooldown_secs;
            info!("Cast: {}", spell.name);
            // TODO: commands.trigger(SpellCastEvent { name: spell.name.clone() })
        }
    }
}
