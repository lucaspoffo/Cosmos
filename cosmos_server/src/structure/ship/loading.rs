use bevy::prelude::{App, Commands, Component, Entity, EventWriter, Query, Res, SystemSet, With};
use cosmos_core::{
    block::Block,
    registry::Registry,
    structure::{events::ChunkSetEvent, loading::ChunksNeedLoaded, Structure},
};

use crate::state::GameState;

/// A flag that denotes that a ship needs created
#[derive(Component)]
pub struct ShipNeedsCreated;

fn create_ships(
    // ChunksNeedLoaded has to be queried to ensure that the chunksetevents will trigger the structure loaded event.
    mut query: Query<(&mut Structure, Entity), (With<ShipNeedsCreated>, With<ChunksNeedLoaded>)>,
    mut commands: Commands,
    blocks: Res<Registry<Block>>,
    mut chunk_set_event_writer: EventWriter<ChunkSetEvent>,
) {
    for (mut structure, entity) in query.iter_mut() {
        commands.entity(entity).remove::<ShipNeedsCreated>();

        // TODO: eventually load this from base ship file?

        let ship_core = blocks
            .from_id("cosmos:ship_core")
            .expect("Ship core block missing!");

        let (width, height, length) = (
            structure.blocks_width(),
            structure.blocks_height(),
            structure.blocks_length(),
        );

        structure.set_block_at(width / 2, height / 2, length / 2, ship_core, &blocks, None);

        for chunk in structure.all_chunks_iter() {
            chunk_set_event_writer.send(ChunkSetEvent {
                structure_entity: entity,
                x: chunk.structure_x(),
                y: chunk.structure_y(),
                z: chunk.structure_z(),
            });
        }
    }
}

pub fn register(app: &mut App) {
    app.add_system_set(SystemSet::on_update(GameState::Playing).with_system(create_ships));
}