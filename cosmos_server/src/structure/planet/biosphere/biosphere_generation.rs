//! Responsible for the default generation of biospheres.

use std::{collections::HashSet, marker::PhantomData, mem::swap};

use bevy::{
    prelude::{Component, Entity, EventReader, EventWriter, Query, Res, ResMut, Resource},
    tasks::AsyncComputeTaskPool,
};
use cosmos_core::{
    block::{Block, BlockFace},
    physics::location::Location,
    structure::{
        chunk::{Chunk, CHUNK_DIMENSIONS},
        planet::Planet,
        ChunkInitEvent, Structure,
    },
    utils::{resource_wrapper::ResourceWrapper, timer::UtilsTimer},
};
use futures_lite::future;
use noise::NoiseFn;

use super::{GeneratingChunk, GeneratingChunks, TGenerateChunkEvent};

const AMPLITUDE: f64 = 7.0;
const DELTA: f64 = 0.05;
const ITERATIONS: usize = 9;

/// Some chunks might not be getting flattened, or maybe I'm just crazy.
/// Within (flattening_fraction * planet size) of the 45 starts the flattening.
const FLAT_FRACTION: f64 = 0.4;

/// This fraction of the original depth always remains, even on the very edge of the world.
const UNFLATTENED: f64 = 0.25;

fn get_top_height(
    (mut x, mut y, mut z): (usize, usize, usize),
    (structure_x, structure_y, structure_z): (f64, f64, f64),
    s_dimensions: usize,
    noise_generator: &noise::OpenSimplex,
    middle_air_start: usize,
) -> usize {
    let mut depth: f64 = 0.0;
    for iteration in 1..=ITERATIONS {
        let iteration = iteration as f64;
        depth += noise_generator.get([
            (x as f64 + structure_x) * (DELTA / iteration),
            (y as f64 + structure_y) * (DELTA / iteration),
            (z as f64 + structure_z) * (DELTA / iteration),
        ]) * AMPLITUDE
            * iteration;
    }

    // For the flattening (it's like the rumbling).
    x = x.min(s_dimensions - x);
    y = y.min(s_dimensions - y);
    z = z.min(s_dimensions - z);

    let initial_height = middle_air_start as f64 + depth;

    // Min is height of the face you're on, second min is the closer to the 45 of the 2 remaining.
    let dist_from_space = s_dimensions as f64 - initial_height;
    let dist_from_45 = x.min(y).max(x.max(y).min(z)) as f64 - dist_from_space;
    let flattening_limit = (s_dimensions as f64 - 2.0 * dist_from_space) * FLAT_FRACTION;
    depth *=
        dist_from_45.min(flattening_limit) / flattening_limit * (1.0 - UNFLATTENED) + UNFLATTENED;

    (middle_air_start as f64 + depth).round() as usize
}

/// Sends a ChunkInitEvent for every chunk that's done generating, monitors when chunks are finished generating.
pub fn notify_when_done_generating<T: Component>(
    mut generating: ResMut<GeneratingChunks<T>>,
    mut event_writer: EventWriter<ChunkInitEvent>,
    mut structure_query: Query<&mut Structure>,
) {
    let mut still_todo = Vec::with_capacity(generating.generating.len());

    swap(&mut generating.generating, &mut still_todo);

    for mut generating_chunk in still_todo {
        if let Some(chunks) = future::block_on(future::poll_once(&mut generating_chunk.task)) {
            let (chunk, structure_entity) = chunks;

            if let Ok(mut structure) = structure_query.get_mut(structure_entity) {
                let (x, y, z) = (
                    chunk.structure_x(),
                    chunk.structure_y(),
                    chunk.structure_z(),
                );

                structure.set_chunk(chunk);

                event_writer.send(ChunkInitEvent {
                    structure_entity,
                    x,
                    y,
                    z,
                });
            }
        } else {
            generating.generating.push(generating_chunk);
        }
    }
}

