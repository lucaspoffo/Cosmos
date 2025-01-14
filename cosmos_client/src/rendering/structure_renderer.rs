use crate::block::lighting::{BlockLightProperties, BlockLighting};
use crate::materials::CosmosMaterial;
use crate::netty::flags::LocalPlayer;
use crate::state::game_state::GameState;
use crate::structure::planet::unload_chunks_far_from_players;
use bevy::prelude::{
    warn, App, BuildChildren, Component, DespawnRecursiveExt, EventReader, GlobalTransform,
    IntoSystemConfigs, Mesh, OnUpdate, PbrBundle, PointLight, PointLightBundle, Quat, Rect,
    StandardMaterial, Transform, Vec3, With,
};
use bevy::reflect::{FromReflect, Reflect};
use bevy::render::primitives::Aabb;
use bevy::utils::hashbrown::HashMap;
use cosmos_core::block::{Block, BlockFace};
use cosmos_core::events::block_events::BlockChangedEvent;
use cosmos_core::physics::location::SECTOR_DIMENSIONS;
use cosmos_core::registry::identifiable::Identifiable;
use cosmos_core::registry::many_to_one::ManyToOneRegistry;
use cosmos_core::registry::Registry;
use cosmos_core::structure::chunk::{Chunk, ChunkEntity, CHUNK_DIMENSIONS, CHUNK_DIMENSIONSF};
use cosmos_core::structure::events::ChunkSetEvent;
use cosmos_core::structure::structure_block::StructureBlock;
use cosmos_core::structure::Structure;
use cosmos_core::utils::array_utils::expand;
use cosmos_core::utils::timer::UtilsTimer;
use rayon::prelude::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;
use std::f32::consts::PI;
use std::sync::Mutex;

use crate::asset::asset_loading::{BlockTextureIndex, MainAtlas};
use crate::{Assets, Commands, Entity, Handle, Query, Res, ResMut};

use super::{BlockMeshRegistry, CosmosMeshBuilder, MeshBuilder, MeshInformation};

#[derive(Debug)]
struct MeshMaterial {
    mesh: Mesh,
    material: Handle<StandardMaterial>,
}

#[derive(Debug)]
struct ChunkMesh {
    mesh_materials: Vec<MeshMaterial>,
    lights: HashMap<(usize, usize, usize), BlockLightProperties>,
}

