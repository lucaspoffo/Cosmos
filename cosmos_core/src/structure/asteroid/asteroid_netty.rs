//! Represents the communications an asteroid needs

use bevy::prelude::Entity;
use serde::{Deserialize, Serialize};

use crate::netty::netty_rigidbody::NettyRigidBody;

#[derive(Debug, Serialize, Deserialize)]
/// All the asteroid server messages
pub enum AsteroidServerMessages {
    /// Creates an asteroid
    ///
    /// Sent when a client requests information for an entity
    Asteroid {
        /// The asteroid's server entity
        entity: Entity,
        /// The asteroid's rigidbody
        body: NettyRigidBody,
        /// The width to be passed into the structure's constructor
        width: u32,
        /// The height to be passed into the structure's constructor
        height: u32,
        /// The length to be passed into the structure's constructor
        length: u32,
    },
}