#[inline]
fn do_face<T: Component + Clone>(
    (sx, sy, sz): (usize, usize, usize),
    (structure_x, structure_y, structure_z): (f64, f64, f64),
    s_dimensions: usize,
    noise_generator: &noise::OpenSimplex,
    middle_air_start: usize,
    block_ranges: &BlockRanges<T>,
    chunk: &mut Chunk,
    up: BlockFace,
) {
    for i in 0..CHUNK_DIMENSIONS {
        for j in 0..CHUNK_DIMENSIONS {
            let seed_coordinates = match up {
                BlockFace::Top => (sx + i, middle_air_start, sz + j),
                BlockFace::Bottom => (sx + i, s_dimensions - middle_air_start, sz + j),
                BlockFace::Front => (sx + i, sy + j, middle_air_start),
                BlockFace::Back => (sx + i, sy + j, s_dimensions - middle_air_start),
                BlockFace::Right => (middle_air_start, sy + i, sz + j),
                BlockFace::Left => (s_dimensions - middle_air_start, sy + i, sz + j),
            };

            let top_height = get_top_height(
                seed_coordinates,
                (structure_x, structure_y, structure_z),
                s_dimensions,
                noise_generator,
                middle_air_start,
            );

            for height in 0..CHUNK_DIMENSIONS {
                let (x, y, z, actual_height) = match up {
                    BlockFace::Top => (i, height, j, sy + height),
                    BlockFace::Bottom => (i, height, j, s_dimensions - (sy + height)),
                    BlockFace::Front => (i, j, height, sz + height),
                    BlockFace::Back => (i, j, height, s_dimensions - (sz + height)),
                    BlockFace::Right => (height, i, j, sx + height),
                    BlockFace::Left => (height, i, j, s_dimensions - (sx + height)),
                };

                if actual_height <= top_height {
                    let block = block_ranges.face_block(top_height - actual_height);
                    chunk.set_block_at(x, y, z, block, up);
                }
            }
        }
    }
}

