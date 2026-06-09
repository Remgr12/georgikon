//! Bulletproof 2-D isometric sprite system.
//!
//! ## How it works
//!
//! * Tag any entity with [`AnimatedSprite`] (specifying a [`SpriteKind`]).
//! * `ensure_sprite_components` automatically inserts a [`Sprite`] component
//!   using the matching placeholder image from [`SpriteAssets`].
//! * Dropping a PNG file into `assets/sprites/<kind>.png` at runtime causes
//!   `swap_loaded_sprites` to transparently replace the placeholder.
//! * `project_iso` (PostUpdate) reads every entity's [`PlayerPosition`] and
//!   writes its isometric screen [`Transform`].  Works for local player, remote
//!   players, mobs, and island objects without any per-system position logic.
//! * `animate_sprites` ticks the frame counter; `drive_anim_from_motion`
//!   derives `Facing` and `is_moving` from `PlayerPosition` delta.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::{
    client::iso::{REST_Y, iso_depth, world_to_screen},
    common::mob::{UnitKind, UnitVisual},
    net::PlayerPosition,
};

// ─── Shadow ───────────────────────────────────────────────────────────────────

/// Handle for the shared shadow ellipse image.
#[derive(Resource)]
pub struct ShadowImage(pub Handle<Image>);

/// Marks a shadow sprite; stores the entity it follows.
#[derive(Component)]
pub struct ShadowOf(pub Entity);

// ─── Health bars & name tags ──────────────────────────────────────────────────

/// Shared 1×1 white pixel image used for solid-colour health bar fills.
#[derive(Resource)]
pub struct BarPixel(pub Handle<Image>);

/// Tags the background rect of a floating health bar.
#[derive(Component)]
pub struct HealthBarBg(pub Entity);

/// Tags the foreground fill of a floating health bar.
#[derive(Component)]
pub struct HealthBarFg(pub Entity);

/// Tags the 2-D text that shows a unit's name.
#[derive(Component)]
pub struct NameTagOf(pub Entity);

/// Prevents double-spawning of health bars on the same entity.
#[derive(Component)]
pub struct HasHealthBar;

// ─── Speech bubbles ──────────────────────────────────────────────────────────

/// A speech bubble that follows an entity and fades out.
#[derive(Component)]
pub struct SpeechBubble {
    pub target: Entity,
    pub timer: f32,
}

// ─── Damage / heal number popups ─────────────────────────────────────────────

/// Tracks the previous health value for an entity so we can detect damage/heals.
#[derive(Component)]
struct PrevHealth(f32);

/// A floating damage/heal number that drifts upward and fades.
#[derive(Component)]
struct DamagePopup {
    timer: f32,
    vel_y: f32,
}

// ─── Public types ─────────────────────────────────────────────────────────────

/// Which logical entity type this sprite represents.
///
/// Determines the placeholder color and the sprite-sheet file name
/// (`assets/sprites/{kind_name}.png`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpriteKind {
    Player,
    Wolf,
    Boar,
    EliteWolf,
    /// Island build prop — tinted per-instance via [`Sprite::color`].
    IslandProp,
}

impl SpriteKind {
    fn asset_name(self) -> &'static str {
        match self {
            SpriteKind::Player => "player",
            SpriteKind::Wolf => "wolf",
            SpriteKind::Boar => "boar",
            SpriteKind::EliteWolf => "wolf", // reuse wolf sprite, tinted red
            SpriteKind::IslandProp => "island_prop",
        }
    }

    /// Default screen size (pixels) for a placeholder sprite of this kind.
    pub fn default_size(self) -> Vec2 {
        match self {
            SpriteKind::Player => Vec2::new(32.0, 56.0),
            SpriteKind::Wolf => Vec2::new(40.0, 30.0),
            SpriteKind::Boar => Vec2::new(44.0, 32.0),
            SpriteKind::EliteWolf => Vec2::new(52.0, 38.0), // larger
            SpriteKind::IslandProp => Vec2::new(32.0, 32.0),
        }
    }

    /// Solid-color used for the auto-generated placeholder image (RGBA).
    fn placeholder_color(self) -> [u8; 4] {
        match self {
            SpriteKind::Player => [80, 180, 80, 255],
            SpriteKind::Wolf => [110, 110, 130, 255],
            SpriteKind::Boar => [140, 90, 60, 255],
            SpriteKind::EliteWolf => [160, 50, 50, 255], // dark red — visually distinct
            SpriteKind::IslandProp => [200, 200, 200, 255],
        }
    }
}

