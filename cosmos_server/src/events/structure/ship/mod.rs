//! Events for the ship

use bevy::prelude::{App, Entity, EventReader, IntoSystemConfig, OnUpdate, Query, ResMut};
use bevy_renet::renet::RenetServer;
use cosmos_core::{
    events::structure::change_pilot_event::ChangePilotEvent,
    netty::{
        cosmos_encoder, server_reliable_messages::ServerReliableMessages,
        server_unreliable_messages::ServerUnreliableMessages, NettyChannel,
    },
    structure::ship::ship_movement::ShipMovement,
};

use crate::state::GameState;

mod core;

#[derive(Debug)]
/// This event is sent when the ship's movement is set
pub struct ShipSetMovementEvent {
    /// The entity for the ship
    pub ship: Entity,
    /// The ship's new movement
    pub movement: ShipMovement,
}

fn monitor_set_movement_events(
    mut query: Query<&mut ShipMovement>,
    mut event_reader: EventReader<ShipSetMovementEvent>,
    mut server: ResMut<RenetServer>,
) {
    for ev in event_reader.iter() {
        if let Ok(mut current_movement) = query.get_mut(ev.ship) {
            current_movement.set(&ev.movement);

            server.broadcast_message(
                NettyChannel::Unreliable.id(),
                cosmos_encoder::serialize(&ServerUnreliableMessages::SetMovement {
                    movement: ev.movement.clone(),
                    ship_entity: ev.ship,
                }),
            );
        }
    }
}

fn monitor_pilot_changes(
    mut event_reader: EventReader<ChangePilotEvent>,
    mut server: ResMut<RenetServer>,
) {
    for ev in event_reader.iter() {
        server.broadcast_message(
            NettyChannel::Reliable.id(),
            cosmos_encoder::serialize(&ServerReliableMessages::PilotChange {
                structure_entity: ev.structure_entity,
                pilot_entity: ev.pilot_entity,
            }),
        );
    }
}

pub(super) fn register(app: &mut App) {
    core::register(app);

    app.add_event::<ShipSetMovementEvent>().add_systems((
        monitor_pilot_changes.in_set(OnUpdate(GameState::Playing)),
        monitor_set_movement_events.in_set(OnUpdate(GameState::Playing)),
    ));
}
