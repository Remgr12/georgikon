//! 2-D isometric terrain rendering.
//!
//! The overworld and guild-island floors are rendered as a coarse grid of
//! coloured tile sprites positioned via [`world_to_transform`].  Both zones
//! are always spawned; [`recv_zone_changed`] toggles visibility as before.
//!
//! Ground tiles always draw *below* entities: their Z uses `iso_depth - 100`,
//! keeping them well beneath entity sprites (whose Z is `iso_depth ≈ 0..0.2`).

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use lightyear::prelude::client::Client;
use lightyear::prelude::MessageReceiver;

use crate::client::input::{ActionState, GameAction};
use crate::client::iso::{TILE_HALF_H, TILE_HALF_W, iso_depth, world_to_screen};
use crate::client::player::Player;
use crate::common::island::ZoneChangedMessage;
use crate::common::zone::ZoneId;
use crate::net::PlayerPosition;

pub struct WorldPlugin;

// World dimensions (same as before — server logic depends on these).
const GROUND_SIZE: f32 = 100.0;
const ISLAND_SIZE: f32 = 250.0;
pub const GROUND_TOP_Y: f32 = 0.0;

/// Step between tile centres in world units.
const TILE_STEP_OVERWORLD: f32 = 4.0;
const TILE_STEP_ISLAND: f32 = 8.0;

/// Z offset applied to all terrain tiles so they draw under entity sprites.
const GROUND_DEPTH_OFFSET: f32 = -100.0;

/// The zone the local client currently occupies (drives build target + terrain).
#[derive(Resource, Clone, Copy)]
pub struct CurrentZone(pub ZoneId);

impl Default for CurrentZone {
    fn default() -> Self {
        CurrentZone(ZoneId::Overworld)
    }
}

#[derive(Component, Clone)]
struct OverworldTerrain;
#[derive(Component, Clone)]
struct IslandTerrain;

/// Marks a client-side NPC entity (not server-replicated).
#[derive(Component)]
struct NpcMarker;

/// Tags a floating text entity that follows an NPC; `y_offset` is the screen-space lift.
#[derive(Component)]
struct NpcTextOf {
    target: Entity,
    y_offset: f32,
}

/// The NPC's name for dialog purposes.
#[derive(Component)]
struct NpcName(String);

/// Bottom-center "Press E to interact" tooltip.
#[derive(Component)]
struct NpcInteractPrompt;

/// NPC interaction dialog overlay.
#[derive(Component)]
struct NpcDialog;

/// Text inside the NPC dialog.
#[derive(Component)]
struct NpcDialogText;

#[derive(Resource, Default)]
struct NpcInteractionState {
    /// Entity of the nearest NPC within range.
    near_npc: Option<Entity>,
    dialog_open: bool,
}

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentZone>();
        app.init_resource::<NpcInteractionState>();
        app.add_systems(Startup, (spawn_world, spawn_npcs, spawn_interaction_ui));
        app.add_systems(Update, recv_zone_changed);
        app.add_systems(Update, handle_npc_interaction.run_if(in_state(crate::screens::Screen::Gameplay)));
        app.add_systems(PostUpdate, update_npc_texts.after(crate::client::sprite::project_iso));
    }
}

