use bevy::prelude::*;
// `lightyear::prelude::*` re-exports: AppMessageExt, AppChannelExt, ChannelMode,
// ChannelSettings, NetworkDirection, MessageSender, MessageReceiver, Connected,
// Replicate, etc.
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use crate::common::account::{LoginRequestMessage, LoginResultMessage, RegisterRequestMessage};
use crate::common::guild::{
    CreateGuildMessage, DisbandGuildMessage, GuildActionResultMessage, GuildInvitePushMessage,
    GuildInviteRequestMessage, GuildInviteResponseMessage, GuildListMessage, GuildStateMessage,
    KickGuildMemberMessage, LeaveGuildMessage, RequestGuildListMessage, SetGuildMotdMessage,
    SetGuildRankMessage, SetGuildSettingsMessage,
};
use crate::common::island::{
    IslandObjectInfo, MoveObjectMessage, PlaceObjectMessage, PrefabCatalogMessage,
    RemoveObjectMessage, TravelRequestMessage, ZoneChangedMessage,
};
use crate::common::mail::{
    MailActionResultMessage, MailListMessage, MarkMailReadMessage, RequestMailMessage,
    SendMailMessage,
};
use crate::common::mob::UnitVisual;
use crate::common::party::{
    KickPartyMemberMessage, LeavePartyMessage, PartyInviteMessage, PartyInvitePushMessage,
    PartyInviteResponseMessage, PartyStateMessage, PromotePartyLeaderMessage,
};
use crate::common::quest::{
    AbandonQuestMessage, AcceptQuestMessage, ProgressionMessage, QuestLogMessage,
    TurnInQuestMessage,
};
use crate::common::social::{ChatBroadcastMessage, ChatNetMessage};
use crate::common::zone::Zone;

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

/// Persistent character identity (survives reconnects/restarts). Distinct from
/// `PlayerId`, which is the ephemeral network entity id.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CharacterId(pub u64);

/// Replicated display name for a player character.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CharacterName(pub String);

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

/// Client requests a trade with another player (by name; sender resolved from
/// the connection).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeRequestNetMessage {
    pub to_name: String,
}

/// Client adds/removes an item to/from their own side of the offer. The actor
/// is resolved server-side from the connection.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeOfferUpdateMessage {
    pub item_id: u32,
    pub quantity: u32,
    /// If true, add to the offer; if false, remove.
    pub add: bool,
}

/// Client signals they accept the current offers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeAcceptMessage;

/// Client declines / cancels the trade.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeDeclineMessage;

/// Server sends authoritative, *recipient-relative* trade state to each party.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TradeStateMessage {
    pub phase: TradePhaseNet,
    /// (item_id, quantity) the receiving player is offering.
    pub your_offer: Vec<(u32, u32)>,
    /// (item_id, quantity) the other player is offering.
    pub their_offer: Vec<(u32, u32)>,
    pub you_accepted: bool,
    pub they_accepted: bool,
    /// Display name of the trade partner.
    pub partner_name: String,
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
        app.register_component::<CharacterId>();
        app.register_component::<CharacterName>();
        app.register_component::<Zone>();
        app.register_component::<UnitVisual>();
        app.register_component::<IslandObjectInfo>();

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

        // --- account / login ---
        app.register_message::<RegisterRequestMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<LoginRequestMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<LoginResultMessage>()
            .add_direction(NetworkDirection::ServerToClient);

        // --- guild ops (client → server) ---
        app.register_message::<CreateGuildMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<DisbandGuildMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<GuildInviteRequestMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<GuildInviteResponseMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<LeaveGuildMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<KickGuildMemberMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<SetGuildRankMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<SetGuildSettingsMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<SetGuildMotdMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<RequestGuildListMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        // --- guild state (server → client) ---
        app.register_message::<GuildStateMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<GuildListMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<GuildInvitePushMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<GuildActionResultMessage>()
            .add_direction(NetworkDirection::ServerToClient);

        // --- party ops (client → server) ---
        app.register_message::<PartyInviteMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<PartyInviteResponseMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<LeavePartyMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<KickPartyMemberMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<PromotePartyLeaderMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        // --- party state (server → client) ---
        app.register_message::<PartyStateMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<PartyInvitePushMessage>()
            .add_direction(NetworkDirection::ServerToClient);

        // --- island / build ops (client → server) ---
        app.register_message::<PlaceObjectMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<MoveObjectMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<RemoveObjectMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<TravelRequestMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        // --- island state (server → client) ---
        app.register_message::<ZoneChangedMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<PrefabCatalogMessage>()
            .add_direction(NetworkDirection::ServerToClient);

        // --- quest ops (client → server) ---
        app.register_message::<AcceptQuestMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<AbandonQuestMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<TurnInQuestMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        // --- quest / progression state (server → client) ---
        app.register_message::<QuestLogMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<ProgressionMessage>()
            .add_direction(NetworkDirection::ServerToClient);

        // --- mail ops (client → server) ---
        app.register_message::<SendMailMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<RequestMailMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<MarkMailReadMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        // --- mail state (server → client) ---
        app.register_message::<MailListMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<MailActionResultMessage>()
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
        // Note: game entities are no longer spawned on connect. The
        // `AccountServerPlugin` spawns a character only after a successful login.
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