/// Which direction the entity is facing in isometric view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Facing {
    #[default]
    SE = 0,
    SW = 1,
    NE = 2,
    NW = 3,
}

/// Animation and motion state attached to every visible entity.
///
/// Tag an entity with this component; [`SpritePlugin`] handles everything else.
#[derive(Component, Clone)]
pub struct AnimatedSprite {
    pub kind: SpriteKind,
    pub facing: Facing,
    pub is_moving: bool,
    /// Current animation frame (column in the atlas row).
    pub frame: usize,
    /// Elapsed seconds since the last frame advance.
    pub timer: f32,
    /// Previous world position — used by `drive_anim_from_motion`.
    prev_pos: Vec3,
}

impl AnimatedSprite {
    pub fn new(kind: SpriteKind) -> Self {
        Self {
            kind,
            facing: Facing::default(),
            is_moving: false,
            frame: 0,
            timer: 0.0,
            prev_pos: Vec3::ZERO,
        }
    }
}

// ─── SpriteAssets resource ────────────────────────────────────────────────────

/// Internal per-kind sprite sheet data.
struct SpriteEntry {
    /// Current image (placeholder or loaded sprite sheet).
    image: Handle<Image>,
    /// Atlas layout (1×1 for placeholder; N×M for real sheet).
    layout: Handle<TextureAtlasLayout>,
    /// Columns per animation row (frames per direction/state).
    frame_cols: u32,
    /// Rows per facing direction.
    facing_rows: u32,
}

/// Global sprite asset registry.  Initialised at startup by [`SpritePlugin`].
///
/// For each [`SpriteKind`] a solid-color placeholder is generated immediately
/// so the game is always renderable.  When a matching PNG arrives from the asset
/// server the placeholder is swapped out transparently.
#[derive(Resource)]
pub struct SpriteAssets {
    /// 1×1 white pixel — use with `Sprite::color` for solid-color tinting.
    pub white: Handle<Image>,
    entries: HashMap<SpriteKind, SpriteEntry>,
    /// Handles for real PNGs still being loaded (`kind → Handle<Image>`).
    pending: HashMap<SpriteKind, Handle<Image>>,
}

impl SpriteAssets {
    /// Get the image handle for a kind (placeholder or real sheet).
    pub fn image(&self, kind: SpriteKind) -> Handle<Image> {
        self.entries[&kind].image.clone()
    }

    /// Get the atlas layout handle for a kind.
    pub fn layout(&self, kind: SpriteKind) -> Handle<TextureAtlasLayout> {
        self.entries[&kind].layout.clone()
    }

    /// Atlas index for the given kind, state, and facing.
    pub fn atlas_index(&self, kind: SpriteKind, facing: Facing, frame: usize) -> usize {
        let entry = &self.entries[&kind];
        let row = (facing as u32) % entry.facing_rows;
        let col = (frame as u32) % entry.frame_cols;
        (row * entry.frame_cols + col) as usize
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Registers and drives the isometric sprite system.
pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_sprite_assets, setup_shadow_image, setup_bar_pixel))
            .add_systems(
                Update,
                (
                    ensure_sprite_components,
                    animate_sprites,
                    drive_anim_from_motion,
                    swap_loaded_sprites,
                    spawn_shadows,
                    update_shadows,
                    spawn_health_bars,
                    update_health_bars,
                    spawn_damage_popups,
                    update_damage_popups,
                    update_speech_bubbles,
                ),
            )
            .add_systems(PostUpdate, project_iso);
    }
}

