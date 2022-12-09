use std::error::Error;

use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
use std::cell::RefCell;
use std::rc::Rc;
use std::{
    cmp,
    collections::HashMap,
    hash::Hash,
    net::{IpAddr, SocketAddr},
};
use tokio::net::UdpSocket;

type PlayerStatus = u8;
pub const Playing: PlayerStatus = 0;
pub const Idle: PlayerStatus = 1;
pub struct User {
    pub ip_addr: SocketAddr,
    pub user_id: u16,
    pub name: String,
    pub emul_name: String,
    pub ping: u32,
    pub connect_type: u8,
    pub player_status: PlayerStatus,
    pub ack_count: u32,
    pub send_count: u16,
    pub cur_seq: u16,
    pub game_room_id: u32,
    pub in_room: bool,
    pub room_order: i32,
    pub packets: Vec<Protocol>,
    pub player_order: i32,
    pub players_input: Vec<Vec<u8>>,
    pub cache_system: CacheSystem,
    pub put_cache: CacheSystem,
}

impl User {
    pub fn new(ip_addr: SocketAddr) -> User {
        User {
            user_id: 0,
            name: "".to_string(),
            emul_name: "".to_string(),
            ping: 0,
            connect_type: 0,
            player_status: Idle,
            ack_count: 0,
            send_count: 0,
            cur_seq: 0,
            game_room_id: 0,
            in_room: false,
            room_order: 0,
            ip_addr,
            packets: Vec::new(),
            player_order: 0,
            players_input: Vec::new(),
            cache_system: CacheSystem::new(),
            put_cache: CacheSystem::new(),
        }
    }
    pub async fn haha(&mut self, server_socket: &mut UdpSocket) {}
    pub async fn make_send_packet(
        &mut self,
        server_socket: &mut UdpSocket,
        mut p: Protocol,
    ) -> Result<(), Box<dyn Error>> {
        // self.server_socket.send_to(b"hihi", self.ip_addr).await?;
        p.header.seq = self.send_count;
        self.packets.push(p);
        let extra_packets = cmp::min(3, self.packets.len());
        let mut packet = Vec::new();
        packet.push(extra_packets as u8);
        let packetLen = self.packets.len();
        for i in 0..extra_packets {
            let prev_procotol = self
                .packets
                .get_mut(packetLen - 1 - i)
                .ok_or(KailleraError::NotFound)?;
            let mut prev_packet = prev_procotol.make_packet()?;
            packet.append(&mut prev_packet);
        }
        server_socket.send_to(&packet, self.ip_addr).await?;
        self.send_count += 1;
        Ok(())
    }
}
#[derive(Debug)]
pub struct Room {
    pub game_name: String,
    pub game_id: u32,
    pub emul_name: String,
    pub creator_id: String,
    pub players: Vec<Option<SocketAddr>>,
    pub game_status: u8,
}

impl Room {
    pub fn new() -> Room {
        Room {
            game_name: "".to_string(),
            game_id: 0,
            emul_name: "".to_string(),
            creator_id: "".to_string(),
            players: Vec::new(),
            game_status: 0,
        }
    }
    pub fn player_count(self: &Self) -> usize {
        self.players.iter().filter(|&n| n.is_none()).count()
    }
}

use thiserror::Error;

use crate::service_server::*;

#[derive(Error, Debug)]
pub enum KailleraError {
    #[error("{}, pos: {}", .message, .pos)]
    InvalidInput { message: String, pos: usize },
    #[error("token error")]
    TokenError,
    #[error("{}, pos: {}", .message, .pos)]
    AlreadyError { message: String, pos: usize },
    #[error("notfound error")]
    NotFound,
}

pub struct UserRoom {
    pub users: HashMap<SocketAddr, Rc<RefCell<User>>>,
    pub rooms: HashMap<u32, Box<Room>>,
    pub next_user_id: u16,
}

