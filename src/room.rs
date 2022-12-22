use std::error::Error;
use std::fmt;
use std::str::from_utf8_unchecked_mut;
use std::time::Instant;

use crate::cache_system::*;
use crate::protocol::*;
use log::{error, info, log_enabled, trace, warn, Level, LevelFilter};
use serde::__private::from_utf8_lossy;
use std::cell::RefCell;
use std::rc::Rc;
use std::{cmp, collections::HashMap, net::SocketAddr};
use thiserror::Error;
use tokio::net::UdpSocket;

type PlayerStatus = u8;
pub const Playing: PlayerStatus = 0;
pub const Idle: PlayerStatus = 1;
type PlayerInput = Vec<u8>;
pub struct User {
    pub ip_addr: SocketAddr,
    pub user_id: u16,
    pub name: Vec<u8>,
    pub emul_name: String,
    pub ping: u32,
    pub connect_type: u8,
    pub player_status: PlayerStatus,
    pub ack_count: u32,
    pub send_count: u16,
    pub cur_seq: u16,
    pub game_room_id: u32,
    pub in_room: bool,
    pub room_order: u8,
    pub packets: Vec<Protocol>,
    pub player_index: u8,
    pub players_input: Vec<Vec<u8>>,
    pub cache_system: CacheSystem,
    pub put_cache: CacheSystem,
    pub s2c_ack_time: Instant,
    pub pings: Vec<i32>,
}

impl User {
    pub fn new(ip_addr: SocketAddr) -> User {
        User {
            user_id: 0,
            name: vec![0u8],
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
            player_index: 0,
            players_input: Vec::new(),
            cache_system: CacheSystem::new(),
            put_cache: CacheSystem::new(),
            pings: Vec::new(),
            s2c_ack_time: Instant::now(),
        }
    }
    pub fn reset_outcoming(&mut self) {
        self.cache_system.reset();
        self.put_cache.reset();
        self.players_input.clear();
        self.players_input.resize(32, Vec::new());
    }

    pub async fn make_send_packet(
        &mut self,
        server_socket: &mut UdpSocket,
        mut p: Protocol,
    ) -> anyhow::Result<()> {
        // } Result<(), Box<dyn Error>> {
        // self.server_socket.send_to(b"hihi", self.ip_addr).await?;
        let ip_addr = self.ip_addr;
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
        server_socket.send_to(&packet, ip_addr).await?;
        self.send_count = self.send_count.wrapping_add(1);
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
        self.players.iter().filter(|&n| n.is_some()).count()
    }
}

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
    pub rooms: HashMap<u32, Rc<RefCell<Room>>>,
    pub next_user_id: u16,
}
impl fmt::Display for UserRoom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut ret = "".to_string();
        // lobby user information
        for (addr, u) in &self.users {
            let uu = u.borrow();
            ret += &format!(
                "{}: user_id: {}, user_name: {}, in_room: {}, room_order: {}, player_order: {}\n",
                addr,
                uu.user_id,
                from_utf8_lossy(uu.name.clone().as_slice()),
                uu.in_room,
                uu.room_order,
                uu.player_index
            );
        }
        // game room information
        for (addr, r) in &self.rooms {
            let r = r.borrow();
            ret += &format!(
                "game_id: {}, game_name: {}, ",
                r.game_id,
                r.game_name.clone()[..3].to_string(),
            );
            ret += &format!("players: [");
            let mut comma = "".to_string();
            for p in &r.players {
                ret += &format!("{} {:?}", comma, p);
                comma = ", ".to_string();
            }
            ret += &"\n".to_string();
        }
        write!(f, "{}", ret)
    }
}

impl UserRoom {
    pub fn new() -> UserRoom {
        UserRoom {
            users: HashMap::new(),
            rooms: HashMap::new(),
            next_user_id: 0,
        }
    }
    //

    // 각 유저는 다른 플레이어에 대한 입력키를 다 가지고 있다.
    // user 에게 보낼 입력데이터를 만드는 함수
    pub fn gen_input(user: Rc<RefCell<User>>, players_num: usize) -> anyhow::Result<Vec<u8>> {
        let require_inputs = user.borrow().connect_type;

        let mut all_input = true;
        for i in 0..players_num {
            let l = match user.borrow().players_input.get(i as usize) {
                Some(i) => i.len() as u8,
                None => break,
            };
            if l < require_inputs * 2 {
                all_input = false;
                break;
            }
        }
        if !all_input {
            anyhow::bail!("yet");
        }

        let mut ret = Vec::new();
        for _ in 0..require_inputs {
            for i in 0..(players_num as usize) {
                {
                    let mut t = user.borrow().players_input[i].clone()[..2].to_vec();
                    ret.append(&mut t);
                }
                {
                    let t = &mut user.borrow_mut().players_input[i];
                    *t = t[2..].to_vec();
                }
            }
        }
        Ok(ret)
    }
    pub fn test_func(&mut self) {}
    pub fn get_room(&mut self, game_id: u32) -> Result<Rc<RefCell<Room>>, KailleraError> {
        let r = self.rooms.get(&game_id).ok_or(KailleraError::NotFound)?;
        Ok(r.clone())
    }
    pub fn get_user(&mut self, ip_addr: SocketAddr) -> Result<Rc<RefCell<User>>, KailleraError> {
        let user = self.users.get(&ip_addr).ok_or(KailleraError::NotFound)?;
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
                self.rooms.insert(ch, Rc::new(RefCell::new(r)));
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
    ) -> anyhow::Result<Protocol> {
        let mut data = Vec::new();
        data.push(0u8);
        data.append(&mut bincode::serialize::<u32>(
            &(self.users.len() as u32 - 1),
        )?);
        data.append(&mut bincode::serialize::<u32>(&(self.rooms.len() as u32))?);

        for i in &self.users {
            let u = i.1.borrow();
            let ip_addr = u.ip_addr;
            if ip_addr != exclude {
                data.append(&mut u.name.clone());
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
            data.append(&mut i.1.borrow().game_name.clone().into_bytes());
            data.push(0u8);
            data.append(&mut bincode::serialize::<u32>(&i.1.borrow().game_id)?);
            data.append(&mut i.1.borrow().emul_name.clone().into_bytes());
            data.push(0u8);
            data.append(&mut i.1.borrow().creator_id.clone().into_bytes());
            data.push(0u8);
            data.append(
                &mut format!("{}/{}\x00", i.1.borrow().player_count(), 4)
                    .as_bytes()
                    .to_vec(),
            );
            data.push(i.1.borrow().game_status);
        }
        let mut p = Protocol::new(USER_SERVER_STATUS, data);
        p.header.seq = seq;
        Ok(p)
    }
}
