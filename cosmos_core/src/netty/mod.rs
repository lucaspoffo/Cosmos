//! Contains all the information required for network requests

pub mod client_reliable_messages;
pub mod client_unreliable_messages;
pub mod cosmos_encoder;
pub mod netty_rigidbody;
pub mod server_laser_cannon_system_messages;
pub mod server_reliable_messages;
pub mod server_unreliable_messages;

use bevy::{prelude::Component, utils::default};
use bevy_renet::renet::{
    ChannelConfig, ReliableChannelConfig, RenetConnectionConfig, UnreliableChannelConfig,
};
use local_ip_address::local_ip;
use std::time::Duration;

/// Used to tell the server to not send this entity to the player
///
/// Useful for entities that are automatically generated by other entities (like chunks)
#[derive(Component)]
pub struct NoSendEntity;

/// Different network channels have an enum here. Make sure to add any new ones here.
pub enum NettyChannel {
    /// These are reliably sent, so they are guarenteed to reach their destination.
    /// Used for `ClientReliableMessages` and `ServerReliableMessages`
    Reliable,
    /// These are unreliably sent, and may never reach their destination or become corrupted.
    /// Used for `ClientUnreliableMessages` and `ServerUnreliableMessages`
    Unreliable,
    /// Used for `ServerLaserCannonSystemMessages`
    LaserCannonSystem,

    /// Used for asteroids
    Asteroids,
}

/// In the future, this should be based off the game version.
///
/// Must have the same protocol to connect to something
pub const PROTOCOL_ID: u64 = 7;

impl NettyChannel {
    /// Gets the ID used in a netty channel
    pub fn id(&self) -> u8 {
        match self {
            Self::Reliable => 0,
            Self::Unreliable => 1,
            Self::LaserCannonSystem => 2,
            Self::Asteroids => 3,
        }
    }

    /// Assembles & returns the configuration for all the client channels
    pub fn client_channels_config() -> Vec<ChannelConfig> {
        vec![
            ReliableChannelConfig {
                channel_id: Self::Reliable.id(),
                message_resend_time: Duration::from_millis(200),
                message_send_queue_size: 4096 * 4,
                message_receive_queue_size: 4096 * 4,
                max_message_size: 12000,
                packet_budget: 13000,
                ..default()
            }
            .into(),
            UnreliableChannelConfig {
                channel_id: Self::Unreliable.id(),
                message_send_queue_size: 4096 * 16,
                message_receive_queue_size: 4096 * 16,
                ..default()
            }
            .into(),
            UnreliableChannelConfig {
                channel_id: Self::LaserCannonSystem.id(),
                packet_budget: 7000,
                max_message_size: 6000,
                message_send_queue_size: 0,
                message_receive_queue_size: 4096 * 16,
                ..default()
            }
            .into(),
            ReliableChannelConfig {
                channel_id: Self::Asteroids.id(),
                message_send_queue_size: 1000,
                message_receive_queue_size: 1024,
                max_message_size: 6000,
                packet_budget: 7000,
                ..Default::default()
            }
            .into(),
        ]
    }

    /// Assembles & returns the config for all the server channels
    pub fn server_channels_config() -> Vec<ChannelConfig> {
        vec![
            ReliableChannelConfig {
                channel_id: Self::Reliable.id(),
                message_resend_time: Duration::from_millis(200),
                message_send_queue_size: 4096 * 4,
                message_receive_queue_size: 4096 * 4,
                max_message_size: 12000,
                packet_budget: 13000,
                ..default()
            }
            .into(),
            UnreliableChannelConfig {
                channel_id: Self::Unreliable.id(),
                message_send_queue_size: 4096 * 16,
                message_receive_queue_size: 4096 * 16,
                ..default()
            }
            .into(),
            UnreliableChannelConfig {
                channel_id: Self::LaserCannonSystem.id(),
                packet_budget: 7000,
                max_message_size: 6000,
                message_send_queue_size: 4096 * 16,
                message_receive_queue_size: 0,
                ..default()
            }
            .into(),
            ReliableChannelConfig {
                channel_id: Self::Asteroids.id(),
                message_send_queue_size: 1024 * 16,
                message_receive_queue_size: 1000,
                max_message_size: 6000,
                packet_budget: 7000,
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Assembles the configuration for a client configuration
pub fn client_connection_config() -> RenetConnectionConfig {
    RenetConnectionConfig {
        send_channels_config: NettyChannel::client_channels_config(),
        receive_channels_config: NettyChannel::client_channels_config(),
        heartbeat_time: Duration::from_millis(10_000),
        ..default()
    }
}

/// Assembles the configuration for a server configuration
pub fn server_connection_config() -> RenetConnectionConfig {
    RenetConnectionConfig {
        send_channels_config: NettyChannel::server_channels_config(),
        receive_channels_config: NettyChannel::server_channels_config(),
        heartbeat_time: Duration::from_millis(10_000),
        ..default()
    }
}

/// Gets the local ip address, or returns `127.0.0.1` if it fails to find it.
pub fn get_local_ipaddress() -> String {
    local_ip()
        .map(|x| x.to_string())
        .unwrap_or("127.0.0.1".to_owned())
}
