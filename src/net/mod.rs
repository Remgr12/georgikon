use bevy::prelude::*;
use lightyear::prelude::AppMessageExt;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerId(pub u64);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerPosition(pub Vec3);

pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.register_component::<PlayerId>();
        app.register_component::<PlayerPosition>();
        app.register_message::<MovementIntentMessage>();
        app.register_message::<CombatIntentMessage>();
        app.register_message::<InventoryIntentMessage>();
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MovementIntentMessage {
    pub player_id: u64,
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

fn handle_connections(mut commands: Commands, query: Query<Entity, Added<Connected>>) {
    for entity in query.iter() {
        println!("Client connected! Entity: {:?}", entity);
        commands.spawn((
            PlayerId(entity.to_bits()),
            PlayerPosition(Vec3::ZERO),
            Replicate::default(),
        ));
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
