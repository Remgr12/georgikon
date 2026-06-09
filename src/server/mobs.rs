//! Mob spawning, simple AI, and melee combat resolution.
//!
//! Mobs are authoritative server entities replicated to clients via
//! `PlayerPosition` (position) and [`UnitVisual`] (nameplate + health bar).
//! Player melee hits are resolved here via the [`MeleeAttack`] event triggered
//! by `server::authority`; kills award loot + XP and emit [`MobKilled`].

use bevy::prelude::*;
use lightyear::prelude::{NetworkVisibility, Replicate};

use crate::common::inventory::Inventory;
use crate::common::mob::{UnitKind, UnitVisual};
use crate::common::stats::CharacterStats;
use crate::common::zone::{Zone, ZoneId};
use crate::net::PlayerPosition;
use crate::server::online::OnlinePlayers;
use crate::server::progression::AwardXp;

/// A player melee attack to be resolved against nearby mobs.
#[derive(Event)]
pub struct MeleeAttack {
    pub attacker_char: u64,
}

/// Emitted when a mob dies (for quest kill-credit).
#[derive(Event)]
pub struct MobKilled {
    pub killer_char: u64,
    pub kind: UnitKind,
}

const MELEE_RANGE: f32 = 3.0;
const MELEE_DAMAGE: f32 = 30.0;
const TARGET_WOLVES: usize = 4;
const TARGET_BOARS: usize = 3;
const TARGET_ELITE_WOLVES: usize = 1;

#[derive(Component)]
struct Mob {
    kind: UnitKind,
    home: Vec3,
    aggro: f32,
    attack_range: f32,
    attack_cd: f32,
    damage: f32,
    speed: f32,
}

#[derive(Resource)]
struct RespawnTimer(Timer);

pub struct MobServerPlugin;

impl Plugin for MobServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RespawnTimer(Timer::from_seconds(5.0, TimerMode::Repeating)));
        app.add_systems(Startup, spawn_initial_mobs);
        app.add_systems(Update, (maintain_population, mob_ai));
        app.add_observer(on_melee_attack);
    }
}

fn spawn_initial_mobs(mut commands: Commands) {
    for _ in 0..TARGET_WOLVES { spawn_mob(&mut commands, UnitKind::Wolf); }
    for _ in 0..TARGET_BOARS { spawn_mob(&mut commands, UnitKind::Boar); }
    for _ in 0..TARGET_ELITE_WOLVES { spawn_mob(&mut commands, UnitKind::EliteWolf); }
}

fn maintain_population(
    time: Res<Time>,
    mut timer: ResMut<RespawnTimer>,
    mut commands: Commands,
    mobs: Query<&Mob>,
) {
    if !timer.0.tick(time.delta()).just_finished() { return; }
    let (mut wolves, mut boars, mut elites) = (0, 0, 0);
    for m in mobs.iter() {
        match m.kind {
            UnitKind::Wolf => wolves += 1,
            UnitKind::Boar => boars += 1,
            UnitKind::EliteWolf => elites += 1,
            UnitKind::Player => {}
        }
    }
    for _ in wolves..TARGET_WOLVES { spawn_mob(&mut commands, UnitKind::Wolf); }
    for _ in boars..TARGET_BOARS { spawn_mob(&mut commands, UnitKind::Boar); }
    for _ in elites..TARGET_ELITE_WOLVES { spawn_mob(&mut commands, UnitKind::EliteWolf); }
}

fn spawn_mob(commands: &mut Commands, kind: UnitKind) {
    let home = Vec3::new(
        rand::random::<f32>() * 80.0 - 40.0,
        1.0,
        rand::random::<f32>() * 80.0 - 40.0,
    );
    let (max_health, level, damage, speed, aggro) = match kind {
        UnitKind::Wolf      => (60.0,  2, 8.0,  3.5, 18.0),
        UnitKind::Boar      => (90.0,  3, 12.0, 2.5, 14.0),
        UnitKind::EliteWolf => (200.0, 6, 18.0, 4.0, 24.0),
        UnitKind::Player    => (100.0, 1, 0.0,  0.0, 0.0),
    };
    let mut stats = CharacterStats::default();
    stats.health.max = max_health;
    stats.health.restore_full();

    commands.spawn((
        Mob {
            kind,
            home,
            aggro,
            attack_range: 2.0,
            attack_cd: 0.0,
            damage,
            speed,
        },
        PlayerPosition(home),
        Zone(ZoneId::Overworld),
        stats.clone(),
        UnitVisual {
            kind,
            name: kind.display_name().to_string(),
            level,
            health: stats.health.current,
            max_health: stats.health.max,
        },
        NetworkVisibility::default(),
        Replicate::default(),
    ));
}

