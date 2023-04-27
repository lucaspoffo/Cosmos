//! Responsible for determining how structures are added to the game when they are needed

use bevy::{
    ecs::system::EntityCommands,
    prelude::{PbrBundle, Transform},
};
use bevy_rapier3d::prelude::Velocity;

use crate::{physics::location::Location, structure::Structure};

/// Used to instantiate structures
pub trait TStructureBuilder {
    /// Builds that structure
    fn insert_structure(
        &self,
        entity: &mut EntityCommands,
        location: Location,
        world_location: &Location,
        velocity: Velocity,
        structure: &mut Structure,
    );
}
#[derive(Default, Debug)]
/// The default structure builder
pub struct StructureBuilder;

impl TStructureBuilder for StructureBuilder {
    fn insert_structure(
        &self,
        entity: &mut EntityCommands,
        mut location: Location,
        world_location: &Location,
        velocity: Velocity,
        structure: &mut Structure,
    ) {
        structure.set_entity(entity.id());

        let relative_coords = world_location.relative_coords_to(&location);

        location.last_transform_loc = Some(relative_coords);

        entity
            .insert(PbrBundle {
                transform: Transform::from_translation(relative_coords),
                ..Default::default()
            })
            .insert((velocity, location));
    }
}