fn do_edge<T: Component + Clone>(
    (sx, sy, sz): (usize, usize, usize),
    (structure_x, structure_y, structure_z): (f64, f64, f64),
    s_dimensions: usize,
    noise_generator: &noise::OpenSimplex,
    middle_air_start: usize,
    block_ranges: &BlockRanges<T>,
    chunk: &mut Chunk,
    j_up: BlockFace,
    k_up: BlockFace,
) {
    let mut j_top = [[0; CHUNK_DIMENSIONS]; CHUNK_DIMENSIONS];
    for (i, layer) in j_top.iter_mut().enumerate().take(CHUNK_DIMENSIONS) {
        for (k, height) in layer.iter_mut().enumerate().take(CHUNK_DIMENSIONS) {
            // Seed coordinates for the noise function. Which loop variable goes to which xyz must agree everywhere.
            let (mut x, mut y, mut z) = (sx + i, sy + i, sz + i);
            match j_up {
                BlockFace::Front => z = middle_air_start,
                BlockFace::Back => z = s_dimensions - middle_air_start,
                BlockFace::Left => x = s_dimensions - middle_air_start,
                BlockFace::Right => x = middle_air_start,
                BlockFace::Top => y = middle_air_start,
                BlockFace::Bottom => y = s_dimensions - middle_air_start,
            };
            match k_up {
                BlockFace::Front | BlockFace::Back => z = sz + k,
                BlockFace::Left | BlockFace::Right => x = sx + k,
                BlockFace::Top | BlockFace::Bottom => y = sy + k,
            };

            // Unmodified top height.
            *height = get_top_height(
                (x, y, z),
                (structure_x, structure_y, structure_z),
                s_dimensions,
                noise_generator,
                middle_air_start,
            );

            // Don't let the top fall "below" the 45.
            let dim_45 = match k_up {
                BlockFace::Front => z,
                BlockFace::Back => s_dimensions - z,
                BlockFace::Left => s_dimensions - x,
                BlockFace::Right => x,
                BlockFace::Top => y,
                BlockFace::Bottom => s_dimensions - y,
            };
            *height = (*height).max(dim_45);
        }
    }

    for i in 0..CHUNK_DIMENSIONS {
        // The minimum (j, j) on the 45 where the two top heights intersect.
        let mut first_both_45 = s_dimensions;
        for j in 0..CHUNK_DIMENSIONS {
            // Seed coordinates for the noise function. Which loop variable goes to which xyz must agree everywhere.
            let (mut x, mut y, mut z) = (sx + i, sy + i, sz + i);
            match k_up {
                BlockFace::Front => z = middle_air_start,
                BlockFace::Back => z = s_dimensions - middle_air_start,
                BlockFace::Left => x = s_dimensions - middle_air_start,
                BlockFace::Right => x = middle_air_start,
                BlockFace::Top => y = middle_air_start,
                BlockFace::Bottom => y = s_dimensions - middle_air_start,
            };
            match j_up {
                BlockFace::Front | BlockFace::Back => z = sz + j,
                BlockFace::Left | BlockFace::Right => x = sx + j,
                BlockFace::Top | BlockFace::Bottom => y = sy + j,
            };

            // Unmodified top height.
            let mut k_top = get_top_height(
                (x, y, z),
                (structure_x, structure_y, structure_z),
                s_dimensions,
                noise_generator,
                middle_air_start,
            );

            // First height, and also the height of the other 45 bc of math.
            let j_height = match j_up {
                BlockFace::Front => z,
                BlockFace::Back => s_dimensions - z,
                BlockFace::Left => s_dimensions - x,
                BlockFace::Right => x,
                BlockFace::Top => y,
                BlockFace::Bottom => s_dimensions - y,
            };

            // Don't let the top height fall "below" the 45, but also don't let it go "above" the first shared 45.
            // This probably won't interfere with anything before the first shared 45 is discovered bc of the loop order.
            k_top = k_top.clamp(j_height, first_both_45);

            // Get smallest top height that's on the 45 for both y and z.
            if j_top[i][j] == j && k_top == j && first_both_45 == s_dimensions {
                first_both_45 = k_top;
            };

            for k in 0..CHUNK_DIMENSIONS {
                // Don't let the top height rise "above" the first shared 45.
                let j_top = j_top[i][k].min(first_both_45);

                // This is super smart I promise, definitely no better way to decide which loop variables are x, y, z.
                let (mut x, mut y, mut z) = (i, i, i);
                match j_up {
                    BlockFace::Front | BlockFace::Back => z = j,
                    BlockFace::Left | BlockFace::Right => x = j,
                    BlockFace::Top | BlockFace::Bottom => y = j,
                };
                match k_up {
                    BlockFace::Front | BlockFace::Back => z = k,
                    BlockFace::Left | BlockFace::Right => x = k,
                    BlockFace::Top | BlockFace::Bottom => y = k,
                };

                // Second height, and also the height of the other 45 (dim_45 in the upper loop must be recalculated here).
                let k_height = match k_up {
                    BlockFace::Front => sz + z,
                    BlockFace::Back => s_dimensions - (sz + z),
                    BlockFace::Left => s_dimensions - (sx + x),
                    BlockFace::Right => sx + x,
                    BlockFace::Top => sy + y,
                    BlockFace::Bottom => s_dimensions - (sy + y),
                };

                // Stops stairways to heaven.
                let num_top: usize = (j_height == j_top) as usize + (k_height == k_top) as usize;
                if j_height <= j_top && k_height <= k_top && num_top <= 1 {
                    // The top block needs different "top" to look good, the block can't tell which "up" looks good.
                    let mut block_up = Planet::get_planet_face_without_structure(
                        sx + x,
                        sy + y,
                        sz + z,
                        s_dimensions,
                        s_dimensions,
                        s_dimensions,
                    );
                    if j_height == j_top {
                        block_up = j_up;
                    }
                    if k_height == k_top {
                        block_up = k_up;
                    }
                    let block = block_ranges.edge_block(j_top - j_height, k_top - k_height);
                    chunk.set_block_at(x, y, z, block, block_up);
                }
            }
        }
    }
}

