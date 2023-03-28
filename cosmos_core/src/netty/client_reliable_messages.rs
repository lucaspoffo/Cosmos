use bevy::prelude::{Component, Entity};
use serde::{Deserialize, Serialize};

use crate::entities::player::render_distance::RenderDistance;

#[derive(Debug, Serialize, Deserialize, Component)]
pub enum ClientReliableMessages {
    PlayerDisconnect,
    SendChunk {
        server_entity: Entity,
    },
    BreakBlock {
        structure_entity: Entity,
        x: u32,
        y: u32,
        z: u32,
    },
    PlaceBlock {
        structure_entity: Entity,
        x: u32,
        y: u32,
        z: u32,
        // block_id is passed along with inventory_slot to verify that the client+server are still in sync
        block_id: u16,
        inventory_slot: u32,
    },
    InteractWithBlock {
        structure_entity: Entity,
        x: u32,
        y: u32,
        z: u32,
    },
    CreateShip {
        name: String,
    },
    PilotQuery {
        ship_entity: Entity,
    },
    StopPiloting,
    ChangeRenderDistance {
        render_distance: RenderDistance,
    },
    /// This does NOT guarentee the entity will be sent - only requests it
    RequestEntityData {
        entity: Entity,
    },
}