fn spawn_world(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let tile_w_ow = (TILE_STEP_OVERWORLD * TILE_HALF_W * 2.0 + 2.0) as u32;
    let tile_h_ow = (TILE_STEP_OVERWORLD * TILE_HALF_H * 2.0 + 2.0) as u32;
    let overworld_tile_a = images.add(diamond_image(tile_w_ow, tile_h_ow, [76, 135, 76, 255]));
    let overworld_tile_b = images.add(diamond_image(tile_w_ow, tile_h_ow, [88, 150, 82, 255]));
    let path_tile = images.add(diamond_image(tile_w_ow, tile_h_ow, [155, 140, 100, 255]));

    let island_tile = images.add(diamond_image(
        (TILE_STEP_ISLAND * TILE_HALF_W * 2.0 + 2.0) as u32,
        (TILE_STEP_ISLAND * TILE_HALF_H * 2.0 + 2.0) as u32,
        [130, 115, 75, 255],
    ));

    // Block top faces and side faces for overworld landmarks.
    const BLOCK_TOP_W: u32 = 64;
    const BLOCK_TOP_H: u32 = 32;
    const BLOCK_FACE_H: f32 = 2.0 * crate::client::iso::Y_LIFT;
    let block_top = images.add(diamond_image(BLOCK_TOP_W, BLOCK_TOP_H, [155, 155, 165, 255]));
    let block_face = images.add(block_face_image(BLOCK_TOP_W, BLOCK_FACE_H as u32,
        [100, 100, 112, 255], [120, 120, 130, 255]));

    // Checkerboard tile grid for the overworld.
    let count = (GROUND_SIZE / TILE_STEP_OVERWORLD).ceil() as i32;
    let half = count as f32 * TILE_STEP_OVERWORLD * 0.5;
    let tw = tile_w_ow as f32;
    let th = tile_h_ow as f32;
    for xi in 0..=count {
        for zi in 0..=count {
            let x = xi as f32 * TILE_STEP_OVERWORLD - half;
            let z = zi as f32 * TILE_STEP_OVERWORLD - half;
            let w = Vec3::new(x, GROUND_TOP_Y, z);
            let xy = world_to_screen(w);
            // Path tiles along the main axes, checkerboard elsewhere.
            let is_path = xi == count / 2 || zi == count / 2;
            let tile = if is_path {
                path_tile.clone()
            } else if (xi + zi) % 2 == 0 {
                overworld_tile_a.clone()
            } else {
                overworld_tile_b.clone()
            };
            commands.spawn((
                OverworldTerrain,
                Sprite { image: tile, custom_size: Some(Vec2::new(tw, th)), ..default() },
                Transform::from_xyz(xy.x, xy.y, iso_depth(w) + GROUND_DEPTH_OFFSET),
                Visibility::Inherited,
            ));
        }
    }

    // Stone block landmarks around the map.
    for x in [-20.0_f32, 0.0, 20.0] {
        for z in [-20.0_f32, 20.0] {
            let base = Vec3::new(x, GROUND_TOP_Y, z);
            let top_world = Vec3::new(x, GROUND_TOP_Y + 2.0, z);
            let top_screen = world_to_screen(top_world);
            let base_z = iso_depth(base) + GROUND_DEPTH_OFFSET + 50.0;
            commands.spawn((
                OverworldTerrain,
                Sprite {
                    image: block_top.clone(),
                    custom_size: Some(Vec2::new(BLOCK_TOP_W as f32, BLOCK_TOP_H as f32)),
                    ..default()
                },
                Transform::from_xyz(top_screen.x, top_screen.y, base_z + 0.02),
            ));
            let face_y = top_screen.y - BLOCK_TOP_H as f32 / 2.0 - BLOCK_FACE_H / 2.0;
            commands.spawn((
                OverworldTerrain,
                Sprite {
                    image: block_face.clone(),
                    custom_size: Some(Vec2::new(BLOCK_TOP_W as f32, BLOCK_FACE_H)),
                    ..default()
                },
                Transform::from_xyz(top_screen.x, face_y, base_z + 0.01),
            ));
        }
    }

    // Trees: foliage diamond + trunk, scattered around the map.
    let tree_foliage = images.add(diamond_image(40, 32, [38, 110, 38, 255]));
    let tree_foliage_dark = images.add(diamond_image(32, 26, [26, 85, 26, 255]));
    let tree_trunk = images.add(block_face_image(10, 16, [90, 60, 30, 255], [110, 72, 36, 255]));
    let tree_positions: &[(f32, f32)] = &[
        (-35.0, -10.0), (-35.0, 10.0), (-35.0, -30.0), (-35.0, 30.0),
        (35.0, -10.0),  (35.0, 10.0),  (35.0, -30.0),  (35.0, 30.0),
        (-15.0, -40.0), (15.0, -40.0), (0.0, -40.0),
        (-15.0, 40.0),  (15.0, 40.0),  (0.0, 40.0),
        (-40.0, 0.0),   (40.0, 0.0),
        (-25.0, -25.0), (25.0, -25.0), (-25.0, 25.0), (25.0, 25.0),
    ];
    for &(x, z) in tree_positions {
        let base = Vec3::new(x, GROUND_TOP_Y, z);
        let foliage_world = Vec3::new(x, GROUND_TOP_Y + 4.0, z);
        let foliage_screen = world_to_screen(foliage_world);
        let trunk_world = Vec3::new(x, GROUND_TOP_Y + 1.5, z);
        let trunk_screen = world_to_screen(trunk_world);
        let tree_z = iso_depth(base) + GROUND_DEPTH_OFFSET + 60.0;

        // Trunk
        commands.spawn((
            OverworldTerrain,
            Sprite { image: tree_trunk.clone(), custom_size: Some(Vec2::new(10.0, 16.0)), ..default() },
            Transform::from_xyz(trunk_screen.x, trunk_screen.y, tree_z + 0.01),
        ));
        // Lower foliage layer (darker)
        commands.spawn((
            OverworldTerrain,
            Sprite { image: tree_foliage_dark.clone(), custom_size: Some(Vec2::new(32.0, 26.0)), ..default() },
            Transform::from_xyz(foliage_screen.x, foliage_screen.y - 8.0, tree_z + 0.02),
        ));
        // Upper foliage layer
        commands.spawn((
            OverworldTerrain,
            Sprite { image: tree_foliage.clone(), custom_size: Some(Vec2::new(40.0, 32.0)), ..default() },
            Transform::from_xyz(foliage_screen.x, foliage_screen.y, tree_z + 0.03),
        ));
    }

    spawn_tile_grid(
        &mut commands,
        IslandTerrain,
        Visibility::Hidden,
        island_tile,
        ISLAND_SIZE,
        TILE_STEP_ISLAND,
    );
}