impl UserRoom {
    pub fn new() -> UserRoom {
        UserRoom {
            users: HashMap::new(),
            rooms: HashMap::new(),
            next_user_id: 0,
        }
    }
    pub fn test_func(&mut self) {}
    pub fn get_user(&mut self, ip_addr: SocketAddr) -> Result<Rc<RefCell<User>>, Box<dyn Error>> {
        let user = self.users.get(&ip_addr).ok_or(KailleraError::NotFound)?;
        // return user;
        // let user = user.get_mut(&ip_addr);
        // let user = self
        //     .users
        //     .get_mut(&ip_addr)
        //     .ok_or(KailleraError::NotFound)?;
        Ok(user.clone())
    }
    pub fn add_room(self: &mut Self, ch: u32, r: Room) -> Result<(), KailleraError> {
        match self.rooms.get(&ch) {
            Some(s) => {
                return Err(KailleraError::AlreadyError {
                    message: "room is already exist".to_string(),
                    pos: 0,
                });
            }
            None => {
                self.rooms.insert(ch, Box::new(r));
            }
        }
        Ok(())
    }
    pub fn delete_room(self: &mut Self, ch: u32) -> Result<(), KailleraError> {
        match self.rooms.remove(&ch) {
            Some(s) => {}
            None => {
                return Err(KailleraError::NotFound);
            }
        }
        Ok(())
    }
    pub fn make_server_status(
        self: &Self,
        seq: u16,
        exclude: SocketAddr,
    ) -> Result<Protocol, Box<dyn Error>> {
        let mut data = Vec::new();
        data.push(0u8);
        data.append(&mut bincode::serialize::<u32>(
            &(self.users.len() as u32 - 1),
        )?);
        data.append(&mut bincode::serialize::<u32>(&(self.rooms.len() as u32))?);

        for i in &self.users {
            let u = i.1.borrow();
            if u.ip_addr != exclude {
                data.append(&mut u.name.clone().into_bytes());
                data.push(0u8);
                data.append(&mut bincode::serialize::<u32>(&u.ping)?);
                data.push(
                    num::ToPrimitive::to_u8(&u.player_status).ok_or(KailleraError::NotFound)?,
                );
                data.append(&mut bincode::serialize::<u16>(&u.user_id)?);
                data.push(u.connect_type);
            }
        }
        for i in &self.rooms {
            data.append(&mut i.1.game_name.clone().into_bytes());
            data.push(0u8);
            data.append(&mut bincode::serialize::<u32>(&i.1.game_id)?);
            data.append(&mut i.1.emul_name.clone().into_bytes());
            data.push(0u8);
            data.append(&mut i.1.creator_id.clone().into_bytes());
            data.push(0u8);
            // let players = format!("%d/%d", i.1.player_count(), 4).as_bytes();
            data.append(
                &mut format!("{}/{}\x00", i.1.player_count(), 4)
                    .as_bytes()
                    .to_vec(),
            );
            data.push(i.1.game_status);
        }
        let mut p = Protocol::new(USER_SERVER_STATUS, data);
        p.header.seq = seq;
        Ok(p)
    }
}

#[derive(Debug)]
pub struct CacheSystem {
    pub position: u8,
    pub incoming_data: HashMap<u8, Vec<u8>>,
    pub incoming_hit_cache: HashMap<Vec<u8>, u8>,
}

impl CacheSystem {
    pub fn new() -> CacheSystem {
        CacheSystem {
            position: 0,
            incoming_data: HashMap::new(),
            incoming_hit_cache: HashMap::new(),
        }
    }
    pub fn get_cache_position(self, b: Vec<u8>) -> Result<u8, KailleraError> {
        match self.incoming_hit_cache.get(&b) {
            Some(s) => Ok(*s),
            None => {
                return Err(KailleraError::NotFound);
            }
        }
    }
    pub fn put_data(self: &mut Self, b: Vec<u8>) -> u8 {
        match self.incoming_hit_cache.get(&b) {
            Some(s) => *s,
            None => {
                self.incoming_data.insert(self.position, b.clone());
                self.incoming_hit_cache.insert(b, self.position);
                self.position += 1;
                if self.position >= 250 {
                    println!("warning cache");
                }
                self.position - 1
            }
        }
    }
    pub fn get_data(self, pos: u8) -> Result<Vec<u8>, KailleraError> {
        match self.incoming_data.get(&pos) {
            Some(s) => Ok(s.clone()),
            None => Err(KailleraError::NotFound),
        }
    }
}
