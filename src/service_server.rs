use anyhow::Context;
use num;
use num_derive;
use serde::{Deserialize, Serialize};
use serde_repr::*;

use crate::room::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::rc::Rc;
use std::{cmp::*, io};
use tokio::net::UdpSocket;

type MessageT = u8;
pub const USER_QUIT: MessageT = 1;
pub const USER_JOIN: MessageT = 2;
pub const USER_LOGIN_INFO: MessageT = 3;
pub const USER_SERVER_STATUS: MessageT = 4;
pub const S2C_ACK: MessageT = 5;
pub const C2S_ACK: MessageT = 6;
pub const GLOBAL_CHAT: MessageT = 7;
pub const GAME_CHAT: MessageT = 8;
pub const KEEPALIVE: MessageT = 9;
pub const CREATE_GAME: MessageT = 0xa;
pub const QUIT_GAME: MessageT = 0xb;
pub const JOIN_GAME: MessageT = 0xc;
pub const PLAYER_INFO: MessageT = 0xd;
pub const UPDATE_GAME_STATUS: MessageT = 0x0e;
pub const KICK_USER_FROM_GAME: MessageT = 0xf;
pub const CLOSE_GAME: MessageT = 0x10;
pub const START_GAME: MessageT = 0x11;
pub const GAME_DATA: MessageT = 0x12;
pub const GAME_CACHE: MessageT = 0x13;
pub const DROP_GAME: MessageT = 0x14;
pub const READY_TO_PLAY_SIGNAL: MessageT = 0x15;
pub const CONNECTION_REJECT: MessageT = 0x16;
pub const SERVER_INFO: MessageT = 0x17;

type GameStatus = u8;
pub const GameStatusWaiting: GameStatus = 0;
pub const GameStatusPlaying: GameStatus = 1;
pub const GameStatusNetSync: GameStatus = 2;
#[derive(
    Serialize,
    Debug,
    Deserialize_repr,
    Eq,
    PartialEq,
    num_derive::FromPrimitive,
    num_derive::ToPrimitive,
    Copy,
    Clone,
)]
#[repr(u8)]

pub enum MessageType {
    UserNone = 0,
    UserQuit = 1,
    UserJoin = 2,
    UserLoginInfo = 3,
    UserServerStatus = 4,
    S2CAck = 5,
    C2SAck = 6,
    GlobalChat = 7,
    GameChat = 8,
    Keepalive = 9,
    CreateGame = 0xa,
    QuitGame = 0xb,
    JoinGame = 0xc,
    PlayerInfo = 0xd,
    UpdateGameStatus = 0x0e,
    KickUserFromGame = 0xf,
    CloseGame = 0x10,
    StartGame = 0x11,
    GameData = 0x12,
    GameCache = 0x13,
    DropGame = 0x14,
    ReadyToPlaySignal = 0x15,
    ConnectionReject = 0x16,
    ServerInfo = 0x17,
    // GameStatusWaiting = 0,
    // GameStatusPlaying = 1,
    // GameStatusNetSync = 2,