/// Spawn a grid of tile sprites covering a square world area of `size × size`.
fn spawn_tile_grid<M: Component + Clone>(
    commands: &mut Commands,
    marker: M,
    visibility: Visibility,
    image: Handle<Image>,
    size: f32,
    step: f32,
) {
    let tile_w = step * TILE_HALF_W * 2.0 + 2.0;
    let tile_h = step * TILE_HALF_H * 2.0 + 2.0;

    let count = (size / step).ceil() as i32;
    let half = count as f32 * step * 0.5;

    for xi in 0..=count {
        for zi in 0..=count {
            let x = xi as f32 * step - half;
            let z = zi as f32 * step - half;
            let w = Vec3::new(x, GROUND_TOP_Y, z);
            let xy = world_to_screen(w);
            commands.spawn((
                marker.clone(),
                Sprite {
                    image: image.clone(),
                    custom_size: Some(Vec2::new(tile_w, tile_h)),
                    ..default()
                },
                Transform::from_xyz(xy.x, xy.y, iso_depth(w) + GROUND_DEPTH_OFFSET),
                visibility,
            ));
        }
    }
}

fn recv_zone_changed(
    mut current: ResMut<CurrentZone>,
    mut rx: Query<&mut MessageReceiver<ZoneChangedMessage>, With<Client>>,
    mut overworld: Query<&mut Visibility, (With<OverworldTerrain>, Without<IslandTerrain>)>,
    mut island: Query<&mut Visibility, (With<IslandTerrain>, Without<OverworldTerrain>)>,
) {
    let Ok(mut receiver) = rx.single_mut() else { return };
    let mut new_zone = None;
    for msg in receiver.receive() {
        new_zone = Some(msg.zone);
    }
    let Some(zone) = new_zone else { return };
    current.0 = zone;
    let on_island = matches!(zone, ZoneId::GuildIsland(_));
    for mut v in overworld.iter_mut() {
        *v = if on_island { Visibility::Hidden } else { Visibility::Inherited };
    }
    for mut v in island.iter_mut() {
        *v = if on_island { Visibility::Inherited } else { Visibility::Hidden };
    }
}