// ─── Systems ──────────────────────────────────────────────────────────────────

/// Create placeholder images + kick off background loads for real sprite sheets.
fn setup_sprite_assets(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    asset_server: Res<AssetServer>,
) {
    // 1×1 white pixel (used for solid-color tinting on island props).
    let white = images.add(solid_image(32, 32, [255, 255, 255, 255]));

    let kinds = [
        SpriteKind::Player,
        SpriteKind::Wolf,
        SpriteKind::Boar,
        SpriteKind::EliteWolf,
        SpriteKind::IslandProp,
    ];

    let mut entries = HashMap::new();
    let mut pending = HashMap::new();

    for kind in kinds {
        let sz = kind.default_size();
        let w = sz.x as u32;
        let h = sz.y as u32;

        // Generate placeholder image.
        let ph_image = images.add(solid_image(w, h, kind.placeholder_color()));

        // 1×1 atlas layout wrapping the whole placeholder.
        let ph_layout = layouts.add(TextureAtlasLayout::from_grid(
            UVec2::new(w, h),
            1,
            1,
            None,
            None,
        ));

        entries.insert(
            kind,
            SpriteEntry {
                image: ph_image,
                layout: ph_layout,
                frame_cols: 1,
                facing_rows: 1,
            },
        );

        // Kick off an async load; silently ignored if the file doesn't exist.
        let path = format!("sprites/{}.png", kind.asset_name());
        let handle: Handle<Image> = asset_server.load(&path);
        pending.insert(kind, handle);
    }

    commands.insert_resource(SpriteAssets { white, entries, pending });
}

/// For every [`AnimatedSprite`] that lacks a [`Sprite`] component, insert one
/// using the current placeholder (or real sheet if already loaded).
fn ensure_sprite_components(
    mut commands: Commands,
    assets: Res<SpriteAssets>,
    q: Query<(Entity, &AnimatedSprite), (Added<AnimatedSprite>, Without<Sprite>)>,
) {
    for (entity, anim) in q.iter() {
        let kind = anim.kind;
        commands.entity(entity).insert(Sprite {
            image: assets.image(kind),
            texture_atlas: Some(TextureAtlas {
                layout: assets.layout(kind),
                index: 0,
            }),
            custom_size: Some(kind.default_size()),
            ..default()
        });
    }
}

/// Tick the animation frame counter for all animated sprites.
fn animate_sprites(
    time: Res<Time>,
    assets: Res<SpriteAssets>,
    mut q: Query<(&mut AnimatedSprite, &mut Sprite)>,
) {
    const WALK_FPS: f32 = 8.0;
    const IDLE_FPS: f32 = 4.0;
    let dt = time.delta_secs();

    for (mut anim, mut sprite) in q.iter_mut() {
        if anim.is_moving {
            anim.timer += dt;
            let frame_dur = 1.0 / WALK_FPS;
            if anim.timer >= frame_dur {
                anim.timer -= frame_dur;
                let cols = assets.entries[&anim.kind].frame_cols as usize;
                anim.frame = (anim.frame + 1) % cols;
            }
        } else {
            anim.timer += dt;
            let frame_dur = 1.0 / IDLE_FPS;
            if anim.timer >= frame_dur {
                anim.timer -= frame_dur;
                let cols = assets.entries[&anim.kind].frame_cols as usize;
                anim.frame = (anim.frame + 1) % cols;
            }
        }

        // Synchronise the atlas index.
        if let Some(ref mut atlas) = sprite.texture_atlas {
            atlas.index = assets.atlas_index(anim.kind, anim.facing, anim.frame);
        }
    }
}

