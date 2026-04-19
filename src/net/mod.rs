use bevy::prelude::*;
// `lightyear::prelude::*` re-exports: AppMessageExt, AppChannelExt, ChannelMode,
// ChannelSettings, NetworkDirection, MessageSender, MessageReceiver, Connected,
// Replicate, etc.
use lightyear::prelude::*;
use lightyear::prelude::server::ClientOf;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use crate::common::social::{ChatBroadcastMessage, ChatNetMessage};
use crate::common::stats::CharacterStats;
use crate::server::player_state::{AuthoritativePlayerState, OwnedPlayer, OwnerConn};

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

/// Unreliable, unordered channel for high-frequency state (movement, snapshots).
pub struct UnreliableChannel;

/// Ordered reliable channel for deterministic events (combat outcomes, trade).
pub struct ReliableChannel;

// ---------------------------------------------------------------------------
// Replicated components
// ---------------------------------------------------------------------------

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerId(pub u64);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerPosition(pub Vec3);

// ---------------------------------------------------------------------------
// Client → Server intent messages
// ---------------------------------------------------------------------------

/// World-space movement intent sent by the client every frame.
/// `axis` is the camera-rotated XZ direction ([x, z]), already in world space.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MovementIntentMessage {
    pub player_id: u64,
    /// World-space horizontal direction [x, z] (normalized or zero).
    pub axis: [f32; 2],
    pub jump_pressed: bool,
    pub sprinting: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatIntentKind {
    Primary,
    Secondary,
    Block,
    Roll,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CombatIntentMessage {
    pub player_id: u64,
    pub kind: CombatIntentKind,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryIntentKind {
    UseHotbarSlot(usize),
    SortInventory,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InventoryIntentMessage {
    pub player_id: u64,
    pub kind: InventoryIntentKind,
}

// ---------------------------------------------------------------------------
// Server → Client state messages
// ---------------------------------------------------------------------------

/// Authoritative player snapshot sent from server to owning client.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerSnapshotMessage {
    /// Monotonically increasing server simulation tick.
    pub tick: u32,
    /// Authoritative world-space position [x, y, z].
    pub position: [f32; 3],
    /// Vertical velocity for physics continuation on the client.
    pub velocity_y: f32,
}

/// Authoritative combat-state update (cooldowns + character resources).
///
/// Sent server → client on the reliable channel after every intent evaluation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CombatStateMessage {
    pub tick: u32,
    pub roll_cooldown: f32,
    pub health: f32,
    pub max_health: f32,
    pub energy: f32,
    pub max_energy: f32,
    pub stamina: f32,
    pub max_stamina: f32,
}

// ---------------------------------------------------------------------------
// Trade messages (client ↔ server, bidirectional)
// ---------------------------------------------------------------------------

/// Client requests a trade with another player.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeRequestNetMessage {
    pub from_player_id: u64,
    pub to_player_id: u64,
}

/// Client updates their own side of the offer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeOfferUpdateMessage {
    pub player_id: u64,
    /// Slot in the player's inventory being offered.
    pub inventory_slot: usize,
    /// Quantity being added (positive) or removed (negative sign encoded as u32 difference).
    pub quantity: u32,
    /// If true, add this slot/qty to offer; if false, remove it.
    pub add: bool,
}

/// Client signals they accept the current offers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeAcceptMessage {
    pub player_id: u64,
}

/// Client declines / cancels the trade.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeDeclineMessage {
    pub player_id: u64,
}

/// Server sends authoritative trade state to both parties.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeStateMessage {
    pub phase: TradePhaseNet,
    /// Items offered by the initiating player: (item_id, quantity).
    pub offer_a: Vec<(u32, u32)>,
    /// Items offered by the responding player: (item_id, quantity).
    pub offer_b: Vec<(u32, u32)>,
    pub accepted_a: bool,
    pub accepted_b: bool,
}

