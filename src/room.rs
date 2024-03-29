use std::fmt;

use log::{info, trace};
use std::sync::atomic;
use std::time::Instant;

use crate::cache_system::*;
use crate::protocol::*;
use log::error;
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
    // pub packets: ProtocolPackets,
    pub ip_addr: SocketAddr,
    pub user_id: u16,
    pub name: Vec<u8>,
    pub emul_name: String,
    pub ping: u32,
    pub connect_type: u8,
    pub atomic_input_size: u8,
    pub player_status: PlayerStatus,
    pub ack_count: u32,
    pub send_count: u16,
    pub cur_seq: u16,
    pub game_room_id: Option<u32>,
    pub room_order: u8,
    pub out_packets: Vec<Protocol>,
    pub in_packets: ProtocolPackets,
    pub player_index: u8,
    pub players_input: Vec<Vec<u8>>,
    pub cache_system: CacheSystem,
    pub put_cache: CacheSystem,
    pub s2c_ack_time: Instant,
    pub pings: Vec<i32>,
    pub keepalive_time: Instant,
}

impl User {
    pub fn new(ip_addr: SocketAddr) -> User {
        User {
            user_id: 0,
            name: vec![0u8],
            emul_name: "".to_string(),
            ping: 0,
            connect_type: 0,
            atomic_input_size: 0,
            player_status: Idle,
            ack_count: 0,
            send_count: 0,
            cur_seq: 0,
            game_room_id: Option::None,
            room_order: 0,
            ip_addr,
            out_packets: Vec::new(),
            in_packets: ProtocolPackets::new(),
            player_index: 0,
            players_input: Vec::new(),
            cache_system: CacheSystem::new(),
            put_cache: CacheSystem::new(),
            pings: Vec::new(),
            s2c_ack_time: Instant::now(),
            keepalive_time: Instant::now(),
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
        self.out_packets.push(p);
        let extra_packets = cmp::min(3, self.out_packets.len());
        let mut packet = Vec::new();
        packet.push(extra_packets as u8);
        let packetLen = self.out_packets.len();
        for i in 0..extra_packets {
            let prev_procotol = self
                .out_packets
                .get_mut(packetLen - 1 - i)
                .ok_or(KailleraError::NotFound)?;
            let mut prev_packet = prev_procotol.make_packet()?;
            packet.append(&mut prev_packet);
        }
        server_socket.send_to(&packet, ip_addr).await?;
        self.send_count = self.send_count.wrapping_add(1);
        Ok(())
    }
    pub async fn send_message(
        &mut self,
        server_socket: &mut UdpSocket,
        message: Vec<u8>,
    ) -> anyhow::Result<()> {
        let data = GlobalChat2Client::new(b"Server".to_vec(), message).packetize()?;
        let p = Protocol::new(GLOBAL_CHAT, data);
        self.make_send_packet(server_socket, p).await?;
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub enum PlayerAddr {
    Playing(SocketAddr),
    Idle(SocketAddr),
    None,
}

impl PlayerAddr {
    fn is_none(&self) -> bool {
        match self {
            PlayerAddr::None => true,
            _ => false,
        }
    }
    fn is_playing(&self) -> bool {
        match self {
            PlayerAddr::Playing(_) => true,
            _ => false,
        }
    }
}
#[derive(Debug)]
pub struct Room {
    pub game_name: String,
    pub game_id: u32,
    pub emul_name: String,
    pub creator_id: String,
    // quitting user in game is None
    pub players: Vec<PlayerAddr>,
    pub game_status: GameStatus,
    pub same_delay: bool,
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
            same_delay: false,
        }
    }
    pub fn player_some_count(&self) -> usize {
        self.players
            .iter()
            .filter(|&n| {
                if let PlayerAddr::None = n {
                    false
                } else {
                    true
                }
            })
            .count()
    }
}

#[derive(Error, Debug)]
pub enum KailleraError {
    #[error("{}, pos: {}", .message, .pos)]
    InvalidInput { message: String, pos: usize },
    #[error("token error")]
    TokenError,
    #[error("{}", .message)]
    AlreadyError { message: String },
    #[error("gamestatus error")]
    GameStatusError { message: String },
    #[error("notfound seq")]
    NotFoundSeq { wanted_seq: u16, cur_seq: u16 },
    #[error("notfound error")]
    NotFound,
    #[error("notfound user")]
    NotFoundUser { message: String },
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
                uu.game_room_id.is_some(),
                uu.room_order,
                uu.player_index
            );
        }
        // game room information
        for (_addr, r) in &self.rooms {
            let r = r.borrow();
            ret += &format!(
                "game_id: {}, game_name: {}, ",
                r.game_id,
                &r.game_name.clone()[..3],
            );
            ret += "players: [";
            let mut comma = "".to_string();
            for p in &r.players {
                ret += &format!("{} {:?}", comma, p);
                comma = ", ".to_string();
            }
            ret += "\n";
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
    pub fn gen_input(user: Rc<RefCell<User>>, room: Rc<RefCell<Room>>) -> anyhow::Result<Vec<u8>> {
        let conntype = user.borrow().connect_type;
        let players_num = room.borrow().players.len();
        let atomic_length = user.borrow().atomic_input_size;
        let mut all_input = true;
        for i in 0..players_num {
            trace!(
                "user name: {}, player num: {}, player exist: {}",
                String::from_utf8_lossy(&user.borrow().name),
                i,
                !room.borrow().players[i].is_none()
            );
        }
        for i in 0..players_num {
            let is_exist_user = room.borrow().players[i].is_playing();
            if !is_exist_user {
                info!("because user is not exist, make game data");
                let t = &mut user.borrow_mut().players_input[i];
                let require_bytes = conntype * atomic_length as u8;
                if t.len() < require_bytes as usize {
                    t.append(&mut vec![0; require_bytes as usize - t.len()]);
                }
                assert!(t.len() >= require_bytes as usize);
            }
            let l = match user.borrow().players_input.get(i) {
                Some(i) => i.len() as u8,
                None => break,
            };
            if l < conntype * atomic_length {
                all_input = false;
                break;
            }
        }
        if !all_input {
            anyhow::bail!("yet");
        }

        let mut ret = Vec::new();
        for _ in 0..conntype {
            for i in 0..players_num {
                {
                    let mut t =
                        user.borrow().players_input[i].clone()[..atomic_length as usize].to_vec();
                    ret.append(&mut t);
                }
                {
                    let t = &mut user.borrow_mut().players_input[i];
                    *t = t[atomic_length as usize..].to_vec();
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
    pub fn add_room(&mut self, ch: u32, r: Rc<RefCell<Room>>) -> Result<(), KailleraError> {
        match self.rooms.get(&ch) {
            Some(_s) => {
                return Err(KailleraError::AlreadyError {
                    message: "room is already exist".to_string(),
                });
            }
            None => {
                self.rooms.insert(ch, r);
            }
        }
        Ok(())
    }
    pub fn delete_room(&mut self, ch: u32) -> Result<(), KailleraError> {
        match self.rooms.remove(&ch) {
            Some(_s) => {}
            None => {
                return Err(KailleraError::NotFound);
            }
        }
        Ok(())
    }
    pub fn make_server_status(&self, exclude: SocketAddr) -> anyhow::Result<Protocol> {
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
                &mut format!("{}/{}\x00", i.1.borrow().player_some_count(), 4)
                    .as_bytes()
                    .to_vec(),
            );
            data.push(i.1.borrow().game_status);
        }
        let p = Protocol::new(USER_SERVER_STATUS, data);
        Ok(p)
    }

    // send GAME_CHAT to players of room
    pub async fn send_game_chat_to_players(
        &mut self,
        server_socket: &mut UdpSocket,
        room: Rc<RefCell<Room>>,
        who: String,
        message: Vec<u8>,
    ) -> anyhow::Result<()> {
        // send GAME_CHAT to players of room
        for i in &room.borrow().players {
            let mut data = Vec::new();
            data.append(&mut who.clone().into_bytes());
            data.push(0u8);
            data.append(&mut message.clone());
            match i {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => {
                    let u = self.get_user(*i)?;
                    u.borrow_mut()
                        .make_send_packet(server_socket, Protocol::new(GAME_CHAT, data))
                        .await?;
                }
                PlayerAddr::None => {}
            }
        }
        Ok(())
    }
}
