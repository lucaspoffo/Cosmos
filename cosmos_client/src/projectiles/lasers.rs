use bevy::prelude::*;
use bevy_renet::renet::*;
use cosmos_core::{
    netty::{server_laser_cannon_system_messages::ServerLaserCannonSystemMessages, NettyChannel},
    projectiles::laser::Laser,
};

use crate::{netty::mapping::NetworkMapping, state::game_state::GameState};

#[derive(Resource)]
struct LaserMesh(Handle<Mesh>);

fn create_laser_mesh(mut meshes: ResMut<Assets<Mesh>>, mut commands: Commands) {
    commands.insert_resource(LaserMesh(
        meshes.add(Mesh::from(shape::Box::new(0.1, 0.1, 1.0))),
    ));
}

fn lasers_netty(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    network_mapping: Res<NetworkMapping>,
    laser_mesh: Res<LaserMesh>,
) {
    while let Some(message) = client.receive_message(NettyChannel::LaserCannonSystem.id()) {
        let msg: ServerLaserCannonSystemMessages = bincode::deserialize(&message).unwrap();

        match msg {
            ServerLaserCannonSystemMessages::CreateLaser {
                color,
                position,
                laser_velocity,
                firer_velocity,
                strength,
                mut no_hit,
            } => {
                if let Some(server_entity) = no_hit {
                    if let Some(client_entity) = network_mapping.client_from_server(&server_entity)
                    {
                        no_hit = Some(*client_entity);
                    }
                }

                Laser::spawn_custom_pbr(
                    position,
                    laser_velocity,
                    firer_velocity,
                    strength,
                    no_hit,
                    PbrBundle {
                        mesh: laser_mesh.0.clone(),
                        material: materials.add(StandardMaterial {
                            base_color: color,
                            // emissive: color,
                            unlit: true,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    &time,
                    &mut commands,
                );
            }
        }
    }
}

pub(crate) fn register(app: &mut App) {
    app.add_system_set(SystemSet::on_enter(GameState::Loading).with_system(create_laser_mesh))
        .add_system_set(SystemSet::on_update(GameState::Playing).with_system(lasers_netty));
}