/// Derive `facing` and `is_moving` from per-frame [`PlayerPosition`] delta.
///
/// Works uniformly for the local player, remote players, and mobs.
fn drive_anim_from_motion(mut q: Query<(&PlayerPosition, &mut AnimatedSprite)>) {
    for (pos, mut anim) in q.iter_mut() {
        let delta = pos.0 - anim.prev_pos;
        let xz_speed_sq = delta.x * delta.x + delta.z * delta.z;
        anim.is_moving = xz_speed_sq > 1e-6;

        if anim.is_moving {
            // Map XZ movement direction to the nearest iso facing.
            // ISO_FORWARD = (-1,0,-1): moving that way = facing SW
            // ISO_RIGHT   = (+1,0,-1): moving that way = facing SE
            anim.facing = classify_facing(delta.x, delta.z);
        }

        anim.prev_pos = pos.0;
    }
}

/// When a real sprite sheet finishes loading, swap it in for all entities of
/// that kind and rebuild the atlas layout for multi-frame animation.
fn swap_loaded_sprites(
    server: Res<AssetServer>,
    mut assets: ResMut<SpriteAssets>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    images: Res<Assets<Image>>,
    mut sprite_q: Query<(&AnimatedSprite, &mut Sprite)>,
) {
    let loaded_kinds: Vec<SpriteKind> = assets
        .pending
        .iter()
        .filter(|(_, h)| server.is_loaded(h.id()))
        .map(|(k, _)| *k)
        .collect();

    for kind in loaded_kinds {
        let handle = assets.pending.remove(&kind).unwrap();

        // Try to build a grid layout from the image dimensions.
        // Convention: sheet rows = 4 (one per Facing), columns = frame count.
        // If dimensions don't divide evenly, fall back to a 1×1 layout.
        let (frame_cols, facing_rows) = if let Some(img) = images.get(&handle) {
            let sz = kind.default_size();
            let cols = (img.width() / sz.x as u32).max(1);
            let rows = (img.height() / sz.y as u32).max(1);
            (cols, rows)
        } else {
            (1, 1)
        };

        let sz = kind.default_size();
        let new_layout = layouts.add(TextureAtlasLayout::from_grid(
            UVec2::new(sz.x as u32, sz.y as u32),
            frame_cols,
            facing_rows,
            None,
            None,
        ));

        // Update the canonical entry.
        let entry = assets.entries.get_mut(&kind).unwrap();
        entry.image = handle.clone();
        entry.layout = new_layout.clone();
        entry.frame_cols = frame_cols;
        entry.facing_rows = facing_rows;

        // Patch every live entity of this kind.
        for (anim, mut sprite) in sprite_q.iter_mut() {
            if anim.kind == kind {
                sprite.image = handle.clone();
                if let Some(ref mut atlas) = sprite.texture_atlas {
                    atlas.layout = new_layout.clone();
                    atlas.index = 0;
                }
            }
        }

        info!(
            "Sprite sheet loaded for {:?}: {}×{} frames",
            kind, frame_cols, facing_rows
        );
    }
}

/// (PostUpdate) Set every entity's isometric screen [`Transform`] from its
/// [`PlayerPosition`] (world space).
///
/// This is the single source of truth for 2-D entity positioning — no per-
/// system `sync_*` functions needed.
pub fn project_iso(mut q: Query<(&PlayerPosition, &mut Transform)>) {
    for (pos, mut tf) in q.iter_mut() {
        let xy = world_to_screen(pos.0);
        tf.translation.x = xy.x;
        tf.translation.y = xy.y;
        tf.translation.z = iso_depth(pos.0);
    }
}

// ─── Shadow systems ───────────────────────────────────────────────────────────

fn setup_shadow_image(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let img = shadow_ellipse(28, 14);
    commands.insert_resource(ShadowImage(images.add(img)));
}

fn spawn_shadows(
    mut commands: Commands,
    shadow_img: Res<ShadowImage>,
    q: Query<Entity, Added<AnimatedSprite>>,
) {
    for entity in q.iter() {
        commands.spawn((
            ShadowOf(entity),
            Sprite {
                image: shadow_img.0.clone(),
                custom_size: Some(Vec2::new(28.0, 14.0)),
                ..default()
            },
            Transform::default(),
        ));
    }
}