fn monitor_block_updates_system(
    mut event: EventReader<BlockChangedEvent>,
    mut chunk_set_event: EventReader<ChunkSetEvent>,
    structure_query: Query<&Structure>,
    mut commands: Commands,
) {
    let mut chunks_todo = HashMap::<Entity, HashSet<(usize, usize, usize)>>::default();

    for ev in event.iter() {
        let structure: &Structure = structure_query.get(ev.structure_entity).unwrap();
        if !chunks_todo.contains_key(&ev.structure_entity) {
            chunks_todo.insert(ev.structure_entity, HashSet::default());
        }

        let chunks = chunks_todo
            .get_mut(&ev.structure_entity)
            .expect("This was just added");

        if ev.block.x() != 0 && ev.block.x() % CHUNK_DIMENSIONS == 0 {
            chunks.insert((
                ev.block.chunk_coord_x() - 1,
                ev.block.chunk_coord_y(),
                ev.block.chunk_coord_z(),
            ));
        }

        if ev.block.x() != structure.blocks_width() - 1
            && (ev.block.x() + 1) % CHUNK_DIMENSIONS == 0
        {
            chunks.insert((
                ev.block.chunk_coord_x() + 1,
                ev.block.chunk_coord_y(),
                ev.block.chunk_coord_z(),
            ));
        }

        if ev.block.y() != 0 && ev.block.y() % CHUNK_DIMENSIONS == 0 {
            chunks.insert((
                ev.block.chunk_coord_x(),
                ev.block.chunk_coord_y() - 1,
                ev.block.chunk_coord_z(),
            ));
        }

        if ev.block.y() != structure.blocks_height() - 1
            && (ev.block.y() + 1) % CHUNK_DIMENSIONS == 0
        {
            chunks.insert((
                ev.block.chunk_coord_x(),
                ev.block.chunk_coord_y() + 1,
                ev.block.chunk_coord_z(),
            ));
        }

        if ev.block.z() != 0 && ev.block.z() % CHUNK_DIMENSIONS == 0 {
            chunks.insert((
                ev.block.chunk_coord_x(),
                ev.block.chunk_coord_y(),
                ev.block.chunk_coord_z() - 1,
            ));
        }

        if ev.block.z() != structure.blocks_length() - 1
            && (ev.block.z() + 1) % CHUNK_DIMENSIONS == 0
        {
            chunks.insert((
                ev.block.chunk_coord_x(),
                ev.block.chunk_coord_y(),
                ev.block.chunk_coord_z() + 1,
            ));
        }

        chunks.insert((
            ev.block.chunk_coord_x(),
            ev.block.chunk_coord_y(),
            ev.block.chunk_coord_z(),
        ));
    }

    for ev in chunk_set_event.iter() {
        let Ok(structure) = structure_query.get(ev.structure_entity) else {
            continue;
        };

        if !chunks_todo.contains_key(&ev.structure_entity) {
            chunks_todo.insert(ev.structure_entity, HashSet::default());
        }

        let chunks = chunks_todo
            .get_mut(&ev.structure_entity)
            .expect("This was just added");

        let (x, y, z) = (ev.x, ev.y, ev.z);

        chunks.insert((x, y, z));

        if ev.z != 0 {
            chunks.insert((x, y, z - 1));
        }
        if ev.z < structure.chunks_length() - 1 {
            chunks.insert((x, y, z + 1));
        }
        if ev.y != 0 {
            chunks.insert((x, y - 1, z));
        }
        if ev.y < structure.chunks_height() - 1 {
            chunks.insert((x, y + 1, z));
        }
        if ev.x != 0 {
            chunks.insert((x - 1, y, z));
        }
        if ev.x < structure.chunks_width() - 1 {
            chunks.insert((x + 1, y, z));
        }
    }

    for (structure, chunks) in chunks_todo {
        if let Ok(structure) = structure_query.get(structure) {
            for (cx, cy, cz) in chunks {
                if let Some(chunk_entity) = structure.chunk_entity(cx, cy, cz) {
                    if let Some(mut chunk_ent) = commands.get_entity(chunk_entity) {
                        chunk_ent.insert(ChunkNeedsRendered);
                    }
                }
            }
        }
    }
}

#[derive(Component)]
struct ChunkNeedsRendered;

#[derive(Debug, Reflect, FromReflect, Clone, Copy)]
struct LightEntry {
    entity: Entity,
    light: BlockLightProperties,
    position: StructureBlock,
    valid: bool,
}

#[derive(Component, Debug, Reflect, FromReflect, Default)]
struct LightsHolder {
    lights: Vec<LightEntry>,
}

#[derive(Component, Debug, Reflect, FromReflect, Default)]
struct ChunkMeshes(Vec<Entity>);

