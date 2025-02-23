//! Handles the initialization of the server world

use std::fs;

use bevy::prelude::*;
use cosmos_core::{netty::cosmos_encoder, utils::resource_wrapper::ResourceWrapper};
use serde::{Deserialize, Serialize};

#[derive(Debug, Resource, Deref, Serialize, Deserialize, Clone, Copy)]
/// This sets the seed the server uses to generate the universe
pub struct ServerSeed(u64);

impl ServerSeed {
    /// Gets the u64 representation of this seed
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Gets the u32 representation of this seed
    pub fn as_u32(&self) -> u32 {
        self.0 as u32
    }
}

pub(super) fn register(app: &mut App) {
    let server_seed = if let Ok(seed) = fs::read("./world/seed.dat") {
        cosmos_encoder::deserialize::<ServerSeed>(&seed)
            .expect("Unable to understand './world/seed.dat' seed file. Is it corrupted?")
    } else {
        let seed = ServerSeed(rand::random());

        fs::create_dir("./world/").expect("Error creating world directory!");
        fs::write("./world/seed.dat", cosmos_encoder::serialize(&seed))
            .expect("Error writing file './world/seed.dat'");

        seed
    };

    app.insert_resource(ResourceWrapper(noise::OpenSimplex::new(
        server_seed.as_u32(),
    )))
    .insert_resource(server_seed);
}