fn do_corner<T: Component + Clone>(
    (sx, sy, sz): (usize, usize, usize),
    (structure_x, structure_y, structure_z): (f64, f64, f64),
    s_dimensions: usize,
    noise_generator: &noise::OpenSimplex,
    middle_air_start: usize,
    block_ranges: &BlockRanges<T>,
    chunk: &mut Chunk,
    x_up: BlockFace,
    y_up: BlockFace,
    z_up: BlockFace,
) {
    // x top height cache.
    let mut x_top = [[0; CHUNK_DIMENSIONS]; CHUNK_DIMENSIONS];
    for (j, layer) in x_top.iter_mut().enumerate().take(CHUNK_DIMENSIONS) {
        for (k, height) in layer.iter_mut().enumerate().take(CHUNK_DIMENSIONS) {
            // Seed coordinates for the noise function.
            let (x, y, z) = match x_up {
                BlockFace::Right => (middle_air_start, sy + j, sz + k),
                _ => (s_dimensions - middle_air_start, sy + j, sz + k),
            };

            // Unmodified top height.
            *height = get_top_height(
                (x, y, z),
                (structure_x, structure_y, structure_z),
                s_dimensions,
                noise_generator,
                middle_air_start,
            );

            // Don't let the top height fall "below" the 45s.
            let y_45 = match y_up {
                BlockFace::Top => y,
                _ => s_dimensions - y,
            };
            let z_45 = match z_up {
                BlockFace::Front => z,
                _ => s_dimensions - z,
            };
            *height = (*height).max(y_45).max(z_45);
        }
    }

    // y top height cache.
    let mut y_top = [[0; CHUNK_DIMENSIONS]; CHUNK_DIMENSIONS];
    for (i, layer) in y_top.iter_mut().enumerate().take(CHUNK_DIMENSIONS) {
        for (k, height) in layer.iter_mut().enumerate().take(CHUNK_DIMENSIONS) {
            // Seed coordinates for the noise function. Which loop variable goes to which xyz must agree everywhere.
            let (x, y, z) = match y_up {
                BlockFace::Top => (sx + i, middle_air_start, sz + k),
                _ => (sx + i, s_dimensions - middle_air_start, sz + k),
            };

            // Unmodified top height.
            *height = get_top_height(
                (x, y, z),
                (structure_x, structure_y, structure_z),
                s_dimensions,
                noise_generator,
                middle_air_start,
            );

            // Don't let the top height fall "below" the 45s.
            let x_45 = match x_up {
                BlockFace::Right => x,
                _ => s_dimensions - x,
            };
            let z_45 = match z_up {
                BlockFace::Front => z,
                _ => s_dimensions - z,
            };
            *height = (*height).max(x_45).max(z_45);
        }
    }

    for i in 0..CHUNK_DIMENSIONS {
        // The minimum (j, j, j) on the 45 where the three top heights intersect.
        let mut first_all_45 = s_dimensions;
        for j in 0..CHUNK_DIMENSIONS {
            // Seed coordinates for the noise function.
            let (x, y, z) = match z_up {
                BlockFace::Front => (sx + i, sy + j, middle_air_start),
                _ => (sx + i, sy + j, s_dimensions - middle_air_start),
            };

            // Unmodified top height.
            let mut z_top = get_top_height(
                (x, y, z),
                (structure_x, structure_y, structure_z),
                s_dimensions,
                noise_generator,
                middle_air_start,
            );

            let x_height = match x_up {
                BlockFace::Right => x,
                _ => s_dimensions - x,
            };

            let y_height = match y_up {
                BlockFace::Top => y,
                _ => s_dimensions - y,
            };

            // Don't let the top height fall "below" the 45, but also don't let it go "above" the first shared 45.
            // This probably won't interfere with anything before the first shared 45 is discovered bc of the loop order.
            z_top = z_top.max(x_height).max(y_height);
            z_top = z_top.min(first_all_45);

            // Get smallest top height that's on the 45 for x, y, and z.
            if x_top[i][j] == j && y_top[i][j] == j && z_top == j && first_all_45 == s_dimensions {
                first_all_45 = z_top;
            };

            for k in 0..CHUNK_DIMENSIONS {
                // Don't let the top rise "above" the first shared 45.
                let x_top = x_top[j][k].min(first_all_45);
                let y_top = y_top[i][k].min(first_all_45);

                let z = sz + k;
                let z_height = match z_up {
                    BlockFace::Front => z,
                    _ => s_dimensions - z,
                };

                // Stops stairways to heaven.
                let num_top: usize = (x_height == x_top) as usize
                    + (y_height == y_top) as usize
                    + (z_height == z_top) as usize;
                if x_height <= x_top && y_height <= y_top && z_height <= z_top && num_top <= 1 {
                    // The top block needs different "top" to look good, the block can't tell which "up" looks good.
                    let mut block_up = Planet::get_planet_face_without_structure(
                        x,
                        y,
                        z,
                        s_dimensions,
                        s_dimensions,
                        s_dimensions,
                    );
                    if x_height == x_top {
                        block_up = x_up;
                    }
                    if y_height == y_top {
                        block_up = y_up;
                    }
                    if z_height == z_top {
                        block_up = z_up;
                    }
                    let block = block_ranges.corner_block(
                        x_top - x_height,
                        y_top - y_height,
                        z_top - z_height,
                    );
                    chunk.set_block_at(i, j, k, block, block_up);
                }
            }
        }
    }
}

