//! Handles client-related structure things

use bevy::prelude::App;

pub mod asteroid;
pub mod chunk_retreiver;
pub mod client_structure_builder;
pub mod planet;
pub mod ship;
mod systems;

pub(super) fn register(app: &mut App) {
    systems::register(app);
    chunk_retreiver::register(app);
    ship::register(app);
    planet::register(app);
    asteroid::register(app);
}
