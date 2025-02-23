//! This handles the loading of different things in the world, such as planets & ships
//!
//! To add your own loading event, add a system after `begin_loading` and before `done_loading`.
//!
//! Use the query: `Query<(Entity, &SerializedData), With<NeedsLoaded>>` to get all the data that will need
//! loaded. From there, you can add any components necessary to the entity to fully load it in.
//!
//! See [`loading::default_load`] for an example.

use std::fs;

use bevy::{
    prelude::{
        App, Commands, Component, CoreSet, DespawnRecursiveExt, Entity, IntoSystemConfig, Query,
        With, Without,
    },
    reflect::Reflect,
};
use bevy_rapier3d::prelude::Velocity;

use cosmos_core::{
    netty::cosmos_encoder, persistence::LoadingDistance, physics::location::Location,
};

use super::{SaveFileIdentifier, SaveFileIdentifierType, SerializedData};

#[derive(Component, Debug, Reflect)]
/// An entity that currently has this is currently in the process of being loaded
pub struct NeedsLoaded;

fn check_needs_loaded(
    query: Query<(Entity, &SaveFileIdentifier), (Without<SerializedData>, With<NeedsLoaded>)>,
    mut commands: Commands,
) {
    for (ent, nl) in query.iter() {
        let path = nl.get_save_file_path();
        let Ok(data) = fs::read(&path) else {
            eprintln!("Error reading record at '{path}'. Is it corrupted?");
            commands.entity(ent).despawn_recursive();
            continue;
        };

        let serialized_data: SerializedData =
            cosmos_encoder::deserialize(&data).expect("Error deserializing data for {path}");

        commands.entity(ent).insert(serialized_data);

        if let SaveFileIdentifierType::Base((entity_id, _, _)) = &nl.identifier_type {
            commands.entity(ent).insert(entity_id.clone());
        }
    }
}

/// To add your own loading event, add a system after `begin_loading` and before `done_loading`.
pub fn begin_loading() {}

/// To add your own loading event, add a system after `begin_loading` and before `done_loading`.
pub fn done_loading(query: Query<Entity, With<NeedsLoaded>>, mut commands: Commands) {
    for ent in query.iter() {
        commands
            .entity(ent)
            .remove::<NeedsLoaded>()
            .remove::<SerializedData>();
    }
}

fn default_load(
    query: Query<(Entity, &SerializedData), With<NeedsLoaded>>,
    mut commands: Commands,
) {
    for (ent, sd) in query.iter() {
        let mut ecmds = commands.entity(ent);

        if let Some(location) = sd.deserialize_data::<Location>("cosmos:location") {
            ecmds.insert(location);
        }
        if let Some(velocity) = sd.deserialize_data::<Velocity>("cosmos:velocity") {
            ecmds.insert(velocity);
        }
        if let Some(loading_distance) =
            sd.deserialize_data::<LoadingDistance>("cosmos:loading_distance")
        {
            ecmds.insert(loading_distance);
        }
    }
}

pub(super) fn register(app: &mut App) {
    app.add_system(check_needs_loaded.in_base_set(CoreSet::PreUpdate))
        // Put all loading-related systems after this
        .add_system(begin_loading.in_base_set(CoreSet::Update))
        // Put all loading-related systems before this
        .add_system(done_loading.after(begin_loading))
        // Like this:
        .add_system(default_load.after(begin_loading).before(done_loading));
}
