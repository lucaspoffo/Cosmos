use bevy::prelude::App;

pub mod block_events;

pub fn register(app: &mut App) {
    block_events::register(app);
}
