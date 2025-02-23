use bevy::prelude::{App, EventReader, EventWriter, IntoSystemConfig, OnUpdate, Query, Res, With};
use cosmos_core::{
    block::Block,
    events::structure::change_pilot_event::ChangePilotEvent,
    registry::{identifiable::Identifiable, Registry},
    structure::{
        ship::{pilot::Pilot, Ship},
        Structure,
    },
};

use crate::{events::blocks::block_events::BlockInteractEvent, state::GameState};

fn handle_block_event(
    mut interact_events: EventReader<BlockInteractEvent>,
    mut change_pilot_event: EventWriter<ChangePilotEvent>,
    s_query: Query<&Structure, With<Ship>>,
    pilot_query: Query<&Pilot>,
    blocks: Res<Registry<Block>>,
) {
    let block = blocks
        .from_id("cosmos:ship_core")
        .expect("ship core block missing!");

    for ev in interact_events.iter() {
        if let Ok(structure) = s_query.get(ev.structure_entity) {
            let block_id = ev.structure_block.block_id(structure);

            if block_id == block.id() {
                // Only works on ships (maybe replace this with pilotable component instead of only checking ships)
                // Cannot pilot a ship that already has a pilot
                if pilot_query.get(ev.structure_entity).is_err() {
                    change_pilot_event.send(ChangePilotEvent {
                        structure_entity: ev.structure_entity,
                        pilot_entity: Some(ev.interactor),
                    });
                }
            }
        }
    }
}

pub(super) fn register(app: &mut App) {
    app.add_system(handle_block_event.in_set(OnUpdate(GameState::Playing)));
}