fn update_shadows(
    entities: Query<&PlayerPosition>,
    mut shadows: Query<(&ShadowOf, &mut Transform)>,
) {
    for (shadow_of, mut tf) in shadows.iter_mut() {
        if let Ok(pos) = entities.get(shadow_of.0) {
            let ground = Vec3::new(pos.0.x, REST_Y, pos.0.z);
            let s = world_to_screen(ground);
            tf.translation.x = s.x;
            tf.translation.y = s.y;
            tf.translation.z = iso_depth(ground) - 0.005;
        }
    }
}

// ─── Health bar & name tag systems ───────────────────────────────────────────

fn setup_bar_pixel(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let img = solid_image(1, 1, [255, 255, 255, 255]);
    commands.insert_resource(BarPixel(images.add(img)));
}

/// Spawn floating health bar + name tag for entities that have both AnimatedSprite and UnitVisual.
/// Handles two orderings: UnitVisual first (mobs) and AnimatedSprite first (remote players).
fn spawn_health_bars(
    mut commands: Commands,
    bar_pixel: Res<BarPixel>,
    // Case A: AnimatedSprite just added; UnitVisual already present
    q_anim: Query<(Entity, &UnitVisual), (Added<AnimatedSprite>, With<UnitVisual>, Without<HasHealthBar>)>,
    // Case B: UnitVisual just added; AnimatedSprite already present
    q_vis: Query<(Entity, &UnitVisual), (Added<UnitVisual>, With<AnimatedSprite>, Without<HasHealthBar>)>,
) {
    // Deduplicate in case an entity matches both triggers (same-frame add of both components).
    let mut seen = std::collections::HashSet::new();
    let all: Vec<(Entity, &UnitVisual)> = q_anim.iter()
        .chain(q_vis.iter())
        .filter(|(e, _)| seen.insert(*e))
        .collect();
    for (entity, visual) in all {
        let bar_w = 36.0_f32;
        let bar_h = 4.0_f32;
        commands.entity(entity).insert(HasHealthBar);
        // Background
        commands.spawn((
            HealthBarBg(entity),
            Sprite {
                image: bar_pixel.0.clone(),
                color: Color::srgba(0.08, 0.08, 0.08, 0.80),
                custom_size: Some(Vec2::new(bar_w, bar_h)),
                ..default()
            },
            Transform::default(),
        ));
        // Foreground fill
        commands.spawn((
            HealthBarFg(entity),
            Sprite {
                image: bar_pixel.0.clone(),
                color: Color::srgb(0.22, 0.80, 0.22),
                custom_size: Some(Vec2::new(bar_w, bar_h)),
                ..default()
            },
            Transform::default(),
        ));
        // Name tag — blue for players, orange for mobs (Veloren pattern)
        let name_color = match visual.kind {
            UnitKind::Player => Color::srgba(0.60, 0.80, 1.00, 0.95),
            UnitKind::EliteWolf => Color::srgba(1.00, 0.40, 0.40, 0.95),
            _ => Color::srgba(0.95, 0.78, 0.50, 0.90),
        };
        commands.spawn((
            NameTagOf(entity),
            Text2d::new(visual.name.clone()),
            TextFont { font_size: 9.0, ..default() },
            TextColor(name_color),
            Transform::default(),
        ));
    }
}

