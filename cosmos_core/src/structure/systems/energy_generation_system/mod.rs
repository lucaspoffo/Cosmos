use bevy::{ecs::schedule::StateData, prelude::App};

pub mod energy_generation_system;

pub fn register<T: StateData + Clone>(app: &mut App, post_loading_state: T, playing_state: T) {
    energy_generation_system::register(app, post_loading_state, playing_state);
}