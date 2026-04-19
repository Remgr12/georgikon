use crate::common::stats::CharacterStats;
use crate::net::CombatIntentKind;

// ---------------------------------------------------------------------------
// Stamina costs per action
// ---------------------------------------------------------------------------

pub const ROLL_STAMINA_COST: f32 = 20.0;
pub const PRIMARY_STAMINA_COST: f32 = 10.0;
pub const SECONDARY_STAMINA_COST: f32 = 15.0;
pub const BLOCK_STAMINA_COST: f32 = 5.0;

// ---------------------------------------------------------------------------
// Transition guards
// ---------------------------------------------------------------------------

/// Returns `true` if the combat intent can be performed given the current stats.
///
/// Called server-side before applying any intent cost.
pub fn can_perform(kind: CombatIntentKind, stats: &CharacterStats) -> bool {
    match kind {
        CombatIntentKind::Roll => stats.stamina.current >= ROLL_STAMINA_COST,
        CombatIntentKind::Primary => stats.stamina.current >= PRIMARY_STAMINA_COST,
        CombatIntentKind::Secondary => stats.stamina.current >= SECONDARY_STAMINA_COST,
        CombatIntentKind::Block => stats.stamina.current >= BLOCK_STAMINA_COST,
    }
}

/// Deduct stat costs for performing the given intent.
///
/// Panics in debug builds if called without checking `can_perform` first.
pub fn apply_cost(kind: CombatIntentKind, stats: &mut CharacterStats) {
    match kind {
        CombatIntentKind::Roll => {
            stats.stamina.spend(ROLL_STAMINA_COST);
        }
        CombatIntentKind::Primary => {
            stats.stamina.spend(PRIMARY_STAMINA_COST);
        }
        CombatIntentKind::Secondary => {
            stats.stamina.spend(SECONDARY_STAMINA_COST);
        }
        CombatIntentKind::Block => {
            stats.stamina.spend(BLOCK_STAMINA_COST);
        }
    }
}
