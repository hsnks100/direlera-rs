use serde::{Deserialize, Serialize};
use std::error::Error;
type MessageT = u8;
use log::{info, trace, warn};
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
// GameStatusWaiting = 0,
// GameStatusPlaying = 1,
// GameStatusNetSync = 2,

// PlayerStatusPlaying = 0,
// PlayerStatusIdle = 1,
// ProtocolPacketsSize = 1,
// ProtocolBodySize = 5,

type GameStatus = u8;
pub const GameStatusWaiting: GameStatus = 0;
pub const GameStatusPlaying: GameStatus = 1;
pub const GameStatusNetSync: GameStatus = 2;
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
    pub fn make_packet(self: &Self) -> anyhow::Result<Vec<u8>> {
        // }, Box<dyn Error>> {
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

pub fn get_protocol_from_bytes(data: &Vec<u8>) -> Result<Vec<Protocol>, Box<dyn Error>> {
    info!("get_protocol data: {:?}", data);
    let mut v = Vec::new();

    let mut cur_pos = 1;
    while cur_pos + 5 <= data.len() {
        // info!("{} <= {}", cur_pos + 5, data.len());
        // info!("protocol body: {:?}", &data[cur_pos..cur_pos + 5]);
        let protocol = bincode::deserialize::<ProtocolHeader>(&data[cur_pos..cur_pos + 5])?;
        let d = &data[cur_pos + 5..cur_pos + 5 + protocol.length as usize - 1];
        cur_pos += (5 + protocol.length - 1) as usize;
        v.push(Protocol {
            header: protocol,
            data: d.to_vec(),
        });
    }
    info!("after parse: {}", v.len());
    return Ok(v);
}