/// Performance hot spot
fn monitor_needs_rendered_system(
    mut commands: Commands,
    structure_query: Query<&Structure>,
    atlas: Res<MainAtlas>,
    mesh_query: Query<Option<&Handle<Mesh>>>,
    mut meshes: ResMut<Assets<Mesh>>,
    blocks: Res<Registry<Block>>,
    materials: Res<ManyToOneRegistry<Block, CosmosMaterial>>,
    meshes_registry: Res<BlockMeshRegistry>,
    lighting: Res<Registry<BlockLighting>>,
    lights_query: Query<&LightsHolder>,
    chunk_meshes_query: Query<&ChunkMeshes>,
    block_textures: Res<Registry<BlockTextureIndex>>,

    local_player: Query<&GlobalTransform, With<LocalPlayer>>,

    chunks_need_rendered: Query<(Entity, &ChunkEntity, &GlobalTransform), With<ChunkNeedsRendered>>,
) {
    let Ok(local_transform) = local_player.get_single() else {
        return;
    };

    let timer: UtilsTimer = UtilsTimer::start();

    // by making the Vec an Option<Vec> I can take ownership of it later, which I cannot do with
    // just a plain Mutex<Vec>.
    // https://stackoverflow.com/questions/30573188/cannot-move-data-out-of-a-mutex
    let to_process = Mutex::new(Some(Vec::new()));

    let mut todo = chunks_need_rendered
        .iter()
        .map(|(x, y, transform)| {
            (
                x,
                y,
                transform
                    .translation()
                    .distance_squared(local_transform.translation()),
            )
        })
        // Only render chunks that are within a reasonable viewing distance
        .filter(|(_, _, distance_sqrd)| *distance_sqrd < SECTOR_DIMENSIONS * SECTOR_DIMENSIONS)
        .collect::<Vec<(Entity, &ChunkEntity, f32)>>();

    let chunks_per_frame = 10;

    // Only sort first `chunks_per_frame`, so no built-in sort algorithm
    let n: usize = chunks_per_frame.min(todo.len());

    for i in 0..n {
        let mut min = todo[i].2;
        let mut best_i = i;

        for (j, item) in todo.iter().enumerate().skip(i + 1) {
            if item.2 < min {
                min = item.2;
                best_i = j;
            }
        }

        todo.swap(i, best_i);
    }

    // Render chunks in parallel
    todo.par_iter()
        .take(chunks_per_frame)
        .copied()
        .for_each(|(entity, ce, _)| {
            let Ok(structure) = structure_query.get(ce.structure_entity) else {
                return;
            };

            let mut renderer = ChunkRenderer::new();

            let (cx, cy, cz) = ce.chunk_location;

            let Some(chunk) = structure.chunk_from_chunk_coordinates(cx, cy, cz) else {
                return;
            };

            let (xi, yi, zi) = (cx as i32, cy as i32, cz as i32);

            let left = structure.chunk_from_chunk_coordinates_oob(xi - 1, yi, zi);
            let right = structure.chunk_from_chunk_coordinates_oob(xi + 1, yi, zi);
            let bottom = structure.chunk_from_chunk_coordinates_oob(xi, yi - 1, zi);
            let top = structure.chunk_from_chunk_coordinates_oob(xi, yi + 1, zi);
            let back = structure.chunk_from_chunk_coordinates_oob(xi, yi, zi - 1);
            let front = structure.chunk_from_chunk_coordinates_oob(xi, yi, zi + 1);

            renderer.render(
                &atlas,
                &materials,
                &lighting,
                chunk,
                left,
                right,
                bottom,
                top,
                back,
                front,
                &blocks,
                &meshes_registry,
                &block_textures,
            );

            let mut mutex = to_process.lock().expect("Error locking to_process vec!");

            mutex
                .as_mut()
                .unwrap()
                .push((entity, renderer.create_mesh()));
        });

    let to_process_chunks = to_process.lock().unwrap().take().unwrap();

    if !to_process_chunks.is_empty() {
        timer.log_duration(&format!(
            "Rendering {} chunks took",
            to_process_chunks.len()
        ));
    }

    for (entity, mut chunk_mesh) in to_process_chunks {
        commands.entity(entity).remove::<ChunkNeedsRendered>();

        let mut old_mesh_entities = Vec::new();

        if let Ok(chunk_meshes_component) = chunk_meshes_query.get(entity) {
            for ent in chunk_meshes_component.0.iter() {
                let old_mesh_handle = mesh_query
                    .get(*ent)
                    .expect("This should have a mesh component.");

                if let Some(old_mesh_handle) = old_mesh_handle {
                    meshes.remove(old_mesh_handle);
                }

                old_mesh_entities.push(*ent);
            }
        }

        let mut new_lights = LightsHolder::default();

        if let Ok(lights) = lights_query.get(entity) {
            for light in lights.lights.iter() {
                let mut light = *light;
                light.valid = false;
                new_lights.lights.push(light);
            }
        }

        let mut entities_to_add = Vec::new();

        if !chunk_mesh.lights.is_empty() {
            for light in chunk_mesh.lights {
                let (x, y, z) = light.0;
                let properties = light.1;

                let mut found = false;
                for light in new_lights.lights.iter_mut() {
                    if light.position.x == x && light.position.y == y && light.position.z == z {
                        if light.light == properties {
                            light.valid = true;
                            found = true;
                        }
                        break;
                    }
                }

                if !found {
                    let light_entity = commands
                        .spawn(PointLightBundle {
                            point_light: PointLight {
                                color: properties.color,
                                intensity: properties.intensity,
                                range: properties.range,
                                radius: 1.0,
                                // Shadows kill all performance
                                shadows_enabled: false, // !properties.shadows_disabled,
                                ..Default::default()
                            },
                            transform: Transform::from_xyz(
                                x as f32 - (CHUNK_DIMENSIONS as f32 / 2.0 - 0.5),
                                y as f32 - (CHUNK_DIMENSIONS as f32 / 2.0 - 0.5),
                                z as f32 - (CHUNK_DIMENSIONS as f32 / 2.0 - 0.5),
                            ),
                            ..Default::default()
                        })
                        .id();

                    new_lights.lights.push(LightEntry {
                        entity: light_entity,
                        light: properties,
                        position: StructureBlock::new(x, y, z),
                        valid: true,
                    });
                    entities_to_add.push(light_entity);
                }
            }
        }

        for light in new_lights.lights.iter().filter(|x| !x.valid) {
            commands.entity(light.entity).despawn_recursive();
        }

        new_lights.lights.retain(|x| x.valid);

        // end lighting
        // meshes

        // If the chunk previously only had one chunk mesh, then it would be on
        // the chunk entity instead of child entities
        commands
            .entity(entity)
            .remove::<Handle<Mesh>>()
            .remove::<Handle<StandardMaterial>>();

        let mut chunk_meshes_component = ChunkMeshes::default();

        if chunk_mesh.mesh_materials.len() > 1 {
            for mesh_material in chunk_mesh.mesh_materials {
                let mesh = meshes.add(mesh_material.mesh);

                let ent = if let Some(ent) = old_mesh_entities.pop() {
                    commands
                        .entity(ent)
                        .insert(mesh)
                        .insert(mesh_material.material);

                    ent
                } else {
                    let s = (CHUNK_DIMENSIONS / 2) as f32;

                    let ent = commands
                        .spawn((
                            PbrBundle {
                                mesh,
                                material: mesh_material.material,
                                ..Default::default()
                            },
                            Aabb::from_min_max(Vec3::new(-s, -s, -s), Vec3::new(s, s, s)),
                        ))
                        .id();

                    entities_to_add.push(ent);

                    ent
                };

                chunk_meshes_component.0.push(ent);
            }
        } else if !chunk_mesh.mesh_materials.is_empty() {
            // To avoid making too many entities (and tanking performance), if only one mesh
            // is present, just stick the mesh info onto the chunk itself.

            let mesh_material = chunk_mesh
                .mesh_materials
                .pop()
                .expect("This has one element in it");

            let mesh = meshes.add(mesh_material.mesh);
            let s = (CHUNK_DIMENSIONS / 2) as f32;

            commands.entity(entity).insert((
                mesh,
                mesh_material.material,
                Aabb::from_min_max(Vec3::new(-s, -s, -s), Vec3::new(s, s, s)),
            ));
        }

        // Any leftover entities are useless now, so kill them
        for mesh in old_mesh_entities {
            commands.entity(mesh).despawn_recursive();
        }

        let mut entity_commands = commands.entity(entity);

        for ent in entities_to_add {
            entity_commands.add_child(ent);
        }

        entity_commands
            // .insert(meshes.add(chunk_mesh.mesh))
            .insert(new_lights)
            .insert(chunk_meshes_component);
    }
}

