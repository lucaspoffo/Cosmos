//! Used to generate planets

use bevy::{
    ecs::event::Event,
    prelude::*,
    utils::{HashMap, HashSet},
};
use bevy_renet::renet::RenetServer;
use cosmos_core::{
    entities::player::Player,
    netty::{cosmos_encoder, server_reliable_messages::ServerReliableMessages, NettyChannel},
    physics::location::Location,
    structure::{
        chunk::{Chunk, CHUNK_DIMENSIONSF},
        planet::Planet,
        structure_iterator::ChunkIteratorResult,
        ChunkState, Structure,
    },
};

use crate::{state::GameState, structure::planet::biosphere::TGenerateChunkEvent};

#[derive(Component)]
/// This component will be in a planet's child entity if a chunk needs generated
///
/// This entity should be used as a flag, and is NOT the same as the chunk's entity
pub struct NeedsGenerated {
    chunk_coords: (usize, usize, usize),
    structure_entity: Entity,
}

/// T represents the event type to be generated
/// K represents the marker type for that specific biosphere
///
/// Use this to register your own planet generator
pub fn check_needs_generated_system<T: TGenerateChunkEvent + Event, K: Component>(
    mut commands: Commands,
    needs_generated_query: Query<(Entity, &NeedsGenerated)>,
    parent_query: Query<&Parent>,
    correct_type_query: Query<(), With<K>>,
    mut event_writer: EventWriter<T>,
) {
    for (entity, chunk) in needs_generated_query.iter() {
        let (cx, cy, cz) = chunk.chunk_coords;

        if let Ok(parent_entity) = parent_query.get(entity) {
            if correct_type_query.contains(parent_entity.get()) {
                event_writer.send(T::new(cx, cy, cz, chunk.structure_entity));

                commands.entity(entity).despawn_recursive();
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RequestChunkEvent {
    pub requester_id: u64,
    pub structure_entity: Entity,
    pub chunk_coords: (usize, usize, usize),
}

#[derive(Debug, Clone, Copy)]
struct RequestChunkBouncer(RequestChunkEvent);

fn bounce_events(
    mut event_reader: EventReader<RequestChunkBouncer>,
    mut event_writer: EventWriter<RequestChunkEvent>,
) {
    for ev in event_reader.iter() {
        println!("Bouncing back...");
        event_writer.send(ev.0);
    }
}

fn get_requested_chunk(
    mut event_reader: EventReader<RequestChunkEvent>,
    mut structure: Query<&mut Structure, With<Planet>>,
    mut event_writer: EventWriter<RequestChunkBouncer>,
    mut server: ResMut<RenetServer>,
    mut commands: Commands,
) {
    for ev in event_reader.iter() {
        if let Ok(mut structure) = structure.get_mut(ev.structure_entity) {
            let (cx, cy, cz) = ev.chunk_coords;

            match structure.get_chunk_state(cx, cy, cz) {
                ChunkState::Loaded => {
                    println!("Chunk was loaded! Sending!");
                    if let Some(chunk) = structure.chunk_from_chunk_coordinates(cx, cy, cz) {
                        server.send_message(
                            ev.requester_id,
                            NettyChannel::Reliable.id(),
                            cosmos_encoder::serialize(&ServerReliableMessages::ChunkData {
                                structure_entity: ev.structure_entity,
                                serialized_chunk: cosmos_encoder::serialize(chunk),
                            }),
                        );
                    }
                }
                ChunkState::Loading => event_writer.send(RequestChunkBouncer(*ev)),
                ChunkState::Unloaded => {
                    structure.set_chunk(Chunk::new(cx, cy, cz));
                    let needs_generated_flag = commands
                        .spawn(NeedsGenerated {
                            chunk_coords: (cx, cy, cz),
                            structure_entity: ev.structure_entity,
                        })
                        .id();

                    commands
                        .entity(ev.structure_entity)
                        .add_child(needs_generated_flag);

                    println!(
                        "FOUND CHUNK THAT NEEDS GENERATED @ {cx} {cy} {cz} (asked by client)!"
                    );

                    event_writer.send(RequestChunkBouncer(*ev));
                }
                ChunkState::Invalid => {
                    eprintln!("Client requested invalid chunk @ {cx} {cy} {cz}");
                }
            }
        }
    }
}

fn generate_chunks_near_players(
    players: Query<&Location, With<Player>>,
    mut planets: Query<(&Location, &mut Structure, Entity), With<Planet>>,
    mut commands: Commands,
) {
    for player in players.iter() {
        let mut best_planet = None;
        let mut best_dist = f32::INFINITY;
        for (location, structure, entity) in planets.iter_mut() {
            let dist = location.distance_sqrd(player);
            if dist < best_dist {
                best_dist = dist;
                best_planet = Some((location, structure, entity));
            }
        }

        if let Some((location, mut best_planet, entity)) = best_planet {
            let player_relative_position: Vec3 = (*player - *location).into();
            let (px, py, pz) = best_planet.relative_coords_to_local_coords(
                player_relative_position.x,
                player_relative_position.y,
                player_relative_position.z,
            );

            let (px, py, pz) = (
                (px as f32 / CHUNK_DIMENSIONSF).floor() as i32,
                (py as f32 / CHUNK_DIMENSIONSF).floor() as i32,
                (pz as f32 / CHUNK_DIMENSIONSF).floor() as i32,
            );

            let rd = 2 as i32;

            let iterator = best_planet.chunk_iter(
                (px - rd, py - rd, pz - rd),
                (px + rd, py + rd, pz + rd),
                true,
            );

            let mut chunks = Vec::with_capacity(iterator.len());

            for chunk in iterator {
                if let ChunkIteratorResult::EmptyChunk {
                    position: (x, y, z),
                } = chunk
                {
                    if best_planet.get_chunk_state(x, y, z) == ChunkState::Unloaded {
                        chunks.push((x, y, z));
                    }
                }
            }

            for (x, y, z) in chunks {
                best_planet.set_chunk(Chunk::new(x, y, z));
                let needs_generated_flag = commands
                    .spawn(NeedsGenerated {
                        chunk_coords: (x, y, z),
                        structure_entity: entity,
                    })
                    .id();

                commands.entity(entity).add_child(needs_generated_flag);

                println!("FOUND CHUNK THAT NEEDS GENERATED @ {x} {y} {z}!");
            }
        }
    }
}

fn unload_chunks_far_from_players(
    players: Query<&Location, With<Player>>,
    mut planets: Query<(&Location, &mut Structure, Entity), With<Planet>>,
    mut commands: Commands,
) {
    let mut potential_chunks = HashMap::<Entity, HashSet<(usize, usize, usize)>>::new();
    for (_, planet, entity) in planets.iter() {
        let mut set = HashSet::new();

        for chunk in planet.all_chunks_iter(false) {
            if let ChunkIteratorResult::FilledChunk { position, chunk: _ } = chunk {
                set.insert(position);
            }
        }

        potential_chunks.insert(entity, set);
    }

    for player in players.iter() {
        let mut best_planet = None;
        let mut best_dist = f32::INFINITY;
        for (location, structure, entity) in planets.iter_mut() {
            let dist = location.distance_sqrd(player);
            if dist < best_dist {
                best_dist = dist;
                best_planet = Some((location, structure, entity));
            }
        }

        if let Some((location, best_planet, entity)) = best_planet {
            let player_relative_position: Vec3 = (*player - *location).into();
            let (px, py, pz) = best_planet.relative_coords_to_local_coords(
                player_relative_position.x,
                player_relative_position.y,
                player_relative_position.z,
            );

            let (px, py, pz) = (
                (px as f32 / CHUNK_DIMENSIONSF).floor() as i32,
                (py as f32 / CHUNK_DIMENSIONSF).floor() as i32,
                (pz as f32 / CHUNK_DIMENSIONSF).floor() as i32,
            );

            let rd = 3 as i32;

            let iterator = best_planet.chunk_iter(
                (px - rd, py - rd, pz - rd),
                (px + rd, py + rd, pz + rd),
                true,
            );

            let set: &mut bevy::utils::hashbrown::HashSet<(usize, usize, usize)> = potential_chunks
                .get_mut(&entity)
                .expect("This was just added");

            for res in iterator {
                let chunk_position = match res {
                    ChunkIteratorResult::EmptyChunk { position } => position,
                    ChunkIteratorResult::FilledChunk { position, chunk: _ } => position,
                };

                println!("Not removing {chunk_position:?}");

                set.remove(&chunk_position);
            }
        }
    }

    for (planet, set) in potential_chunks {
        if let Ok((_, mut structure, _)) = planets.get_mut(planet) {
            for (cx, cy, cz) in set {
                println!("Unloading chunk at {cx} {cy} {cz}");
                structure.unload_chunk_at(cx, cy, cz, &mut commands);
            }
        }
    }
}

pub(super) fn register(app: &mut App) {
    app.add_systems(
        (
            generate_chunks_near_players,
            // unload_chunks_far_from_players,
            get_requested_chunk,
            bounce_events,
        )
            .chain()
            .in_set(OnUpdate(GameState::Playing)),
    )
    .add_event::<RequestChunkEvent>()
    .add_event::<RequestChunkBouncer>();
}
