mod server;

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::rc::Rc;
use std::time::{Instant, SystemTime};
use bevy::prelude::*;
use bevy::winit::WinitPlugin;
use bevy_rapier3d::na::Vector3;
use bevy_rapier3d::prelude::{Collider, LockedAxes, RigidBody, Velocity};
use bevy_renet::renet::{RenetServer, ServerAuthentication, ServerConfig, ServerEvent};
use bevy_renet::RenetServerPlugin;
use cosmos_core::block::blocks::{AIR, CHERRY_LEAF, CHERRY_LOG, DIRT, GRASS, STONE};
use cosmos_core::entities::player::Player;
use cosmos_core::netty::netty::{ClientUnreliableMessages, NettyChannel, PROTOCOL_ID, server_connection_config};
use cosmos_core::netty::netty::ServerReliableMessages::{MOTD, PlayerCreate, PlayerRemove, StructureCreate};
use cosmos_core::netty::netty::ServerUnreliableMessages::{BulkBodies};
use cosmos_core::netty::netty_rigidbody::NettyRigidBody;
use cosmos_core::physics::structure_physics::StructurePhysics;
use cosmos_core::plugin::cosmos_core_plugin::CosmosCorePluginGroup;
use cosmos_core::structure::chunk::CHUNK_DIMENSIONS;
use cosmos_core::structure::structure::{BlockChangedEvent, Structure, StructureBlock};
use rand::Rng;

#[derive(Debug, Default)]
pub struct ServerLobby {
    pub players: HashMap<u64, Entity>,
}

#[derive(Debug, Default)]
pub struct NetworkTick (u32);


fn server_sync_bodies(
    mut server: ResMut<RenetServer>,
    mut tick: ResMut<NetworkTick>,
    players: Query<(Entity, &Transform, &Velocity)>) {

    let mut bodies = Vec::new();

    for (entity, transform, velocity) in players.iter() {
        bodies.push((entity.clone(), NettyRigidBody::new(&velocity, &transform)));
    }

    tick.0 += 1;

    let sync_message = BulkBodies {
        time_stamp: tick.0,
        bodies
    };
    let message = bincode::serialize(&sync_message).unwrap();

    server.broadcast_message(NettyChannel::Unreliable.id(), message);
}

#[derive(Default)]
struct ClientTicks {
    ticks: HashMap<u64, Option<u32>>
}

fn server_listen_messages(
    mut server: ResMut<RenetServer>,
    lobby: ResMut<ServerLobby>,
    mut players: Query<(&mut Transform, &mut Velocity), With<Player>>) {

    for client_id in server.clients_id().into_iter() {
        while let Some(message) = server.receive_message(client_id, NettyChannel::Unreliable.id()) {
            let command: ClientUnreliableMessages = bincode::deserialize(&message).unwrap();

            match command {
                ClientUnreliableMessages::PlayerBody { body } => {
                    if let Some(player_entity) = lobby.players.get(&client_id) {
                        if let Ok((mut transform, mut velocity)) = players.get_mut(*player_entity) {
                            transform.translation = body.translation.into();
                            transform.rotation = body.rotation.into();
                            velocity.linvel = body.body_vel.linvel.into();
                            velocity.angvel = body.body_vel.angvel.into();
                        }
                    }
                }
            }
        }
    }
}

fn handle_events_system(
    mut commands: Commands,
    mut server: ResMut<RenetServer>,
    mut server_events: EventReader<ServerEvent>,
    mut lobby: ResMut<ServerLobby>,
    mut client_ticks: ResMut<ClientTicks>,
    players: Query<(Entity, &Player, &Transform, &Velocity)>,
    structures_query: Query<(Entity, &Structure, &Transform, &Velocity)>)
{
    for event in server_events.iter() {
        match event {
            ServerEvent::ClientConnected(id, _user_data) => {
                println!("Client {} connected", id);

                for (entity, player, transform, velocity) in players.iter() {
                    let body = NettyRigidBody::new(&velocity, &transform);

                    let msg = bincode::serialize(&PlayerCreate {
                        entity,
                        id: player.id,
                        body,
                        name: player.name.clone(),
                    }).unwrap();

                    server.send_message(*id, NettyChannel::Reliable.id(), msg);
                }

                let name = "epic nameo";
                let player = Player::new(String::from(name), *id);
                let transform = Transform::from_xyz(0.0, 60.0, 0.0);
                let velocity = Velocity::default();

                let netty_body = NettyRigidBody::new(&velocity, &transform);

                let mut player_entity = commands.spawn();
                player_entity.insert(transform);
                player_entity.insert(LockedAxes::ROTATION_LOCKED);
                player_entity.insert(RigidBody::Dynamic);
                player_entity.insert(velocity);
                player_entity.insert(Collider::capsule_y(0.5, 0.25));
                player_entity.insert(player);

                lobby.players.insert(*id, player_entity.id());

                let msg = bincode::serialize(&PlayerCreate {
                    entity: player_entity.id(),
                    id: *id,
                    name: String::from(name),
                    body: netty_body
                }).unwrap();

                server.send_message(*id, NettyChannel::Reliable.id(), bincode::serialize(&MOTD {
                    motd: "Welcome to the server!".into()
                }).unwrap());

                server.broadcast_message(NettyChannel::Reliable.id(), msg);

                for (entity, structure, transform, velocity) in structures_query.iter() {
                    server.send_message(*id, NettyChannel::Reliable.id(),
                                        bincode::serialize(&StructureCreate {
                        entity: entity.clone(),
                        body: NettyRigidBody::new(velocity, transform),
                        serialized_structure: bincode::serialize(structure).unwrap()
                    }).unwrap());
                }
            }
            ServerEvent::ClientDisconnected(id) => {
                println!("Client {} disconnected", id);

                client_ticks.ticks.remove(id);
                if let Some(player_entity) = lobby.players.remove(&id) {
                    commands.entity(player_entity).despawn();
                }

                let message = bincode::serialize(&PlayerRemove {
                    id: *id
                }).unwrap();

                server.broadcast_message(NettyChannel::Reliable.id(), message);
            }
        }
    }
}

