use bevy::prelude::*;

// ---------------------------------------------------------------------------
// Cross-reference components
// ---------------------------------------------------------------------------

/// Component on a `ClientOf` connection entity pointing to its game entity.
#[derive(Component, Debug)]
pub struct OwnedPlayer(pub Entity);

/// Component on a game entity pointing back to the owning `ClientOf` connection entity.
#[derive(Component, Debug)]
pub struct OwnerConn(pub Entity);

// ---------------------------------------------------------------------------
// Authoritative simulation state
// ---------------------------------------------------------------------------

/// Server-authoritative simulation state for one connected player.
///
/// Lives on the game entity (alongside `PlayerId`, `PlayerPosition`, `Replicate`).
/// The server sim writes here; `PlayerPosition` is then synced from this for replication.
#[derive(Component, Debug, Default)]
pub struct AuthoritativePlayerState {
    /// World-space position.
    pub position: Vec3,
    /// Vertical velocity (for jump / gravity simulation).
    pub velocity_y: f32,
    /// Whether the player is sprinting this tick.
    pub sprinting: bool,
    /// Seconds remaining on the roll cooldown.
    pub roll_cooldown: f32,
    /// Monotonically increasing simulation tick counter (wraps at u32::MAX).
    pub tick: u32,
}
