use std::collections::HashMap;

use serde::{Deserialize, Serialize};

type MessageT = u8;
use log::info;
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

pub type GameStatus = u8;
pub const GAME_STATUS_WAITING: GameStatus = 0;
pub const GAME_STATUS_PLAYING: GameStatus = 1;
pub const GAME_STATUS_NET_SYNC: GameStatus = 2;
// #[repr(C, packed)]

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProtocolPureHeader {
    pub length: u16,
    pub message_type: MessageT,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProtocolSeqHeader {
    pub seq: u16,
    pub header: ProtocolPureHeader,
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

#[derive(Debug, Clone)]
pub struct Protocol {
    pub header: ProtocolSeqHeader,
    pub data: Vec<u8>,
}

// impl Copy for Protocol {
//     fn copy(&self) -> Self {
//         Self {
//             header: self.header,
//             data: self.data.clone(),
//         }
//     }
// }

impl Protocol {
    pub fn new(message_type: MessageT, data: Vec<u8>) -> Protocol {
        Protocol {
            header: ProtocolSeqHeader {
                seq: 0,
                header: ProtocolPureHeader {
                    length: data.len() as u16 + 1,
                    message_type,
                },
            },
            data,
        }
    }
    pub fn make_packet(&self) -> anyhow::Result<Vec<u8>> {
        // }, Box<dyn Error>> {
        let mut v = Vec::new();
        let prob = ProtocolSeqHeader {
            seq: self.header.seq,
            header: ProtocolPureHeader {
                length: self.data.len() as u16 + 1,
                message_type: self.header.header.message_type,
            },
        };
        let mut s = bincode::serialize::<ProtocolSeqHeader>(&prob)?;
        v.append(&mut s);
        v.append(&mut self.data.clone());
        Ok(v)
    }
}

pub fn get_protocol_from_bytes(data: &Vec<u8>) -> anyhow::Result<Vec<Protocol>> {
    let mut v = Vec::new();

    let mut cur_pos = 1;
    while cur_pos + 5 <= data.len() {
        // info!("{} <= {}", cur_pos + 5, data.len());
        // info!("protocol body: {:?}", &data[cur_pos..cur_pos + 5]);
        let protocol = bincode::deserialize::<ProtocolSeqHeader>(&data[cur_pos..cur_pos + 5])?;
        let d = &data[cur_pos + 5..cur_pos + 5 + protocol.header.length as usize - 1];
        cur_pos += (5 + protocol.header.length - 1) as usize;
        v.push(Protocol {
            header: protocol,
            data: d.to_vec(),
        });
    }
    Ok(v)
}

// Sequence to Protocol Store
pub struct ProtocolPackets {
    matched_seq: Option<u16>,
    pub packets: HashMap<u16, Protocol>,
}

impl ProtocolPackets {
    pub fn new() -> ProtocolPackets {
        ProtocolPackets {
            packets: HashMap::new(),
            matched_seq: None,
        }
    }
    pub fn add(&mut self, protocol: Protocol) {
        // don't add old packet
        match self.matched_seq {
            Some(seq) => {
                if protocol.header.seq > seq {
                    self.packets.entry(protocol.header.seq).or_insert(protocol);
                }
            }
            None => {
                self.packets.entry(protocol.header.seq).or_insert(protocol);
            }
        }
    }
    pub fn fetch_protocol(&mut self, seq: u16) -> Option<Protocol> {
        let t = self.packets.remove(&seq);
        if t.is_some() {
            self.matched_seq = Some(seq);
        }
        t
    }
    pub fn len(&self) -> usize {
        self.packets.len()
    }
    // show seq list in in_packets
    pub fn show_seq_list(&self) {
        for (k, _) in &self.packets {
            info!("seq list: {:?}", *k);
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserJoinPacket2Client {
    pub user_name: Vec<u8>,
    pub user_id: u16,
    pub ping: u32,
    pub connection_type: u8,
}

impl UserJoinPacket2Client {
    pub fn new(
        user_name: Vec<u8>,
        user_id: u16,
        ping: u32,
        connection_type: u8,
    ) -> UserJoinPacket2Client {
        UserJoinPacket2Client {
            user_name,
            user_id,
            ping,
            connection_type,
        }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut self.user_name.clone());
        v.push(0u8);
        v.append(&mut bincode::serialize(&self.user_id)?);
        v.append(&mut bincode::serialize(&self.ping)?);
        v.append(&mut bincode::serialize(&self.connection_type)?);
        Ok(v)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserQuitPacket2Client {
    pub user_name: Vec<u8>,
    pub user_id: u16,
    pub message: Vec<u8>,
}

impl UserQuitPacket2Client {
    pub fn new(user_name: Vec<u8>, user_id: u16, message: Vec<u8>) -> UserQuitPacket2Client {
        UserQuitPacket2Client {
            user_name,
            user_id,
            message,
        }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut self.user_name.clone());
        v.push(0u8);
        v.append(&mut bincode::serialize(&self.user_id)?);
        v.append(&mut self.message.clone());
        v.push(0u8);
        Ok(v)
    }
}

pub struct AckPacket2Client {
    pub n: u8,
    pub p0: u32,
    pub p1: u32,
    pub p2: u32,
    pub p3: u32,
}

impl AckPacket2Client {
    pub fn new(n: u8, p0: u32, p1: u32, p2: u32, p3: u32) -> AckPacket2Client {
        AckPacket2Client { n, p0, p1, p2, p3 }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut bincode::serialize(&self.n)?);
        v.append(&mut bincode::serialize(&self.p0)?);
        v.append(&mut bincode::serialize(&self.p1)?);
        v.append(&mut bincode::serialize(&self.p2)?);
        v.append(&mut bincode::serialize(&self.p3)?);
        Ok(v)
    }
}

pub struct GlobalChat2Client {
    pub user_name: Vec<u8>,
    pub message: Vec<u8>,
}

impl GlobalChat2Client {
    pub fn new(user_name: Vec<u8>, message: Vec<u8>) -> GlobalChat2Client {
        GlobalChat2Client { user_name, message }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut self.user_name.clone());
        v.push(0u8);
        v.append(&mut self.message.clone());
        v.push(0u8);
        Ok(v)
    }
}

pub struct GameChat2Client {
    pub user_name: Vec<u8>,
    pub message: Vec<u8>,
}

impl GameChat2Client {
    pub fn new(user_name: Vec<u8>, message: Vec<u8>) -> GameChat2Client {
        GameChat2Client { user_name, message }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut self.user_name.clone());
        v.push(0u8);
        v.append(&mut self.message.clone());
        v.push(0u8);
        Ok(v)
    }
}

pub struct CreateGame2Client {
    pub user_name: Vec<u8>,
    pub game_name: Vec<u8>,
    pub emul_name: Vec<u8>,
    pub game_id: u32,
}

impl CreateGame2Client {
    pub fn new(
        user_name: Vec<u8>,
        game_name: Vec<u8>,
        emul_name: Vec<u8>,
        game_id: u32,
    ) -> CreateGame2Client {
        CreateGame2Client {
            user_name,
            game_name,
            emul_name,
            game_id,
        }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut self.user_name.clone());
        v.push(0u8);
        v.append(&mut self.game_name.clone());
        v.push(0u8);
        v.append(&mut self.emul_name.clone());
        v.push(0u8);
        v.append(&mut bincode::serialize(&self.game_id)?);
        Ok(v)
    }
}

pub struct JoinGame2Client {
    pub n: u8,
    pub game_id: u32,
    pub user_name: Vec<u8>,
    pub ping: u32,
    pub user_id: u16,
    pub connection_type: u8,
}

impl JoinGame2Client {
    pub fn new(
        game_id: u32,
        user_name: Vec<u8>,
        ping: u32,
        user_id: u16,
        connection_type: u8,
    ) -> JoinGame2Client {
        JoinGame2Client {
            n: 0,
            game_id,
            user_name,
            ping,
            user_id,
            connection_type,
        }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut bincode::serialize(&self.n)?);
        v.append(&mut bincode::serialize(&self.game_id)?);
        v.append(&mut self.user_name.clone());
        v.push(0u8);
        v.append(&mut bincode::serialize(&self.ping)?);
        v.append(&mut bincode::serialize(&self.user_id)?);
        v.append(&mut bincode::serialize(&self.connection_type)?);
        Ok(v)
    }
}