/// Update positions and fill-widths of all floating health bars.
fn update_health_bars(
    entities: Query<(&PlayerPosition, Option<&UnitVisual>, &AnimatedSprite)>,
    mut bg_q: Query<(&HealthBarBg, &mut Transform), (Without<HealthBarFg>, Without<NameTagOf>)>,
    mut fg_q: Query<(&HealthBarFg, &mut Transform, &mut Sprite), Without<HealthBarBg>>,
    mut name_q: Query<(&NameTagOf, &mut Transform, &mut Text2d), Without<HealthBarBg>>,
) {
    for (HealthBarBg(target), mut tf) in &mut bg_q {
        let Ok((pos, visual, anim)) = entities.get(*target) else { continue };
        let s = world_to_screen(pos.0);
        let lift = anim.kind.default_size().y * 0.5 + 10.0;
        let z = iso_depth(pos.0) + 0.8;
        tf.translation = Vec3::new(s.x, s.y + lift, z);
        let _ = visual; // used in fg_q for fg color
    }

    for (HealthBarFg(target), mut tf, mut sprite) in &mut fg_q {
        let Ok((pos, visual, anim)) = entities.get(*target) else { continue };
        let frac = visual
            .map(|v| if v.max_health > 0.0 { (v.health / v.max_health).clamp(0.0, 1.0) } else { 1.0 })
            .unwrap_or(1.0);
        let bar_w = 36.0_f32;
        let bar_h = 4.0_f32;
        let filled = bar_w * frac;
        let s = world_to_screen(pos.0);
        let lift = anim.kind.default_size().y * 0.5 + 10.0;
        let z = iso_depth(pos.0) + 0.801;
        tf.translation = Vec3::new(s.x - (bar_w - filled) * 0.5, s.y + lift, z);
        sprite.custom_size = Some(Vec2::new(filled.max(0.5), bar_h));
        // Veloren pattern: players get blue-tinted bars, mobs get red/orange
        sprite.color = match visual.map(|v| v.kind) {
            Some(UnitKind::Player) => player_health_color(frac),
            _ => health_color(frac),
        };
    }

    for (NameTagOf(target), mut tf, mut label) in &mut name_q {
        let Ok((pos, visual, anim)) = entities.get(*target) else { continue };
        let s = world_to_screen(pos.0);
        let lift = anim.kind.default_size().y * 0.5 + 16.0;
        let z = iso_depth(pos.0) + 0.802;
        tf.translation = Vec3::new(s.x, s.y + lift, z);
        if let Some(v) = visual {
            let prefix = match v.kind {
                UnitKind::EliteWolf => "⚡ ",  // elite marker
                _ => "",
            };
            let new_text = format!("{}{}  Lv{}", prefix, v.name, v.level);
            if label.0 != new_text { label.0 = new_text; }
        }
    }
}

/// Detect health changes on UnitVisual entities and spawn number popups.
fn spawn_damage_popups(
    mut commands: Commands,
    mut q: Query<(Entity, &UnitVisual, &PlayerPosition, Option<&mut PrevHealth>)>,
) {
    for (entity, visual, pos, prev_opt) in &mut q {
        match prev_opt {
            Some(mut prev) => {
                let delta = visual.health - prev.0;
                if delta < -0.5 {
                    // Damage
                    let s = world_to_screen(pos.0);
                    let z = iso_depth(pos.0) + 1.0;
                    let offset_x = (rand_jitter(entity) - 0.5) * 14.0;
                    commands.spawn((
                        DamagePopup { timer: 1.2, vel_y: 28.0 },
                        Text2d::new(format!("{:.0}", -delta)),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(Color::srgb(1.0, 0.22, 0.10)),
                        Transform::from_xyz(s.x + offset_x, s.y + 18.0, z),
                    ));
                } else if delta > 0.5 {
                    // Heal
                    let s = world_to_screen(pos.0);
                    let z = iso_depth(pos.0) + 1.0;
                    commands.spawn((
                        DamagePopup { timer: 1.0, vel_y: 22.0 },
                        Text2d::new(format!("+{:.0}", delta)),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(Color::srgb(0.25, 0.90, 0.35)),
                        Transform::from_xyz(s.x, s.y + 18.0, z),
                    ));
                }
                prev.0 = visual.health;
            }
            None => {
                commands.entity(entity).insert(PrevHealth(visual.health));
            }
        }
    }
}