/// Wire-safe trade phase enum (mirrors server::trade::TradePhase).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradePhaseNet {
    Mutate,
    Review,
    Complete,
    Declined,
}

// ---------------------------------------------------------------------------
// Shared protocol plugin
// ---------------------------------------------------------------------------

pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        // --- channels ---
        app.add_channel::<UnreliableChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..Default::default()
        });
        app.add_channel::<ReliableChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(Default::default()),
            ..Default::default()
        });

        // --- replicated components ---
        app.register_component::<PlayerId>();
        app.register_component::<PlayerPosition>();

        // --- intent messages (client → server) ---
        app.register_message::<MovementIntentMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<CombatIntentMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<InventoryIntentMessage>()
            .add_direction(NetworkDirection::ClientToServer);

        // --- state messages (server → client) ---
        app.register_message::<PlayerSnapshotMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<CombatStateMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<TradeStateMessage>()
            .add_direction(NetworkDirection::ServerToClient);

        // --- trade messages (client → server) ---
        app.register_message::<TradeRequestNetMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<TradeOfferUpdateMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<TradeAcceptMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<TradeDeclineMessage>()
            .add_direction(NetworkDirection::ClientToServer);

        // --- chat messages ---
        app.register_message::<ChatNetMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ChatBroadcastMessage>()
            .add_direction(NetworkDirection::ServerToClient);
    }
}

// ---------------------------------------------------------------------------
// Server network plugin
// ---------------------------------------------------------------------------

pub struct ServerNetworkPlugin;

impl Plugin for ServerNetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(lightyear::prelude::server::ServerPlugins {
            tick_duration: std::time::Duration::from_secs_f64(1.0 / 64.0),
        });
        app.add_systems(Startup, server_setup);
        app.add_systems(Update, handle_connections);
    }
}

/// Spawn an authoritative game entity for each newly connected client.
/// Filtered to `With<ClientOf>` so this does not fire for the local `Client`
/// entity in combined client+server mode.
fn handle_connections(
    mut commands: Commands,
    query: Query<Entity, (Added<Connected>, With<ClientOf>)>,
) {
    for conn_entity in query.iter() {
        tracing::info!("Client connected! conn_entity={:?}", conn_entity);

        // Spawn the replicated game entity with authoritative simulation state.
        let game_entity = commands
            .spawn((
                PlayerId(conn_entity.to_bits()),
                PlayerPosition(Vec3::new(0.0, 1.0, 0.0)),
                AuthoritativePlayerState {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    ..Default::default()
                },
                CharacterStats::default(),
                OwnerConn(conn_entity),
                Replicate::default(),
            ))
            .id();

        // Link the connection entity back to its game entity.
        commands.entity(conn_entity).insert(OwnedPlayer(game_entity));
    }
}

fn server_setup(mut commands: Commands) {
    let server_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 5000));
    commands.spawn((
        lightyear::prelude::server::ServerUdpIo::default(),
        lightyear::prelude::server::NetcodeServer::new(
            lightyear::prelude::server::NetcodeConfig::default()
                .with_protocol_id(1)
                .with_key([0; 32]),
        ),
        LocalAddr(server_addr),
    ));
}

pub struct ClientNetworkPlugin;

impl Plugin for ClientNetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(lightyear::prelude::client::ClientPlugins {
            tick_duration: std::time::Duration::from_secs_f64(1.0 / 64.0),
        });
        app.add_systems(Startup, client_setup);
    }
}

fn client_setup(mut commands: Commands) {
    let server_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 5000));
    let client_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
    let auth = lightyear::prelude::Authentication::Manual {
        server_addr,
        client_id: 1,
        private_key: [0; 32],
        protocol_id: 1,
    };
    commands.spawn((
        lightyear::prelude::UdpIo::default(),
        lightyear::prelude::client::NetcodeClient::new(
            auth,
            lightyear::prelude::client::NetcodeConfig::default(),
        )
        .unwrap(),
        LocalAddr(client_addr),
    ));
}
