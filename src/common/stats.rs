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
// Experience / leveling
// ---------------------------------------------------------------------------

/// Per-character progression. `xp` is total accumulated experience.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Experience {
    pub level: u32,
    pub xp: u64,
}

impl Default for Experience {
    fn default() -> Self {
        Self { level: 1, xp: 0 }
    }
}

impl Experience {
    /// Total xp required to have *reached* `level` (level 1 = 0 xp).
    /// Quadratic curve: 100 * (level-1)^2.
    pub fn xp_for_level(level: u32) -> u64 {
        let l = level.saturating_sub(1) as u64;
        100 * l * l
    }

    /// Total xp needed to reach the next level.
    pub fn xp_to_next(&self) -> u64 {
        Self::xp_for_level(self.level + 1)
    }

    /// Add xp and apply any level-ups. Returns the number of levels gained.
    pub fn add_xp(&mut self, amount: u64) -> u32 {
        self.xp += amount;
        let mut gained = 0;
        while self.xp >= Self::xp_for_level(self.level + 1) {
            self.level += 1;
            gained += 1;
        }
        gained
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::Experience;

    #[test]
    fn xp_curve_is_quadratic() {
        assert_eq!(Experience::xp_for_level(1), 0);
        assert_eq!(Experience::xp_for_level(2), 100);
        assert_eq!(Experience::xp_for_level(3), 400);
        assert_eq!(Experience::xp_for_level(4), 900);
    }

    #[test]
    fn add_xp_levels_up() {
        let mut e = Experience::default();
        assert_eq!(e.level, 1);
        // 100 xp reaches level 2 exactly.
        assert_eq!(e.add_xp(100), 1);
        assert_eq!(e.level, 2);
        // Jump straight past several levels at once.
        let gained = e.add_xp(800); // total 900 -> level 4
        assert_eq!(e.level, 4);
        assert_eq!(gained, 2);
    }

    #[test]
    fn xp_to_next_tracks_level() {
        let mut e = Experience::default();
        assert_eq!(e.xp_to_next(), 100);
        e.add_xp(100);
        assert_eq!(e.xp_to_next(), 400);
    }
}