// ─── NPC systems ─────────────────────────────────────────────────────────────

const NPC_DATA: &[(&str, &str, Vec3)] = &[
    ("Elder Savan", "!", Vec3::new(6.0, 1.0, -6.0)),
    ("Merchant Lira", "$", Vec3::new(-6.0, 1.0, 4.0)),
    ("Guard Bren", "?", Vec3::new(8.0, 1.0, 2.0)),
];

fn spawn_npcs(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let npc_img = images.add(rect_image(18, 30, [190, 140, 70, 255]));
    for (name, marker, pos) in NPC_DATA {
        let screen = world_to_screen(*pos);
        let z = iso_depth(*pos) + 0.5;
        let npc = commands.spawn((
            NpcMarker,
            NpcName(name.to_string()),
            PlayerPosition(*pos),
            Sprite {
                image: npc_img.clone(),
                custom_size: Some(Vec2::new(18.0, 30.0)),
                ..default()
            },
            Transform::from_xyz(screen.x, screen.y, z),
        )).id();

        commands.spawn((
            NpcTextOf { target: npc, y_offset: 22.0 },
            Text2d::new(*name),
            TextFont { font_size: 9.0, ..default() },
            TextColor(Color::srgb(0.95, 0.85, 0.35)),
            Transform::from_xyz(screen.x, screen.y + 22.0, z + 0.05),
        ));
        commands.spawn((
            NpcTextOf { target: npc, y_offset: 34.0 },
            Text2d::new(*marker),
            TextFont { font_size: 14.0, ..default() },
            TextColor(Color::srgb(1.0, 0.80, 0.10)),
            Transform::from_xyz(screen.x, screen.y + 34.0, z + 0.06),
        ));
    }
}

fn spawn_interaction_ui(mut commands: Commands) {
    // "Press E to interact" tooltip at bottom-center
    commands.spawn((
        NpcInteractPrompt,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(60.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-120.0)),
            width: Val::Px(240.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.88)),
        GlobalZIndex(80),
        Visibility::Hidden,
        children![(
            Text::new("[E] Talk"),
            TextFont { font_size: 13.0, ..default() },
            TextColor(Color::srgb(0.90, 0.82, 0.30)),
        )],
    ));

    // NPC dialog box
    commands
        .spawn((
            NpcDialog,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(80.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-180.0)),
                width: Val::Px(360.0),
                padding: UiRect::all(Val::Px(14.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.06, 0.06, 0.12, 0.95)),
            BorderColor::all(Color::srgba(0.40, 0.35, 0.20, 0.80)),
            GlobalZIndex(85),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                NpcDialogText,
                Text::new(""),
                TextFont { font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.92, 0.88, 0.80)),
            ));
            p.spawn((
                Text::new("[E] Close  ·  [L] Quest Log"),
                TextFont { font_size: 10.0, ..default() },
                TextColor(Color::srgba(0.55, 0.55, 0.55, 1.0)),
            ));
        });
}

const NPC_INTERACT_RANGE: f32 = 5.0;