/// Stores which blocks make up each biosphere, and how far below the top solid block each block generates.
/// Blocks in ascending order ("stone" = 5 first, "grass" = 0 last).
#[derive(Resource, Clone)]
pub struct BlockRanges<T: Component + Clone> {
    _phantom: PhantomData<T>,
    ranges: Vec<(Block, usize)>,
}

impl<T: Component + Clone> BlockRanges<T> {
    /// Creates a new block range, for each planet type to specify its blocks.
    pub fn new(ranges: Vec<(Block, usize)>) -> Self {
        BlockRanges::<T> {
            _phantom: Default::default(),
            ranges,
        }
    }

    fn face_block(&self, depth: usize) -> &Block {
        for (block, d) in self.ranges.iter() {
            if depth >= *d {
                return block;
            }
        }
        panic!("No matching block range for depth {depth}.");
    }

    fn edge_block(&self, j_depth: usize, k_depth: usize) -> &Block {
        for (block, d) in self.ranges.iter() {
            if j_depth >= *d && k_depth >= *d {
                return block;
            }
        }
        panic!("No matching block range for depths {j_depth} and {k_depth}.");
    }

    fn corner_block(&self, x_depth: usize, y_depth: usize, z_depth: usize) -> &Block {
        for (block, d) in self.ranges.iter() {
            if x_depth >= *d && y_depth >= *d && z_depth >= *d {
                return block;
            }
        }
        panic!("No matching block range for depths {x_depth}, {y_depth}, and {z_depth}.");
    }
}