#[derive(Default, Debug, Reflect, FromReflect)]
struct ChunkRendererInstance {
    indices: Vec<u32>,
    uvs: Vec<[f32; 2]>,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    lights: HashMap<(usize, usize, usize), BlockLightProperties>,
}

#[derive(Default, Debug, Reflect, FromReflect)]
struct MeshInfo {
    renderer: ChunkRendererInstance,
    mesh_builder: CosmosMeshBuilder,
}

impl MeshBuilder for MeshInfo {
    #[inline]
    fn add_mesh_information(&mut self, mesh_info: &MeshInformation, position: Vec3, uvs: Rect) {
        self.mesh_builder
            .add_mesh_information(mesh_info, position, uvs);
    }

    fn build_mesh(self) -> Mesh {
        self.mesh_builder.build_mesh()
    }
}

#[derive(Default, Debug, Reflect)]
struct ChunkRenderer {
    meshes: HashMap<Handle<StandardMaterial>, MeshInfo>,
    lights: HashMap<(usize, usize, usize), BlockLightProperties>,
}

impl ChunkRenderer {
    fn new() -> Self {
        Self::default()
    }

    /// Renders a chunk into mesh information that can then be turned into a bevy mesh
    fn render(
        &mut self,
        atlas: &MainAtlas,
        materials: &ManyToOneRegistry<Block, CosmosMaterial>,
        lighting: &Registry<BlockLighting>,
        chunk: &Chunk,
        left: Option<&Chunk>,
        right: Option<&Chunk>,
        bottom: Option<&Chunk>,
        top: Option<&Chunk>,
        back: Option<&Chunk>,
        front: Option<&Chunk>,
        blocks: &Registry<Block>,
        meshes: &BlockMeshRegistry,
        block_textures: &Registry<BlockTextureIndex>,
    ) {
        let cd2 = CHUNK_DIMENSIONSF / 2.0;

        let mut faces = Vec::with_capacity(6);

        for ((x, y, z), (block, block_info)) in chunk
            .blocks()
            .copied()
            .zip(chunk.block_info_iterator().copied())
            .enumerate()
            .map(|(i, block)| (expand(i, CHUNK_DIMENSIONS, CHUNK_DIMENSIONS), block))
            .filter(|((x, y, z), _)| chunk.has_block_at(*x, *y, *z))
        {
            let (center_offset_x, center_offset_y, center_offset_z) = (
                x as f32 - cd2 + 0.5,
                y as f32 - cd2 + 0.5,
                z as f32 - cd2 + 0.5,
            );
            // right
            if (x != CHUNK_DIMENSIONS - 1 && chunk.has_see_through_block_at(x + 1, y, z, blocks))
                || (x == CHUNK_DIMENSIONS - 1
                    && (right
                        .map(|c| c.has_see_through_block_at(0, y, z, blocks))
                        .unwrap_or(true)))
            {
                faces.push(BlockFace::Right);
            }
            // left
            if (x != 0 && chunk.has_see_through_block_at(x - 1, y, z, blocks))
                || (x == 0
                    && (left
                        .map(|c| c.has_see_through_block_at(CHUNK_DIMENSIONS - 1, y, z, blocks))
                        .unwrap_or(true)))
            {
                faces.push(BlockFace::Left);
            }

            // top
            if (y != CHUNK_DIMENSIONS - 1 && chunk.has_see_through_block_at(x, y + 1, z, blocks))
                || (y == CHUNK_DIMENSIONS - 1
                    && (top
                        .map(|c| c.has_see_through_block_at(x, 0, z, blocks))
                        .unwrap_or(true)))
            {
                faces.push(BlockFace::Top);
            }
            // bottom
            if (y != 0 && chunk.has_see_through_block_at(x, y - 1, z, blocks))
                || (y == 0
                    && (bottom
                        .map(|c| c.has_see_through_block_at(x, CHUNK_DIMENSIONS - 1, z, blocks))
                        .unwrap_or(true)))
            {
                faces.push(BlockFace::Bottom);
            }

            // back
            if (z != CHUNK_DIMENSIONS - 1 && chunk.has_see_through_block_at(x, y, z + 1, blocks))
                || (z == CHUNK_DIMENSIONS - 1
                    && (front
                        .map(|c| c.has_see_through_block_at(x, y, 0, blocks))
                        .unwrap_or(true)))
            {
                faces.push(BlockFace::Back);
            }
            // front
            if (z != 0 && chunk.has_see_through_block_at(x, y, z - 1, blocks))
                || (z == 0
                    && (back
                        .map(|c| c.has_see_through_block_at(x, y, CHUNK_DIMENSIONS - 1, blocks))
                        .unwrap_or(true)))
            {
                faces.push(BlockFace::Front);
            }

            if !faces.is_empty() {
                let block = blocks.from_numeric_id(block);

                let Some(material) = materials.get_value(block) else {
                    continue;
                };

                let Some(mesh) = meshes.get_value(block) else {
                    continue;
                };

                if !self.meshes.contains_key(&material.handle) {
                    self.meshes
                        .insert(material.handle.clone(), Default::default());
                }

                let mesh_builder = self.meshes.get_mut(&material.handle).unwrap();

                let rotation = block_info.get_rotation();

                for face in faces.iter().map(|x| BlockFace::rotate_face(*x, rotation)) {
                    let index = block_textures
                        .from_id(block.unlocalized_name())
                        .unwrap_or_else(|| {
                            block_textures
                                .from_id("missing")
                                .expect("Missing texture should exist.")
                        });

                    let Some(image_index) = index.atlas_index_from_face(face) else {
                        warn!("Missing image index -- {index:?}");
                        continue;
                    };

                    let uvs = atlas.uvs_for_index(image_index);

                    let mut mesh_info = mesh.info_for_face(face).clone();

                    let rotation = match rotation {
                        BlockFace::Top => Quat::IDENTITY,
                        BlockFace::Front => Quat::from_axis_angle(Vec3::X, PI / 2.0),
                        BlockFace::Back => Quat::from_axis_angle(Vec3::X, -PI / 2.0),
                        BlockFace::Left => Quat::from_axis_angle(Vec3::Z, PI / 2.0),
                        BlockFace::Right => Quat::from_axis_angle(Vec3::Z, -PI / 2.0),
                        BlockFace::Bottom => Quat::from_axis_angle(Vec3::X, PI),
                    };

                    for pos in mesh_info.positions.iter_mut() {
                        *pos = rotation.mul_vec3((*pos).into()).into();
                    }

                    for norm in mesh_info.normals.iter_mut() {
                        *norm = rotation.mul_vec3((*norm).into()).into();
                    }

                    mesh_builder.add_mesh_information(
                        &mesh_info,
                        Vec3::new(center_offset_x, center_offset_y, center_offset_z),
                        uvs,
                    );
                }

                faces.clear();

                if let Some(lighting) = lighting.from_id(block.unlocalized_name()) {
                    self.lights.insert((x, y, z), lighting.properties);
                }
            }
        }
    }

    fn create_mesh(self) -> ChunkMesh {
        let mut mesh_materials = Vec::new();

        for (material, chunk_mesh_info) in self.meshes {
            let mesh = chunk_mesh_info.build_mesh();

            mesh_materials.push(MeshMaterial { material, mesh });
        }

        let lights = self.lights;

        ChunkMesh {
            lights,
            mesh_materials,
        }
    }
}

pub(super) fn register(app: &mut App) {
    app.add_systems(
        (monitor_needs_rendered_system, monitor_block_updates_system)
            .in_set(OnUpdate(GameState::Playing))
            .before(unload_chunks_far_from_players),
    )
    // .add_system(add_renderer)
    .register_type::<LightsHolder>();
}