fn update_damage_popups(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Transform, &mut TextColor, &mut DamagePopup)>,
) {
    let dt = time.delta_secs();
    for (entity, mut tf, mut tcolor, mut popup) in &mut q {
        popup.timer -= dt;
        tf.translation.y += popup.vel_y * dt;
        popup.vel_y = (popup.vel_y - 40.0 * dt).max(0.0);
        let alpha = (popup.timer / 0.6).clamp(0.0, 1.0);
        let c = tcolor.0.to_srgba();
        tcolor.0 = Color::srgba(c.red, c.green, c.blue, alpha);
        if popup.timer <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// Deterministic per-entity jitter [0, 1] for staggering damage numbers.
fn rand_jitter(entity: Entity) -> f32 {
    let bits = entity.to_bits();
    ((bits ^ (bits >> 13)) & 0xFF) as f32 / 255.0
}

fn update_speech_bubbles(
    mut commands: Commands,
    time: Res<Time>,
    entities: Query<(&PlayerPosition, Option<&AnimatedSprite>)>,
    mut bubbles: Query<(Entity, &mut SpeechBubble, &mut Transform, &mut TextColor)>,
) {
    let dt = time.delta_secs();
    for (entity, mut bubble, mut tf, mut tcolor) in &mut bubbles {
        bubble.timer -= dt;
        if bubble.timer <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let alpha = (bubble.timer / 1.5).clamp(0.0, 1.0);
        let c = tcolor.0.to_srgba();
        tcolor.0 = Color::srgba(c.red, c.green, c.blue, alpha);

        if let Ok((pos, anim)) = entities.get(bubble.target) {
            let lift = anim.map(|a| a.kind.default_size().y * 0.5 + 22.0).unwrap_or(36.0);
            let s = world_to_screen(pos.0);
            tf.translation.x = s.x;
            tf.translation.y = s.y + lift;
            tf.translation.z = iso_depth(pos.0) + 0.9;
        }
    }
}

/// Health bar color for mobs (green → yellow → red).
fn health_color(frac: f32) -> Color {
    if frac > 0.6 { Color::srgb(0.22, 0.80, 0.22) }
    else if frac > 0.3 { Color::srgb(0.85, 0.75, 0.10) }
    else { Color::srgb(0.85, 0.18, 0.18) }
}

/// Health bar color for remote players (blue-tinted, Veloren style).
fn player_health_color(frac: f32) -> Color {
    if frac > 0.6 { Color::srgb(0.25, 0.55, 0.95) }
    else if frac > 0.3 { Color::srgb(0.70, 0.55, 0.95) }
    else { Color::srgb(0.95, 0.25, 0.25) }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Create a solid-color `Image` of the given dimensions.
fn solid_image(width: u32, height: u32, rgba: [u8; 4]) -> Image {
    Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Generate a soft dark ellipse image for drop shadows (transparent background).
fn shadow_ellipse(w: u32, h: u32) -> Image {
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for py in 0..h {
        for px in 0..w {
            let nx = (px as f32 + 0.5 - cx) / cx;
            let ny = (py as f32 + 0.5 - cy) / cy;
            let d2 = nx * nx + ny * ny;
            if d2 <= 1.0 {
                let alpha = ((1.0 - d2.sqrt()) * 160.0) as u8;
                let i = ((py * w + px) * 4) as usize;
                // R,G,B = 0 (black), A = gradient alpha
                pixels[i + 3] = alpha;
            }
        }
    }
    Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Map a world-space XZ delta to the nearest iso [`Facing`].
#[inline]
fn classify_facing(dx: f32, dz: f32) -> Facing {
    // ISO_RIGHT  = (+1,0,-1): dx > 0, dz < 0 → SE
    // ISO_FORWARD = (-1,0,-1): dx < 0, dz < 0 → SW
    // Backwards of ISO_RIGHT  = (-1,0,+1): dx < 0, dz > 0 → NW
    // Backwards of ISO_FORWARD = (+1,0,+1): dx > 0, dz > 0 → NE
    match (dx >= 0.0, dz >= 0.0) {
        (true, false) => Facing::SE,
        (false, false) => Facing::SW,
        (true, true) => Facing::NE,
        (false, true) => Facing::NW,
    }
}
