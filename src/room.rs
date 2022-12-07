use std::{
    collections::HashMap,
    hash::Hash,
    net::{IpAddr, SocketAddr},
};

#[derive(Debug)]
pub enum PlayerStatus {
    Playing,
    Idle,
}
#[derive(Debug)]
pub struct User {
    pub ip_addr: SocketAddr,
    pub user_id: u16,
    pub name: String,
    pub emul_name: String,
    pub ping: u32,
    pub connect_type: u8,
    pub player_status: PlayerStatus,
    pub ack_count: u32,
    pub send_count: i32,
    pub cur_seq: u16,
    pub game_room_id: u32,
    pub in_room: bool,
    pub room_order: i32,
}

impl User {
    pub fn new(ip_addr: SocketAddr) -> User {
        User {
            user_id: 0,
            name: "".to_string(),
            emul_name: "".to_string(),
            ping: 0,
            connect_type: 0,
            player_status: PlayerStatus::Idle,
            ack_count: 0,
            send_count: 0,
            cur_seq: 0,
            game_room_id: 0,
            in_room: false,
            room_order: 0,
            ip_addr,
        }
    }
}
#[derive(Debug)]
pub struct Room {
    pub game_name: String,
    pub game_id: String,
    pub emul_name: String,
    pub creator_id: String,
    pub players: Vec<String>,
    pub game_status: u8,
}

impl Room {
    pub fn new() -> Room {
        Room {
            game_name: "".to_string(),
            game_id: "".to_string(),
            emul_name: "".to_string(),
            creator_id: "".to_string(),
            players: Vec::new(),
            game_status: 0,
        }
    }
    pub fn player_count(self: &Self) -> usize {
        self.players
            .iter()
            .filter(|&n| *n == "".to_string())
            .count()
    }
}

use thiserror::Error;

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
    pub users: HashMap<SocketAddr, User>,
    pub rooms: HashMap<u32, Room>,
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
    pub fn add_room(self: &mut Self, ch: u32, r: Room) -> Result<(), KailleraError> {
        match self.rooms.get(&ch) {
            Some(s) => {
                return Err(KailleraError::AlreadyError {
                    message: "room is already exist".to_string(),
                    pos: 0,
                });
            }
            None => {
                self.rooms.insert(ch, r);
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
    pub fn add_user(self: &mut Self, ip_addr: SocketAddr, user: User) -> Result<(), KailleraError> {
        if self.users.get(&ip_addr).is_some() {
            return Err(KailleraError::AlreadyError {
                message: "user is exist".to_string(),
                pos: 0,
            });
        } else {
            self.users.insert(ip_addr, user);
        }
        Ok(())
    }
}