/// Calls do_face, do_edge, and do_corner to generate the chunks of a planet.
pub fn generate_planet<T: Component + Clone, E: TGenerateChunkEvent + Send + Sync + 'static>(
    mut query: Query<(&mut Structure, &Location)>,
    mut generating: ResMut<GeneratingChunks<T>>,
    mut events: EventReader<E>,
    noise_generator: Res<ResourceWrapper<noise::OpenSimplex>>,
    block_ranges: Res<BlockRanges<T>>,
) {
    let chunks = events
        .iter()
        .filter_map(|ev| {
            let structure_entity = ev.get_structure_entity();
            let (x, y, z) = ev.get_chunk_coordinates();
            if let Ok((mut structure, _)) = query.get_mut(structure_entity) {
                Some((
                    structure_entity,
                    structure.take_or_create_chunk_for_loading(x, y, z),
                ))
            } else {
                None
            }
        })
        .collect::<Vec<(Entity, Chunk)>>();

    let thread_pool = AsyncComputeTaskPool::get();

    let chunks = chunks
        .into_iter()
        .flat_map(|(structure_entity, chunk)| {
            let Ok((structure, location)) = query.get(structure_entity) else {
                return None;
            };

            let s_width = structure.blocks_width();
            let s_height = structure.blocks_height();
            let s_length = structure.blocks_length();
            let location = *location;

            Some((
                chunk,
                s_width,
                s_height,
                s_length,
                location,
                structure_entity,
            ))
        })
        .collect::<Vec<(Chunk, usize, usize, usize, Location, Entity)>>();

    if !chunks.is_empty() {
        println!("Doing {} chunks!", chunks.len());

        for (mut chunk, s_width, s_height, s_length, location, structure_entity) in chunks {
            let block_ranges = block_ranges.clone();
            // let grass = grass.clone();
            // let dirt = dirt.clone();
            // let stone = stone.clone();
            // Not super expensive, only copies about 256 8 bit values.
            // Still not ideal though.
            let noise_generator = **noise_generator;

            let task = thread_pool.spawn(async move {
                let timer = UtilsTimer::start();

                // let grass = &grass;
                // let dirt = &dirt;
                // let stone = &stone;

                let middle_air_start = s_height - CHUNK_DIMENSIONS * 5;

                let actual_pos = location.absolute_coords_f64();

                let structure_z = actual_pos.z;
                let structure_y = actual_pos.y;
                let structure_x = actual_pos.x;

                // To save multiplication operations later.
                let sz = chunk.structure_z() * CHUNK_DIMENSIONS;
                let sy = chunk.structure_y() * CHUNK_DIMENSIONS;
                let sx = chunk.structure_x() * CHUNK_DIMENSIONS;

                // Get all possible planet faces from the chunk corners.
                let mut planet_faces = HashSet::new();
                for z in 0..=1 {
                    for y in 0..=1 {
                        for x in 0..=1 {
                            planet_faces.insert(Planet::get_planet_face_without_structure(
                                sx + x * CHUNK_DIMENSIONS,
                                sy + y * CHUNK_DIMENSIONS,
                                sz + z * CHUNK_DIMENSIONS,
                                s_width,
                                s_height,
                                s_length,
                            ));
                        }
                    }
                }

                // Support for the middle of the planet.
                if planet_faces.contains(&BlockFace::Top) {
                    planet_faces.remove(&BlockFace::Bottom);
                }
                if planet_faces.contains(&BlockFace::Right) {
                    planet_faces.remove(&BlockFace::Left);
                }
                if planet_faces.contains(&BlockFace::Front) {
                    planet_faces.remove(&BlockFace::Back);
                }

                if planet_faces.len() == 1 {
                    // Chunks on only one face.
                    do_face(
                        (sx, sy, sz),
                        (structure_x, structure_y, structure_z),
                        s_height,
                        &noise_generator,
                        middle_air_start,
                        &block_ranges,
                        &mut chunk,
                        *planet_faces.iter().next().unwrap(),
                    );
                } else if planet_faces.len() == 2 {
                    // Chunks on an edge.
                    let mut face_iter = planet_faces.iter();
                    do_edge(
                        (sx, sy, sz),
                        (structure_x, structure_y, structure_z),
                        s_height,
                        &noise_generator,
                        middle_air_start,
                        &block_ranges,
                        &mut chunk,
                        *face_iter.next().unwrap(),
                        *face_iter.next().unwrap(),
                    );
                } else {
                    let x_face = if planet_faces.contains(&BlockFace::Right) {
                        BlockFace::Right
                    } else {
                        BlockFace::Left
                    };
                    let y_face = if planet_faces.contains(&BlockFace::Top) {
                        BlockFace::Top
                    } else {
                        BlockFace::Bottom
                    };
                    let z_face = if planet_faces.contains(&BlockFace::Front) {
                        BlockFace::Front
                    } else {
                        BlockFace::Back
                    };
                    do_corner(
                        (sx, sy, sz),
                        (structure_x, structure_y, structure_z),
                        s_height,
                        &noise_generator,
                        middle_air_start,
                        &block_ranges,
                        &mut chunk,
                        x_face,
                        y_face,
                        z_face,
                    );
                }
                timer.log_duration("Chunk: ");
                (chunk, structure_entity)
            });

            generating.generating.push(GeneratingChunk::new(task));
        }
    }
}