    // PlayerStatusPlaying = 0,
    // PlayerStatusIdle = 1,
    // ProtocolPacketsSize = 1,
    // ProtocolBodySize = 5,
}
#[repr(C, packed)]
#[derive(Serialize, Deserialize)]
pub struct ProtocolHeader {
    pub seq: u16,
    pub length: u16,
    pub message_type: MessageT,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AckProtocol {
    dummy0: u8,
    dummy1: u32,
    dummy2: u32,
    dummy3: u32,
    dummy4: u32,
}

impl AckProtocol {
    pub fn new() -> AckProtocol {
        AckProtocol {
            dummy0: 0,
            dummy1: 0,
            dummy2: 1,
            dummy3: 2,
            dummy4: 3,
        }
    }
}

pub struct Protocol {
    pub header: ProtocolHeader,
    pub data: Vec<u8>,
}

impl Protocol {
    pub fn new(message_type: MessageT, data: Vec<u8>) -> Protocol {
        Protocol {
            header: ProtocolHeader {
                seq: 0,
                length: 0,
                message_type,
            },
            data,
        }
    }
    pub fn make_packet(self: &Self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut v = Vec::new();
        let prob = ProtocolHeader {
            seq: self.header.seq,
            length: self.data.len() as u16 + 1,
            message_type: self.header.message_type,
        };
        let mut s = bincode::serialize::<ProtocolHeader>(&prob)?;
        v.append(&mut s);
        v.append(&mut self.data.clone());
        Ok(v)
    }
}
pub struct ServiceServer {
    pub socket: UdpSocket,
    pub buf: Vec<u8>,
    pub to_send: Option<(usize, SocketAddr)>,
    pub user_room: UserRoom,
    pub game_id: u32,
}

impl ServiceServer {
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        println!("Service Run");
        // let &mut ServiceServer {
        //     socket,
        //     mut buf,
        //     mut to_send,
        //     user_room,
        // } = self;
        loop {
            // First we check to see if there's a message we need to echo back.
            // If so then we try to send it back to the original source, waiting
            // until it's writable and we're able to do so.
            if let Some((size, peer)) = self.to_send {
                // println!("service size: {}, ", size);
                let r = get_protocol_from_bytes(&self.buf[..size].to_vec())?;
                let user = self
                    .user_room
                    .users
                    .entry(peer)
                    .or_insert(Rc::new(RefCell::new(User::new(peer))));
                let messages: Vec<_> = r
                    .iter()
                    .filter(|&n| n.header.seq == user.borrow().cur_seq)
                    .collect();
                println!(
                    "message len: {}, want seq: {}, r: {}",
                    messages.len(),
                    user.borrow().cur_seq,
                    r.len(),
                );
                let message = messages[0];
                // let u = user.get_mut();
                // let u = u.get_mut();
                let user = user.clone();
                user.borrow_mut().cur_seq += 1;
                // *user.borrow_mut().get_mut().cur_seq += 1;
                // let tttt = *user.borrow_mut().cur_seq; // += 1;
                // *(user.borrow_mut().cur_seq) += 1;
                println!(
                    "recv message_type: {:?}, content: {:?}",
                    message.header.message_type, message.data,
                );
                if message.header.message_type == USER_QUIT {
                } else if message.header.message_type == USER_LOGIN_INFO {
                    self.user_room.next_user_id += 1;
                    user.borrow_mut().user_id = self.user_room.next_user_id;
                    user.borrow_mut().player_status = Idle;
                    self.svc_user_login(message.data.clone(), peer).await?;
                } else if message.header.message_type == USER_LOGIN_INFO {
                } else if message.header.message_type == USER_SERVER_STATUS {
                } else if message.header.message_type == S2C_ACK {
                } else if message.header.message_type == C2S_ACK {
                    self.svc_ack(message.data.clone(), peer).await?;
                } else if message.header.message_type == GLOBAL_CHAT {
                    self.svc_global_chat(message.data.clone(), peer).await?;
                } else if message.header.message_type == GAME_CHAT {
                    // self.svc_ack(message.data.clone(), peer).await?;
                } else if message.header.message_type == CREATE_GAME {
                    self.svc_create_game(message.data.clone(), peer).await?;
                }

                // self.socket.send_to("SERVICE\x00".as_bytes(), &peer).await?;
            }
            self.to_send = Some(self.socket.recv_from(&mut self.buf).await?);
        }
    }
    pub async fn svc_user_login(
        &mut self,
        buf: Vec<u8>,
        ip_addr: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        let user = self.user_room.get_user(ip_addr)?;
        let iter = buf.split(|num| num == &0).collect::<Vec<_>>();

        let user_name = String::from_utf8(iter.get(0).ok_or(KailleraError::NotFound)?.to_vec())?;
        let emul_name = String::from_utf8(iter.get(1).ok_or(KailleraError::NotFound)?.to_vec())?;
        let conn_type = iter.get(2).ok_or(KailleraError::NotFound)?[0];
        user.borrow_mut().name = user_name.clone();
        user.borrow_mut().emul_name = emul_name.clone();
        user.borrow_mut().connect_type = conn_type.clone();
        println!("login info: {} {} {}", user_name, emul_name, conn_type);

        let send_data = bincode::serialize::<AckProtocol>(&AckProtocol::new())?;
        let protocol = Protocol::new(S2C_ACK, send_data);
        user.borrow_mut()
            .make_send_packet(&mut self.socket, protocol)
            .await?;
        // self.socket.send_to(&send_data, ip_addr).await?;
        Ok(())
    }
    pub async fn svc_ack(
        self: &mut Self,
        buf: Vec<u8>,
        ip_addr: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        println!("on svc_ack");
        // let user = self
        //     .user_room
        //     .users
        //     .get_mut(&ip_addr)
        //     .ok_or(KailleraError::NotFound)?;
        let user_room = &mut self.user_room;
        let user = user_room.get_user(ip_addr)?;
        // let user_name = user.name.clone();
        // let user_id = user.user_id;
        // let user_ping = user.ping;
        // let user_conn_type = user.connect_type;
        // let user_send_count = user.send_count;

        if user.borrow().send_count <= 4 {
            let send_data = bincode::serialize::<AckProtocol>(&AckProtocol::new())?;
            let protocol = Protocol::new(S2C_ACK, send_data);
            user.borrow_mut()
                .make_send_packet(&mut self.socket, protocol)
                .await?;
        } else {
            user.borrow_mut().ping = 3;
            {
                let p = user_room.make_server_status(user.borrow().send_count, ip_addr)?;
                user.borrow_mut()
                    .make_send_packet(&mut self.socket, p)
                    .await?;
            }
            for i in &self.user_room.users {
                let mut data = Vec::new();
                let mut name = i.1.borrow().name.clone().as_bytes().to_vec();
                data.append(&mut name);
                data.push(0u8);
                data.append(&mut bincode::serialize::<u16>(&i.1.borrow().user_id)?);
                data.append(&mut bincode::serialize::<u32>(&i.1.borrow().ping)?);
                data.push(i.1.borrow().connect_type);
                i.1.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(USER_JOIN, data))
                    .await?;
            }
            {
                let mut data = Vec::new();
                data.append(&mut b"Server\x00".to_vec());
                data.append(&mut b"Dire's kaillera server^^\x00".to_vec());
                user.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(SERVER_INFO, data))
                    .await?;
            }
            // {
            //     data := make([]byte, 0)
            //     data = append(data, []byte("Server"+"\x00")...)
            //     data = append(data, []byte("Dire's kaillera server^^"+"\x00")...)
            //     user.SendPacket(server, *NewProtocol(MessageTypeServerInfo, data))
            // }

