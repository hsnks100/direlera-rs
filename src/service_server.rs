use anyhow::Context;
use num;
use num_derive;
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_repr::*;
use std::error::Error;
use std::net::SocketAddr;
use std::{cmp::*, io};
use tokio::net::UdpSocket;

use crate::room::{KailleraError, PlayerStatus, User, UserRoom};

#[derive(
    Debug, Deserialize_repr, Eq, PartialEq, num_derive::FromPrimitive, num_derive::ToPrimitive,
)]
#[repr(u8)]
pub enum MessageType {
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
// #[repr(C, packed)]
#[derive(Deserialize, Debug)]
pub struct ProtocolHeader {
    pub seq: u16,
    pub length: u16,
    pub message_type: MessageType,
}

#[derive(Serialize, Deserialize, Debug)]
struct AckProtocol {
    dummy0: u8,
    dummy1: u32,
    dummy2: u32,
    dummy3: u32,
    dummy4: u32,
}

impl AckProtocol {
    fn new() -> AckProtocol {
        AckProtocol {
            dummy0: 0,
            dummy1: 0,
            dummy2: 1,
            dummy3: 2,
            dummy4: 3,
        }
    }
}

#[derive(Debug)]
pub struct Protocol {
    pub header: ProtocolHeader,
    pub data: Vec<u8>,
}

impl Protocol {
    pub fn new(message_type: MessageType, data: Vec<u8>) -> Protocol {
        Protocol {
            header: ProtocolHeader {
                seq: 0,
                length: 0,
                message_type,
            },
            data,
        }
    }
}

pub struct ServiceServer {
    pub socket: UdpSocket,
    pub buf: Vec<u8>,
    pub to_send: Option<(usize, SocketAddr)>,
    pub user_room: UserRoom,
}

impl ServiceServer {
    pub async fn run(self: &mut Self) -> Result<(), Box<dyn Error>> {
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
                println!("service size: {}, ", size);
                let r = get_protocol_from_bytes(&self.buf[..size].to_vec())?;
                let user = match self.user_room.users.get_mut(&peer) {
                    Some(s) => Some(s),
                    None => {
                        let u = User::new(self.socket, peer);
                        self.user_room.add_user(peer, u)?;
                        self.user_room.users.get_mut(&peer)
                    }
                };
                let user = user.ok_or(KailleraError::NotFound)?;

                let messages: Vec<_> = r.iter().filter(|&n| n.header.seq == user.cur_seq).collect();
                let message = messages[0];
                println!("recv message: {:?}", message);
                if message.header.message_type == MessageType::UserQuit {
                } else if message.header.message_type == MessageType::UserLoginInfo {
                    self.user_room.next_user_id += 1;
                    user.user_id = self.user_room.next_user_id;
                    user.player_status = PlayerStatus::Idle;
                    self.SvcUserLogin(message.data.clone(), peer).await;
                } else if message.header.message_type == MessageType::UserLoginInfo {
                } else if message.header.message_type == MessageType::UserServerStatus {
                } else if message.header.message_type == MessageType::S2CAck {
                } else if message.header.message_type == MessageType::C2SAck {
                }

                self.socket.send_to("SERVICE\x00".as_bytes(), &peer).await?;
            }
            self.to_send = Some(self.socket.recv_from(&mut self.buf).await?);
        }
    }
    pub async fn SvcUserLogin(
        self: &mut Self,
        buf: Vec<u8>,
        ip_addr: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        let iter = buf.split(|num| num == &0).collect::<Vec<_>>();

        let user_name = String::from_utf8(iter.get(0).ok_or(KailleraError::NotFound)?.to_vec())?;
        let emul_name = String::from_utf8(iter.get(1).ok_or(KailleraError::NotFound)?.to_vec())?;
        let conn_type = iter.get(2).ok_or(KailleraError::NotFound)?[0];
        let user = self
            .user_room
            .users
            .get_mut(&ip_addr)
            .context(KailleraError::NotFound)?;
        user.name = user_name;
        user.emul_name = emul_name;
        user.connect_type = conn_type;
        println!(
            "login info: {} {} {}",
            user.name, user.emul_name, user.connect_type
        );

        let send_data = bincode::serialize::<AckProtocol>(&AckProtocol::new())?;
        let protocol = Protocol::new(MessageType::S2CAck, send_data);
        // self.socket.send_to(&send_data, ip_addr).await?;
        Ok(())
    }
}

pub fn get_protocol_from_bytes(data: &Vec<u8>) -> Result<Vec<Protocol>, Box<dyn Error>> {
    let mut v = Vec::new();

    let mut cur_pos = 1;
    let mut loopCount = 0;
    while cur_pos + 5 <= data.len() {
        println!("{} <= {}", cur_pos + 5, data.len());
        loopCount += 1;
        println!("protocol body: {:?}", &data[cur_pos..cur_pos + 5]);
        let protocol = bincode::deserialize::<ProtocolHeader>(&data[cur_pos..cur_pos + 5])?;
        let d = &data[cur_pos + 5..cur_pos + 5 + protocol.length as usize - 1];
        cur_pos += (5 + protocol.length - 1) as usize;
        v.push(Protocol {
            header: protocol,
            data: d.to_vec(),
        });
        // v.push(Protocol {
        //     protocol: ProtocolHeader {
        //         seq: protocol.seq,
        //         length: protocol.length,
        //         message_type: num::FromPrimitive::from_u8(protocol.message_type)
        //             .ok_or(KailleraError::TokenError)?,
        //     },
        //     data: d.to_vec(),
        // });
    }
    return Ok(v);
}
