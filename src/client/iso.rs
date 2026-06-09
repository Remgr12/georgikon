//! Isometric world-to-screen projection utilities.
//!
//! World coordinates use Bevy's right-handed system with Y-up and the playfield
//! on the XZ plane.  The isometric view maps:
//!   - A step in +X → screen right and down  (iso east)
//!   - A step in +Z → screen left and down   (iso west)
//!   - A step in +Y → screen up              (jump lift; no effect on depth)
//!
//! Depth (`Transform.translation.z`) is derived from x+z so entities with a
//! larger x+z sum draw over entities with a smaller sum.  Jump height (Y) does
//! **not** affect draw order — jumping never reorders sprites.

use bevy::prelude::*;

// ─── Tile geometry ────────────────────────────────────────────────────────────

/// Half the screen-pixel width spanned by one world-unit step in X or Z.
pub const TILE_HALF_W: f32 = 32.0;
/// Half the screen-pixel height spanned by one world-unit step in X or Z.
pub const TILE_HALF_H: f32 = 16.0;

/// Screen pixels one world Y-unit lifts a sprite (for jump animation).
pub const Y_LIFT: f32 = 24.0;

/// World Y at which entities rest on the ground.
pub const REST_Y: f32 = 1.0;

/// Converts x+z world sum to a Bevy-2D z depth value.  Higher z = drawn on top.
pub const DEPTH_SCALE: f32 = 0.001;

// ─── Fixed movement bases (camera-independent) ────────────────────────────────

/// Unit step in world space for pressing **W** ("forward" in iso view).
/// Not normalised — call `.normalize_or_zero()` before multiplying by speed.
pub const ISO_FORWARD: Vec3 = Vec3::new(-1.0, 0.0, -1.0);

/// Unit step in world space for pressing **D** ("right" in iso view).
pub const ISO_RIGHT: Vec3 = Vec3::new(1.0, 0.0, -1.0);

// ─── Core projection functions ────────────────────────────────────────────────

/// Project a world position to a 2-D screen position.
///
/// X and Z drive the isometric diamond; Y offset is a vertical screen lift
/// (only the portion above `REST_Y` contributes).
#[inline]
pub fn world_to_screen(w: Vec3) -> Vec2 {
    let screen_x = (w.x - w.z) * TILE_HALF_W;
    let screen_y = -(w.x + w.z) * TILE_HALF_H + (w.y - REST_Y) * Y_LIFT;
    Vec2::new(screen_x, screen_y)
}

/// Derive a draw-order Z from a world position.
///
/// A larger `x + z` sum is "closer" to the iso camera and must render on top.
/// The Y axis is intentionally excluded so jumping never changes draw order.
#[inline]
pub fn iso_depth(w: Vec3) -> f32 {
    (w.x + w.z) * DEPTH_SCALE
}

/// Build a full [`Transform`] for a world entity (screen XY + depth Z).
#[inline]
pub fn world_to_transform(w: Vec3) -> Transform {
    let xy = world_to_screen(w);
    Transform::from_xyz(xy.x, xy.y, iso_depth(w))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_monotonic_in_xz_sum() {
        // Entities further "into" the iso scene (larger x+z) have higher depth.
        let near = Vec3::new(0.0, 99.0, 0.0); // x+z = 0,  y shouldn't matter
        let mid = Vec3::new(1.0, 0.0, 1.0); // x+z = 2
        let far = Vec3::new(3.0, 9.0, 3.0); // x+z = 6
        assert!(iso_depth(near) < iso_depth(mid));
        assert!(iso_depth(mid) < iso_depth(far));
    }

    #[test]
    fn screen_x_invariant_to_y() {
        // Jumping (changing Y) must not shift the sprite left/right.
        let ground = world_to_screen(Vec3::new(5.0, REST_Y, 3.0));
        let airborne = world_to_screen(Vec3::new(5.0, REST_Y + 10.0, 3.0));
        assert!((ground.x - airborne.x).abs() < f32::EPSILON);
    }

    #[test]
    fn iso_right_increases_screen_x() {
        // Moving in +ISO_RIGHT direction should increase screen X.
        let orig = world_to_screen(Vec3::ZERO);
        let moved = world_to_screen(ISO_RIGHT.normalize_or_zero());
        assert!(moved.x > orig.x);
    }
}