            // {
            //     for _, u := range s.userChannel.Users {
            //         data := make([]byte, 0)
            //         data = append(data, []byte(user.Name+"\x00")...)
            //         data = append(data, Uint16ToBytes(user.UserId)...)
            //         data = append(data, Uint32ToBytes(user.Ping)...)
            //         data = append(data, user.ConnectType)
            //         u.SendPacket(server, *NewProtocol(MessageTypeUserJoin, data))
            //     }
            // }
        }

        Ok(())
    }
    pub async fn svc_global_chat(
        self: &mut Self,
        buf: Vec<u8>,
        ip_addr: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        let user_room = &mut self.user_room;
        let user = user_room.get_user(ip_addr)?;
        for i in &self.user_room.users {
            let mut data = Vec::new();
            data.append(&mut user.borrow().name.clone().as_bytes().to_vec());
            data.push(0u8);
            data.append(&mut buf[1..].to_vec());
            i.1.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(GLOBAL_CHAT, data))
                .await?;
        }

        Ok(())
    }
    pub async fn svc_create_game(
        self: &mut Self,
        buf: Vec<u8>,
        ip_addr: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        let user_room = &mut self.user_room;
        let user = user_room.get_user(ip_addr)?;
        let iter = buf.split(|num| num == &0).collect::<Vec<_>>();
        // let game_name = String::from_utf8(iter.get(1).ok_or(KailleraError::NotFound)?.to_vec())?;
        let mut game_name = iter.get(1).ok_or(KailleraError::NotFound)?.to_vec();
        game_name.push(0u8);

        for (_, user) in &self.user_room.users {
            let mut data = Vec::new();
            data.append(&mut user.borrow().name.clone().as_bytes().to_vec());
            data.append(&mut game_name);
            data.append(&mut user.borrow().emul_name.clone().as_bytes().to_vec());
            data.append(&mut bincode::serialize::<u32>(&self.game_id)?);
            user.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(CREATE_GAME, data))
                .await?;
        }
        let mut new_room = Room::new();
        new_room.creator_id = user.borrow().name.clone();
        new_room.emul_name = user.borrow().emul_name.clone();
        new_room.game_id = self.game_id;
        user.borrow_mut().game_room_id = new_room.game_id;
        user.borrow_mut().in_room = true;
        self.game_id += 1;
        new_room.game_name =
            String::from_utf8_lossy(iter.get(1).ok_or(KailleraError::NotFound)?).to_string();
        new_room.game_status = GameStatusWaiting;
        new_room.players.push(Some(user.borrow().ip_addr));
        // update game status
        {
            for (_, user) in &self.user_room.users {
                let mut data = Vec::new();
                data.push(0u8);
                data.append(&mut bincode::serialize::<u32>(&new_room.game_id)?);
                data.push(new_room.game_status);
                data.push(new_room.player_count() as u8);
                data.push(4u8);
                user.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(UPDATE_GAME_STATUS, data))
                    .await?;
            }
        }
        // join game
        {
            for (_, user) in &self.user_room.users {
                let mut data = Vec::new();
                data.push(0u8);
                data.append(&mut bincode::serialize::<u32>(&new_room.game_id)?);
                data.append(&mut user.borrow().name.clone().as_bytes().to_vec());
                data.push(0u8);
                data.append(&mut bincode::serialize::<u32>(&user.borrow().ping)?);
                data.append(&mut bincode::serialize::<u16>(&user.borrow().user_id)?);
                data.push(user.borrow().connect_type);
                user.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(JOIN_GAME, data))
                    .await?;
            }
        }
        // server info
        {
            let mut data = Vec::new();
            data.append(&mut b"Server\x00".to_vec());
            let game_name_str =
                String::from_utf8_lossy(&iter.get(1).ok_or(KailleraError::NotFound)?.to_vec())
                    .to_string();
            let s = format!(
                "{} Creates Room: {}\x00",
                user.borrow().name.clone(),
                game_name_str.to_string()
            );
            data.append(&mut s.as_bytes().to_vec());
            user.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(SERVER_INFO, data))
                .await?;
        }
        self.user_room.add_room(new_room.game_id, new_room)?;

        Ok(())
    }
    // TODO: JoinGame, QuitGame, GameData, GameCahce, GameStart
}

pub fn get_protocol_from_bytes(data: &Vec<u8>) -> Result<Vec<Protocol>, Box<dyn Error>> {
    println!("get_protocol data: {:?}", data);
    let mut v = Vec::new();

    let mut cur_pos = 1;
    let mut loopCount = 0;
    while cur_pos + 5 <= data.len() {
        // println!("{} <= {}", cur_pos + 5, data.len());
        loopCount += 1;
        // println!("protocol body: {:?}", &data[cur_pos..cur_pos + 5]);
        let protocol = bincode::deserialize::<ProtocolHeader>(&data[cur_pos..cur_pos + 5])?;
        let d = &data[cur_pos + 5..cur_pos + 5 + protocol.length as usize - 1];
        cur_pos += (5 + protocol.length - 1) as usize;
        v.push(Protocol {
            header: protocol,
            data: d.to_vec(),
        });
    }
    println!("after parse: {}", v.len());
    return Ok(v);
}