pub struct UpdateGameStatus2Client {
    pub n: u8,
    pub game_id: u32,
    pub game_status: u8, // 0: Waiting, 1:Playing, 2:Netsync
    pub num_of_players: u8,
    pub max_players: u8,
}

impl UpdateGameStatus2Client {
    pub fn new(
        n: u8,
        game_id: u32,
        game_status: u8,
        num_of_players: u8,
        max_players: u8,
    ) -> UpdateGameStatus2Client {
        UpdateGameStatus2Client {
            n,
            game_id,
            game_status,
            num_of_players,
            max_players,
        }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut bincode::serialize(&self.n)?);
        v.append(&mut bincode::serialize(&self.game_id)?);
        v.append(&mut bincode::serialize(&self.game_status)?);
        v.append(&mut bincode::serialize(&self.num_of_players)?);
        v.append(&mut bincode::serialize(&self.max_players)?);
        Ok(v)
    }
}

pub struct GameData2Client {
    pub unused: u8,
    pub len: u16,
    pub game_data: Vec<u8>,
}

impl GameData2Client {
    pub fn new(len: u16, game_data: Vec<u8>) -> GameData2Client {
        GameData2Client {
            unused: 0,
            len,
            game_data,
        }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut bincode::serialize(&self.unused)?);
        v.append(&mut bincode::serialize(&self.len)?);
        v.append(&mut self.game_data.clone());
        Ok(v)
    }
}