fn create_structure(mut commands: Commands,
    mut event_writer: EventWriter<BlockChangedEvent>) {
    let mut entity_cmd = commands.spawn();

    let mut structure = Structure::new(1, 1, 1, entity_cmd.id());

    let physics_updater = StructurePhysics::new(&structure, entity_cmd.id());

    let mut now = Instant::now();
    for z in 0..CHUNK_DIMENSIONS * structure.length() {
        for x in 0..CHUNK_DIMENSIONS * structure.width() {
            let y: f32 = (CHUNK_DIMENSIONS * structure.height()) as f32 - ((x + z) as f32 / 12.0).sin().abs() * 4.0 - 10.0;

            let y_max = y.ceil() as usize;
            for yy in 0..y_max {
                if yy == y_max - 1 {
                    structure.set_block_at(x, yy, z, &GRASS, &mut event_writer);

                    let mut rng = rand::thread_rng();

                    let n1: u8 = rng.gen();

                    if n1 < 1 {
                        for ty in (yy+1)..(yy + 7) {
                            if ty != yy + 6 {
                                structure.set_block_at(x, ty, z, &CHERRY_LOG, &mut event_writer);
                            }
                            else {
                                structure.set_block_at(x, ty, z, &CHERRY_LEAF, &mut event_writer);
                            }

                            if ty > yy + 2 {
                                let range;
                                if ty < yy + 5 {
                                    range = -2..3;
                                }
                                else {
                                    range = -1..2;
                                }

                                for tz in range.clone() {
                                    for tx in range.clone() {
                                        if tx == 0 && tz == 0 || (tx + (x as i32) < 0 || tz + (z as i32) < 0 || ((tx + (x as i32)) as usize) >= structure.width() * 32 || ((tz + (z as i32)) as usize) >= structure.length() * 32) {
                                            continue;
                                        }
                                        structure.set_block_at((x as i32 + tx) as usize, ty, (z as i32 + tz) as usize, &CHERRY_LEAF, &mut event_writer);
                                    }
                                }
                            }
                        }
                    }
                }
                else if yy > y_max - 5 {
                    structure.set_block_at(x, yy, z, &DIRT, &mut event_writer);
                }
                else {
                    structure.set_block_at(x, yy, z, &STONE, &mut event_writer);
                }
            }
        }
    }

    println!("Done in {}ms", now.elapsed().as_millis());

    entity_cmd.insert_bundle(PbrBundle {
        transform: Transform {
            translation: Vec3::new(0.0, 0.0, 0.0),
            ..default()
        },
        ..default()
    })
        .insert(RigidBody::Fixed)
        .insert(Velocity::default())
        .with_children(|parent| {
            for z in 0..structure.length() {
                for y in 0..structure.height() {
                    for x in 0..structure.width() {
                        let mut entity = parent.spawn().id();

                        structure.set_chunk_entity(x, y, z, entity);
                    }
                }
            }
        })
        .insert(physics_updater);

    let block = structure.block_at(0, 0, 0);
    entity_cmd.insert(structure);

    // TODO: Replace this with a better event that makes it clear it's a new structure
    event_writer.send(BlockChangedEvent {
        block: StructureBlock::new(0, 0, 0),
        structure_entity: entity_cmd.id(),
        old_block: &AIR,
        new_block: block
    });
}

fn main() {

    let port: u16 = 1337;

    let address: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let socket = UdpSocket::bind(address).unwrap();

    let server_config = ServerConfig::new(20, PROTOCOL_ID, address, ServerAuthentication::Unsecure);
    let connection_config = server_connection_config(); //RenetConnectionConfig::default();
    let cur_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();

    let server = RenetServer::new(cur_time, server_config, connection_config, socket).unwrap();

    App::new()
        .add_plugins(CosmosCorePluginGroup::default())
        .add_plugin(RenetServerPlugin)
        .add_plugin(WinitPlugin::default())

        .insert_resource(ServerLobby::default())
        .insert_resource(NetworkTick(0))
        .insert_resource(ClientTicks::default())
        .insert_resource(server)
        .add_event::<BlockChangedEvent>()

        .add_startup_system(create_structure)

        .add_system(server_listen_messages)
        .add_system(server_sync_bodies)
        .add_system(handle_events_system)

        .run();
}