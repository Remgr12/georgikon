use bevy::prelude::*;

// ---------------------------------------------------------------------------
// Bounded resource
// ---------------------------------------------------------------------------

/// A scalar resource with a current value, a maximum, and a per-second regen rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundedStat {
    pub current: f32,
    pub max: f32,
    /// Regeneration per second (may be 0 or negative for degeneration).
    pub regen_per_sec: f32,
}

impl BoundedStat {
    pub fn new(max: f32, regen_per_sec: f32) -> Self {
        Self {
            current: max,
            max,
            regen_per_sec,
        }
    }

    /// Regenerate/degenerate by `dt` seconds, clamping to [0, max].
    pub fn tick(&mut self, dt: f32) {
        self.current = (self.current + self.regen_per_sec * dt).clamp(0.0, self.max);
    }

    /// Consume `amount`, returning true if sufficient resources were available.
    pub fn spend(&mut self, amount: f32) -> bool {
        if self.current >= amount {
            self.current -= amount;
            true
        } else {
            false
        }
    }

    /// Fill to maximum.
    pub fn restore_full(&mut self) {
        self.current = self.max;
    }

    /// Fraction [0, 1] for UI display.
    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            0.0
        } else {
            (self.current / self.max).clamp(0.0, 1.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Character stats component
// ---------------------------------------------------------------------------

/// Per-entity authoritative character statistics.
///
/// Lives on both server (authoritative) and client (locally-estimated,
/// overwritten by server via `CombatStateMessage`).
#[derive(Component, Debug, Clone)]
pub struct CharacterStats {
    pub health: BoundedStat,
    pub energy: BoundedStat,
    pub stamina: BoundedStat,
}

impl Default for CharacterStats {
    fn default() -> Self {
        Self {
            health: BoundedStat::new(100.0, 2.0),   // 100 HP, 2 HP/s regen
            energy: BoundedStat::new(100.0, 5.0),   // 100 energy, 5/s regen
            stamina: BoundedStat::new(100.0, 15.0), // 100 stamina, 15/s regen
        }
    }
}

impl CharacterStats {
    /// Regenerate all stats by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        self.health.tick(dt);
        self.energy.tick(dt);
        self.stamina.tick(dt);
    }
}

// ---------------------------------------------------------------------------
// Stat bar UI markers (used by ui.rs)
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct HealthBar;

#[derive(Component)]
pub struct EnergyBar;

#[derive(Component)]
pub struct StaminaBar;