pub struct GameCache2Client {
    pub unused: u8,
    pub cache_position: u8,
}

impl GameCache2Client {
    pub fn new(cache_position: u8) -> GameCache2Client {
        GameCache2Client {
            unused: 0,
            cache_position,
        }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut bincode::serialize(&self.unused)?);
        v.append(&mut bincode::serialize(&self.cache_position)?);
        Ok(v)
    }
}

pub struct GameDrop2Client {
    pub user_name: Vec<u8>,
    pub player_number: u8,
}

impl GameDrop2Client {
    pub fn new(user_name: Vec<u8>, player_number: u8) -> GameDrop2Client {
        GameDrop2Client {
            user_name,
            player_number,
        }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut self.user_name.clone());
        v.push(0u8);
        v.append(&mut bincode::serialize(&self.player_number)?);
        Ok(v)
    }
}

pub struct ConnectionReject2Client {
    pub user_name: Vec<u8>,
    pub user_id: u16,
}

impl ConnectionReject2Client {
    pub fn new(user_name: Vec<u8>, user_id: u16) -> ConnectionReject2Client {
        ConnectionReject2Client { user_name, user_id }
    }
    pub fn packetize(&self) -> anyhow::Result<Vec<u8>> {
        let mut v = Vec::new();
        v.append(&mut self.user_name.clone());
        v.push(0u8);
        v.append(&mut bincode::serialize(&self.user_id)?);
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::*;

    #[test]
    fn pack_test() {
        let prob = ProtocolSeqHeader {
            seq: 0x1234,
            header: ProtocolPureHeader {
                length: 0x4321,
                message_type: 6,
            },
        };
        let s = bincode::serialize::<ProtocolSeqHeader>(&prob);
        assert!(s.is_ok());
        let s = s.unwrap();
        assert_eq!(s.len(), 5);
        assert_eq!(s[0], 0x34);
        assert_eq!(s[1], 0x12);
        assert_eq!(s[2], 0x21);
        assert_eq!(s[3], 0x43);
        assert_eq!(s[4], 6);

        let de = bincode::deserialize::<ProtocolSeqHeader>(&s);
        assert_eq!(de.is_ok(), true);
        let de = de.unwrap();
        assert_eq!(de.seq, 0x1234);
        assert_eq!(de.header.length, 0x4321);
        assert_eq!(de.header.message_type, 6);
    }
    #[test]
    fn get_protocol() {
        let i = vec![
            3, 2, 0, 18, 0, 6, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 1, 0, 18, 0, 6,
            0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0, 0, 24, 0, 3, 118, 98, 111, 120,
            117, 115, 101, 114, 0, 77, 65, 77, 69, 76, 79, 78, 32, 65, 45, 52, 51, 0, 1,
        ];
        let r = get_protocol_from_bytes(&i);
        assert!(r.is_ok());
        let r = r.unwrap();

        assert_eq!(r.len(), 3);
        assert_eq!(r[0].header.header.message_type, 6);
        assert_eq!(r[1].header.header.message_type, 6);
        assert_eq!(r[2].header.header.message_type, 3);
        // assert_eq!(r[0].header.length, 18);
        // assert_eq!(r[1].header.length, 18);
        // assert_eq!(r[2].header.length, 24);
        // assert_eq!(r[0].header.seq, 0);
        // assert_eq!(r[1].header.seq, 0);
        // assert_eq!(r[2].header.seq, 0);

        // print protocols
        for p in r {
            let pp = p.header.header.message_type;
            let _ppp = p.header.header.length;
            let pppp = p.header.seq;
            println!("protocol: {}, type: {}", pppp, pp,);
        }
    }
    #[test]
    fn fetch_protocol() {
        let mut store = ProtocolPackets::new();
        let mut p0 = Protocol::new(1, vec![1, 2, 3]);
        p0.header.seq = 1;

        let mut p1 = Protocol::new(2, vec![1, 2, 3]);
        p1.header.seq = 2;

        let mut p2 = Protocol::new(3, vec![1, 2, 3]);
        p2.header.seq = 3;

        store.add(p0);
        store.add(p1);
        store.add(p2);

        let r = store.fetch_protocol(1);
        assert!(r.is_some());
        let mut p0 = Protocol::new(1, vec![1, 2, 3]);
        p0.header.seq = 5;
        store.add(p0);
        let mut p0 = Protocol::new(1, vec![1, 2, 3]);
        p0.header.seq = 5;
        store.add(p0);
        let r = store.fetch_protocol(4);
        assert!(r.is_none());
        let mut p0 = Protocol::new(4, vec![1, 2, 3]);
        p0.header.seq = 4;
        store.add(p0);
        let r = store.fetch_protocol(4);
        assert!(r.is_some());
        let r = store.fetch_protocol(5);
        assert!(r.is_some());
    }
}