fn handle_npc_interaction(
    player_q: Query<&PlayerPosition, With<Player>>,
    npc_q: Query<(Entity, &PlayerPosition, &NpcName), With<NpcMarker>>,
    actions: Res<ActionState>,
    mut state: ResMut<NpcInteractionState>,
    mut prompt_q: Query<&mut Visibility, (With<NpcInteractPrompt>, Without<NpcDialog>)>,
    mut dialog_q: Query<&mut Visibility, (With<NpcDialog>, Without<NpcInteractPrompt>)>,
    mut text_q: Query<&mut Text, With<NpcDialogText>>,
) {
    let Ok(player_pos) = player_q.single() else { return };

    // Find the nearest NPC in range
    let mut nearest: Option<(Entity, f32, &str)> = None;
    for (entity, npc_pos, npc_name) in &npc_q {
        let d = player_pos.0.distance(npc_pos.0);
        if d < NPC_INTERACT_RANGE {
            if nearest.map_or(true, |(_, bd, _)| d < bd) {
                nearest = Some((entity, d, npc_name.0.as_str()));
            }
        }
    }

    state.near_npc = nearest.map(|(e, _, _)| e);

    // Toggle dialog on E
    if actions.just_pressed(GameAction::Interact) {
        if state.dialog_open {
            state.dialog_open = false;
        } else if let Some((_, _, name)) = nearest {
            state.dialog_open = true;
            if let Ok(mut t) = text_q.single_mut() {
                t.0 = npc_dialog_text(name);
            }
        }
    }

    // Update UI visibility
    let show_prompt = state.near_npc.is_some() && !state.dialog_open;
    if let Ok(mut v) = prompt_q.single_mut() {
        *v = if show_prompt { Visibility::Inherited } else { Visibility::Hidden };
    }
    if let Ok(mut v) = dialog_q.single_mut() {
        *v = if state.dialog_open { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn npc_dialog_text(name: &str) -> String {
    match name {
        "Elder Savan" =>
            "Ah, an adventurer! Wolves and boars grow bolder by the day.\n\
             Slay them and I shall reward you.\n\n\
             Press [L] to open the Quest Log.\n\
             Type /qaccept <id> in chat to accept a quest.\n\
             Type /qturnin <id> when objectives are complete.",
        "Merchant Lira" =>
            "Welcome, traveller! Join a guild and grow your power.\n\
             Exclusive guilds offer special island privileges.\n\n\
             /gcreate <name>  — found a guild\n\
             /ginvite <name>  — invite a player\n\
             /glist           — browse all guilds\n\
             /visit           — travel to your guild island",
        "Guard Bren" =>
            "Halt! ... Oh, you're no threat. Carry on.\n\
             The overworld is dangerous. Form a party for safety.\n\n\
             /pinvite <name>  — invite to party\n\
             /paccept         — accept a party invite\n\
             /tradewith <name>— open a trade session\n\
             /mail <name> <msg>— send mail",
        _ =>
            "Greetings, adventurer. Safe travels.",
    }.to_string()
}

fn update_npc_texts(
    npcs: Query<&Transform, With<NpcMarker>>,
    mut texts: Query<(&NpcTextOf, &mut Transform), Without<NpcMarker>>,
) {
    for (tag, mut tf) in &mut texts {
        if let Ok(npc_tf) = npcs.get(tag.target) {
            tf.translation.x = npc_tf.translation.x;
            tf.translation.y = npc_tf.translation.y + tag.y_offset;
        }
    }
}

// ─── Image helpers ────────────────────────────────────────────────────────────

/// Generate a solid-colour rectangular image.
fn rect_image(w: u32, h: u32, rgba: [u8; 4]) -> Image {
    let pixels = rgba.iter().cycle().take((w * h * 4) as usize).cloned().collect();
    Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Generate a diamond (rhombus) image with transparent corners.
/// The diamond touches all four edge midpoints and fills the interior.
fn diamond_image(w: u32, h: u32, rgba: [u8; 4]) -> Image {
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for py in 0..h {
        for px in 0..w {
            let nx = (px as f32 + 0.5 - cx).abs() / cx;
            let ny = (py as f32 + 0.5 - cy).abs() / cy;
            if nx + ny <= 1.0 {
                let i = ((py * w + px) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(&rgba);
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

/// Generate a block side-face image split vertically: left half and right half
/// use different shades to convey the SW/SE faces of an isometric cube.
fn block_face_image(w: u32, h: u32, left: [u8; 4], right: [u8; 4]) -> Image {
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let cx = w / 2;
    for py in 0..h {
        for px in 0..w {
            let rgba = if px < cx { left } else { right };
            let i = ((py * w + px) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&rgba);
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