#[allow(clippy::type_complexity)]
fn mob_ai(
    time: Res<Time>,
    mut mobs: Query<(&mut PlayerPosition, &mut Mob, &mut UnitVisual, &Zone), With<Mob>>,
    mut players: Query<(&PlayerPosition, &Zone, &mut CharacterStats), Without<Mob>>,
) {
    let dt = time.delta_secs();
    for (mut pos, mut mob, _visual, zone) in mobs.iter_mut() {
        mob.attack_cd = (mob.attack_cd - dt).max(0.0);

        // Find the nearest player in the same zone.
        let mut nearest: Option<(f32, Vec3)> = None;
        for (ppos, pzone, _) in players.iter() {
            if pzone.0 != zone.0 {
                continue;
            }
            let d = ppos.0.distance(pos.0);
            if nearest.map_or(true, |(best, _)| d < best) {
                nearest = Some((d, ppos.0));
            }
        }

        match nearest {
            // In aggro + attack range: bite on cooldown.
            Some((d, _)) if d <= mob.attack_range => {
                if mob.attack_cd <= 0.0 {
                    mob.attack_cd = 1.2;
                    for (pp, pz, mut stats) in players.iter_mut() {
                        if pz.0 == zone.0 && pp.0.distance(pos.0) <= mob.attack_range {
                            stats.health.current = (stats.health.current - mob.damage).max(0.0);
                            break;
                        }
                    }
                }
            }
            // In aggro range: chase.
            Some((d, ppos)) if d <= mob.aggro => {
                let dir = (ppos - pos.0).normalize_or_zero();
                pos.0 += dir * mob.speed * dt;
            }
            // Otherwise drift home.
            _ => {
                if pos.0.distance(mob.home) > 0.5 {
                    let dir = (mob.home - pos.0).normalize_or_zero();
                    pos.0 += dir * mob.speed * 0.5 * dt;
                }
            }
        }
    }
}

/// Resolve a player melee swing against the nearest mob.
fn on_melee_attack(
    trigger: On<MeleeAttack>,
    mut commands: Commands,
    online: Res<OnlinePlayers>,
    mut attackers: Query<(&PlayerPosition, &Zone, &mut Inventory)>,
    mut mobs: Query<
        (Entity, &PlayerPosition, &mut CharacterStats, &mut UnitVisual, &Zone),
        With<Mob>,
    >,
) {
    let attacker_char = trigger.event().attacker_char;
    let Some(game) = online.game_of(attacker_char) else {
        return;
    };
    let Ok((apos, azone, mut inv)) = attackers.get_mut(game) else {
        return;
    };
    let origin = apos.0;
    let zone = azone.0;

    // Nearest mob in range in the same zone.
    let mut best: Option<(Entity, f32)> = None;
    for (e, mpos, _, _, mzone) in mobs.iter() {
        if mzone.0 != zone {
            continue;
        }
        let d = mpos.0.distance(origin);
        if d <= MELEE_RANGE && best.map_or(true, |(_, bd)| d < bd) {
            best = Some((e, d));
        }
    }
    let Some((mob_entity, _)) = best else {
        return;
    };

    let Ok((_, _, mut stats, mut visual, _)) = mobs.get_mut(mob_entity) else {
        return;
    };
    stats.health.current = (stats.health.current - MELEE_DAMAGE).max(0.0);
    visual.health = stats.health.current;
    visual.max_health = stats.health.max;

    if stats.health.current <= 0.0 {
        let kind = visual.kind;
        // Loot.
        for (item_id, qty) in loot_for(kind) {
            inv.add(item_id, qty);
        }
        // XP + quest credit.
        commands.trigger(AwardXp {
            char_id: attacker_char,
            amount: xp_for(kind),
        });
        commands.trigger(MobKilled {
            killer_char: attacker_char,
            kind,
        });
        commands.entity(mob_entity).despawn();
    }
}

fn xp_for(kind: UnitKind) -> u64 {
    match kind {
        UnitKind::Wolf      => 30,
        UnitKind::Boar      => 50,
        UnitKind::EliteWolf => 150,
        UnitKind::Player    => 0,
    }
}

fn loot_for(kind: UnitKind) -> Vec<(u32, u32)> {
    let mut loot = Vec::new();
    match kind {
        UnitKind::Wolf => {
            loot.push((6, 1)); // Wolf Pelt
            if rand::random::<f32>() < 0.4 { loot.push((4, 1)); }  // Gold Coin
            if rand::random::<f32>() < 0.3 { loot.push((9, 1)); }  // Healing Herb
        }
        UnitKind::Boar => {
            loot.push((7, 1)); // Boar Tusk
            if rand::random::<f32>() < 0.5 { loot.push((4, 2)); }  // Gold Coins
            if rand::random::<f32>() < 0.4 { loot.push((9, 1)); }  // Healing Herb
        }
        UnitKind::EliteWolf => {
            loot.push((6, 3)); // 3× Wolf Pelts
            loot.push((8, 2)); // Silver Coins
            loot.push((4, 5)); // Gold Coins
            if rand::random::<f32>() < 0.3 { loot.push((5, 1)); }  // rare: Magic Staff
        }
        UnitKind::Player => {}
    }
    loot
}
