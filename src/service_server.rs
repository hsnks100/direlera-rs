use crate::protocol::*;
use crate::room::*;

#[cfg(feature = "alloc")]
use encoding_rs::*;
use log::{info, trace};
use rand::Rng;
use serde::__private::from_utf8_lossy;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;

use tokio::select;

use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;
use std::time::Instant;

use tokio::net::UdpSocket;

pub struct ServiceServer {
    pub config: HashMap<String, String>,
    pub socket: UdpSocket,
    pub buf: Vec<u8>,
    pub to_send: Option<(usize, SocketAddr)>,
    pub session_manager: UserRoom,
    pub game_id: u32,
    pub rx: Receiver<Event>,
    pub tx: Sender<Event>,
}

#[derive(Debug)]
pub enum Event {
    KeepaliveTimer,
}
impl ServiceServer {
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        info!("Service Run");

        loop {
            // let r = self.keepalive_timer;
            // let r2 = self.service;
            select! {
                _ = ServiceServer::keepalive_timer(self.tx.clone()) => {
                }
                _ = self.service() => {
                }
            }
        }
    }

    pub async fn keepalive_timer(tx: Sender<Event>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(10000));
        loop {
            interval.tick().await;
            tx.send(Event::KeepaliveTimer).await?;
        }
    }
    pub async fn keepalive_event(&mut self) -> anyhow::Result<()> {
        // check user timeout
        let now = Instant::now();
        let mut timeout_users = vec![];
        for (k, v) in self.session_manager.users.iter() {
            if now.duration_since(v.borrow().keepalive_time) > Duration::from_secs(240) {
                info!("timeout!!!: {:#?}", k);
                timeout_users.push(*k);
            }
        }
        for i in timeout_users.iter() {
            let user = self.session_manager.get_user(*i)?;
            let _ = self.fun_quit_game(user.clone()).await;
            // send quit message to all
            let data = UserQuitPacket2Client::new(
                user.borrow().name.clone(),
                user.borrow().user_id,
                b"time out".to_vec(),
            )
            .packetize()?;
            for (_addr, u) in &self.session_manager.users {
                u.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(USER_QUIT, data.clone()))
                    .await?;
            }
            self.session_manager.users.remove(i);
        }
        Ok(())
    }
    pub async fn service(&mut self) -> anyhow::Result<()> {
        loop {
            select! {
                _ = self.rx.recv() => {
                    self.keepalive_event().await?;
                }
                ts = self.socket.recv_from(&mut self.buf) => {
                    self.to_send = Some(ts?);
                    if let Some((size, peer)) = self.to_send {
                        let result = self.service_proc(size, peer).await;
                        if result.is_err() {
                            info!("err content: {:#?}", result.err());
                        }
                    }
                }
            }
        }
    }

    pub async fn service_proc(&mut self, size: usize, peer: SocketAddr) -> anyhow::Result<()> {
        // info!("service size: {}, ", size);
        let r = get_protocol_from_bytes(&self.buf[..size].to_vec())?;
        if r.len() == 0 {
            info!("protocol length: 0");
        }
        let user = {
            match self.session_manager.users.get(&peer) {
                Some(i) => i.clone(),
                None => {
                    if r.len() == 1 && r[0].header.seq == 0 {
                        info!("new user: insert");
                        Rc::new(RefCell::new(User::new(peer)))
                    } else {
                        return Err(KailleraError::NotFoundUser {
                            message: format!("{:?}", r[0]),
                        }
                        .into());
                    }
                }
            }
        };
        for i in r.iter() {
            user.borrow_mut().in_packets.add(i.clone());
        }
        let want_seq = user.borrow().cur_seq;
        let message = user.borrow_mut().in_packets.fetch_protocol(want_seq);
        let message = match message {
            Some(i) => i,
            None => {
                info!(
                    "user name: {}",
                    String::from_utf8_lossy(&user.borrow().name.clone())
                );
                for i in &user.borrow().in_packets.packets {
                    info!("seq: {}", i.1.header.seq);
                }
                return Err(KailleraError::NotFoundSeq {
                    wanted_seq: want_seq,
                    cur_seq: 9999,
                }
                .into());
            }
        };
        user.borrow_mut().keepalive_time = Instant::now();
        user.borrow().in_packets.show_seq_list();
        // let messages: Vec<_> = r
        //     .iter()
        //     .filter(|&n| n.header.seq == user.borrow().cur_seq)
        //     .collect();

        // let message = messages.get(0).ok_or(KailleraError::NotFound)?;
        let user = user.clone();
        user.borrow_mut().cur_seq += 1;
        if message.header.header.message_type == USER_QUIT {
            self.svc_user_quit(message.data.clone(), user).await?;
        } else if message.header.header.message_type == USER_LOGIN_INFO {
            self.session_manager.users.insert(peer, user.clone());
            self.session_manager.next_user_id += 1;
            user.borrow_mut().user_id = self.session_manager.next_user_id;
            user.borrow_mut().player_status = Idle;
            self.svc_user_login(message.data.clone(), peer).await?;
        } else if message.header.header.message_type == USER_LOGIN_INFO {
        } else if message.header.header.message_type == USER_SERVER_STATUS {
        } else if message.header.header.message_type == S2C_ACK {
        } else if message.header.header.message_type == C2S_ACK {
            self.svc_ack(message.data.clone(), user).await?;
        } else if message.header.header.message_type == GLOBAL_CHAT {
            self.svc_global_chat(message.data.clone(), peer).await?;
        } else if message.header.header.message_type == GAME_CHAT {
            self.svc_game_chat(message.data.clone(), peer).await?;
        } else if message.header.header.message_type == KEEPALIVE {
            info!(
                "keepalive user name: {}",
                String::from_utf8_lossy(user.borrow().name.clone().as_slice())
            );
            user.borrow_mut().keepalive_time = Instant::now();
        } else if message.header.header.message_type == CREATE_GAME {
            self.svc_create_game(message.data.clone(), user).await?;
        } else if message.header.header.message_type == QUIT_GAME {
            info!("ON QUIT_GAME");
            self.svc_quit_game(message.data.clone(), user).await?;
        } else if message.header.header.message_type == JOIN_GAME {
            self.svc_join_game(message.data.clone(), user).await?;
        } else if message.header.header.message_type == KICK_USER_FROM_GAME {
            self.svc_kick_user(message.data.clone(), user).await?;
        } else if message.header.header.message_type == START_GAME {
            self.svc_start_game(message.data.clone(), user).await?;
        } else if message.header.header.message_type == GAME_DATA {
            self.svc_game_data(message.data.clone(), user).await?;
        } else if message.header.header.message_type == GAME_CACHE {
            self.svc_game_cache(message.data.clone(), user).await?;
        } else if message.header.header.message_type == DROP_GAME {
            info!("ON DROP_GAME");
            self.svc_drop_game(message.data.clone(), user).await?;
        } else if message.header.header.message_type == READY_TO_PLAY_SIGNAL {
            self.svc_ready_to_playsignal(message.data.clone(), user)
                .await?;
        }

        Ok(())
    }
    pub async fn svc_user_quit(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        info!("== svc_user_quit ==");
        let _ = self.fun_quit_game(user.clone()).await;

        let client_message = &buf[3..];
        // send quit message to all
        let data = UserQuitPacket2Client::new(
            user.borrow().name.clone(),
            user.borrow().user_id,
            client_message.to_vec(),
        )
        .packetize()?;
        for (_addr, u) in &self.session_manager.users {
            u.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(USER_QUIT, data.clone()))
                .await?;
        }
        self.session_manager.users.remove(&user.borrow().ip_addr);
        Ok(())
    }
    pub async fn svc_user_login(
        &mut self,
        buf: Vec<u8>,
        ip_addr: SocketAddr,
    ) -> anyhow::Result<()> {
        let user = self.session_manager.get_user(ip_addr)?;
        let iter = buf.split(|num| num == &0).collect::<Vec<_>>();

        info!("iter len: {}", iter.len());
        let un = iter.get(0).ok_or(KailleraError::NotFound)?.to_vec();
        let emul_name =
            String::from_utf8_lossy(iter.get(1).ok_or(KailleraError::NotFound)?).to_string();
        let conn_type = iter.get(2).ok_or(KailleraError::NotFound)?[0];
        user.borrow_mut().name = un.clone();
        user.borrow_mut().emul_name = emul_name.clone();
        user.borrow_mut().connect_type = conn_type;
        info!("login info: {:?} {} {}", un.clone(), emul_name, conn_type);

        let send_data = bincode::serialize::<AckProtocol>(&AckProtocol::new())?;
        let protocol = Protocol::new(S2C_ACK, send_data);
        user.borrow_mut().s2c_ack_time = Instant::now();

        user.borrow_mut()
            .make_send_packet(&mut self.socket, protocol)
            .await?;
        // self.socket.send_to(&send_data, ip_addr).await?;
        Ok(())
    }
    pub async fn svc_ack(&mut self, _buf: Vec<u8>, user: Rc<RefCell<User>>) -> anyhow::Result<()> {
        info!("on svc_ack");
        let elapsed = user.borrow().s2c_ack_time.elapsed().as_millis();
        let user_room = &mut self.session_manager;
        user.borrow_mut().pings.push(elapsed as i32);
        if user.borrow().send_count <= 4 {
            let send_data = bincode::serialize::<AckProtocol>(&AckProtocol::new())?;
            user.borrow_mut().s2c_ack_time = Instant::now();
            let protocol = Protocol::new(S2C_ACK, send_data);
            user.borrow_mut()
                .make_send_packet(&mut self.socket, protocol)
                .await?;
        } else {
            let sum: i32 = user.borrow().pings.iter().sum();
            let len = user.borrow().pings.len() as f64;

            let average = sum as f64 / len;
            let is_random = match self.config.get("random_ping") {
                Some(x) => x.parse::<bool>().unwrap(),
                None => false,
            };
            if is_random {
                // set random ping [0, 100]
                user.borrow_mut().ping = rand::thread_rng().gen_range(0..100);
            } else {
                user.borrow_mut().ping = average as u32;
            }
            {
                let p = user_room.make_server_status(user.borrow().ip_addr)?;
                user.borrow_mut()
                    .make_send_packet(&mut self.socket, p)
                    .await?;
            }
            for i in &self.session_manager.users {
                let data = UserJoinPacket2Client::new(
                    user.borrow().name.clone(),
                    user.borrow().user_id,
                    user.borrow().ping,
                    user.borrow().connect_type,
                )
                .packetize()?;
                i.1.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(USER_JOIN, data))
                    .await?;
            }
            {
                let mut data = Vec::new();
                data.append(&mut b"Server\x00".to_vec());
                let mut euc_kr = encoding_rs::EUC_KR
                    .encode(&self.config.get("notice").unwrap_or(&"".to_string()).clone())
                    .0
                    .to_vec();
                data.append(&mut euc_kr);
                const VERSION: &str = env!("CARGO_PKG_VERSION");
                data.append(&mut b"\ndirelera version: ".to_vec());
                data.append(&mut VERSION.as_bytes().to_vec());
                data.push(0);
                user.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(SERVER_INFO, data))
                    .await?;
            }
        }

        Ok(())
    }
    pub async fn svc_global_chat(
        &mut self,
        buf: Vec<u8>,
        ip_addr: SocketAddr,
    ) -> anyhow::Result<()> {
        let user_room = &mut self.session_manager;
        let user = user_room.get_user(ip_addr)?;
        let message = buf[1..].to_vec();
        let data =
            GlobalChat2Client::new(user.borrow().name.clone(), message.clone()).packetize()?;
        for i in &self.session_manager.users {
            i.1.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(GLOBAL_CHAT, data.clone()))
                .await?;
        }
        // cp949 to utf-8 for message

        println!(
            "chat message: {:?}",
            encoding_rs::EUC_KR.decode(&message).0.to_string()
        );

        if String::from_utf8_lossy(&message.clone()) == "ts\x00" {
            println!("admin mode");
            let d = self.session_manager.to_string();
            for i in &self.session_manager.users {
                let d = d.clone();
                let split_data: Vec<_> = d.split('\n').collect();
                for each_data in split_data {
                    if !each_data.is_empty() {
                        let data = GlobalChat2Client::new(
                            user.borrow().name.clone(),
                            each_data.to_string().into_bytes(),
                        )
                        .packetize()?;
                        i.1.borrow_mut()
                            .make_send_packet(&mut self.socket, Protocol::new(GLOBAL_CHAT, data))
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }
    pub async fn svc_game_chat(&mut self, buf: Vec<u8>, ip_addr: SocketAddr) -> anyhow::Result<()> {
        // let user_room = &self.user_room;
        let user = self.session_manager.get_user(ip_addr)?;
        let room_id = match user.borrow().game_room_id {
            Some(i) => i,
            None => {
                return Err(KailleraError::NotFound.into());
            }
        };
        let room = self.session_manager.get_room(room_id)?;
        let mut ips = Vec::new();
        for i in &room.borrow().players {
            ips.push(*i);
        }

        let data = GameChat2Client::new(user.borrow().name.clone(), buf.clone()[1..].to_vec())
            .packetize()?;
        let chat_content = buf.clone()[1..].to_vec();
        if chat_content == b"/samedelay true\x00" {
            info!("delay true");
            room.borrow_mut().same_delay = true;
        } else if chat_content == b"/samedelay false\x00" {
            info!("delay false");
            room.borrow_mut().same_delay = false;
        }
        info!("game chat: {:?}", chat_content);
        info!("cmp chat: {:?}", b"/samedelay true");
        for i in ips {
            match i {
                PlayerAddr::None => {}
                PlayerAddr::Playing(s) | PlayerAddr::Idle(s) => {
                    let u = self.session_manager.get_user(s)?;
                    u.borrow_mut()
                        .make_send_packet(&mut self.socket, Protocol::new(GAME_CHAT, data.clone()))
                        .await?;
                }
            }
        }
        Ok(())
    }
    pub async fn svc_create_game(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        let has_room = user.borrow().game_room_id.is_some();
        if has_room {
            user.borrow_mut()
                .send_message(
                    &mut self.socket,
                    b"You are already joining the room.".to_vec(),
                )
                .await?;
            return Err(KailleraError::AlreadyError {
                message: "already has room.".to_string(),
            }
            .into());
        }
        let iter = buf.split(|num| num == &0).collect::<Vec<_>>();
        // let game_name = String::from_utf8(iter.get(1).ok_or(KailleraError::NotFound)?.to_vec())?;
        // create game packet
        {
            let game_name = iter.get(1).ok_or(KailleraError::NotFound)?.to_vec();
            let data = CreateGame2Client::new(
                user.borrow().name.clone(),
                game_name.clone(),
                user.borrow().emul_name.clone().into(),
                self.game_id,
            )
            .packetize()?;
            info!("S->C: CREATE_GAME id: {}", self.game_id);
            for (_, user) in &self.session_manager.users {
                user.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(CREATE_GAME, data.clone()))
                    .await?;
            }
        }
        let mut new_room = Room::new();
        new_room.creator_id = from_utf8_lossy(user.borrow().name.clone().as_slice()).to_string();
        new_room.emul_name = user.borrow().emul_name.clone();
        new_room.game_id = self.game_id;
        user.borrow_mut().game_room_id = Some(new_room.game_id);
        self.game_id += 1;
        new_room.game_name =
            String::from_utf8_lossy(iter.get(1).ok_or(KailleraError::NotFound)?).to_string();
        new_room.game_status = GAME_STATUS_WAITING;
        new_room
            .players
            .push(PlayerAddr::Idle(user.borrow().ip_addr));
        // update game status
        {
            let data = UpdateGameStatus2Client::new(
                new_room.game_id,
                new_room.game_status,
                new_room.player_some_count() as u8,
                4,
            )
            .packetize()?;
            for (_, user) in &self.session_manager.users {
                user.borrow_mut()
                    .make_send_packet(
                        &mut self.socket,
                        Protocol::new(UPDATE_GAME_STATUS, data.clone()),
                    )
                    .await?;
            }
        }
        // join game
        let new_room = Rc::new(RefCell::new(new_room));
        {
            // send data to room's players
            let data = JoinGame2Client::new(
                new_room.borrow().game_id,
                user.borrow().name.clone(),
                user.borrow().ping,
                user.borrow().user_id,
                user.borrow().connect_type,
            )
            .packetize()?;
            info!(
                "S->C: JOIN_GAME id: {}, user_id: {} ",
                new_room.borrow().game_id,
                user.borrow().user_id
            );
            user.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(JOIN_GAME, data))
                .await?;

            self.session_manager
                .send_game_chat_to_players(
                    &mut self.socket,
                    new_room.clone(),
                    "SERVER".to_string(),
                    "direlera supports follow the options\x00".as_bytes().into(),
                )
                .await?;
            self.session_manager
                .send_game_chat_to_players(
                    &mut self.socket,
                    new_room.clone(),
                    "SERVER".to_string(),
                    "/samedealy true|false\x00".as_bytes().into(),
                )
                .await?;
            // for (_, u) in &self.session_manager.users {
        }
        // server info
        {
            let mut data = Vec::new();
            data.append(&mut b"Server\x00".to_vec());
            let game_name_str =
                String::from_utf8_lossy(iter.get(1).ok_or(KailleraError::NotFound)?).to_string();
            let s = format!("Creates Room: {}\x00", game_name_str);
            data.append(&mut s.as_bytes().to_vec());
            user.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(SERVER_INFO, data))
                .await?;
        }
        let gi = new_room.borrow().game_id;
        self.session_manager.add_room(gi, new_room)?;

        Ok(())
    }
    pub async fn svc_join_game(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        info!("on svc_join_game");
        let has_room = user.borrow().game_room_id.is_some();
        if has_room {
            user.borrow_mut()
                .send_message(
                    &mut self.socket,
                    b"You are already joining the room.".to_vec(),
                )
                .await?;
            return Err(KailleraError::AlreadyError {
                message: "already has room.".to_string(),
            }
            .into());
        }
        let game_id = bincode::deserialize::<u32>(&buf[1..5])?;
        let _conn_type = buf.get(12).ok_or(KailleraError::NotFound);
        let join_room = self.session_manager.get_room(game_id)?;
        if join_room.borrow().game_status != GAME_STATUS_WAITING {
            return Err(KailleraError::GameStatusError {
                message: "game is playing".to_string(),
            }
            .into());
        }
        info!("[svc_join_game] game id: {}", game_id);

        join_room
            .borrow_mut()
            .players
            .push(PlayerAddr::Idle(user.borrow().ip_addr));
        user.borrow_mut().game_room_id = Some(game_id);

        // send join message to all users.
        let data = UpdateGameStatus2Client::new(
            game_id,
            join_room.borrow().game_status,
            join_room.borrow().player_some_count() as u8,
            4,
        )
        .packetize()?;
        for (_addr, user) in &self.session_manager.users {
            user.borrow_mut()
                .make_send_packet(
                    &mut self.socket,
                    Protocol::new(UPDATE_GAME_STATUS, data.clone()),
                )
                .await?;
        }
        // response game join message
        {
            let mut data = Vec::new();
            data.push(0u8);
            data.append(&mut bincode::serialize::<u32>(
                &(join_room.borrow().players.len() as u32 - 1),
            )?);
            for i in &join_room.borrow().players {
                if let PlayerAddr::Idle(addr) | PlayerAddr::Playing(addr) = *i {
                    let room_user = self.session_manager.get_user(addr)?;
                    let room_user = room_user.borrow();
                    data.append(&mut room_user.name.clone());
                    data.push(0u8);
                    data.append(&mut bincode::serialize::<u32>(&room_user.ping)?);
                    data.append(&mut bincode::serialize::<u16>(&room_user.user_id)?);
                    data.push(room_user.connect_type);
                }
            }
            user.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(PLAYER_INFO, data))
                .await?;
        }
        // send joingame to all users
        {
            let data = JoinGame2Client::new(
                game_id,
                user.borrow().name.clone(),
                user.borrow().ping,
                user.borrow().user_id,
                user.borrow().connect_type,
            )
            .packetize()?;
            // send data to room's player
            for i in &join_room.borrow().players {
                if let PlayerAddr::Idle(addr) | PlayerAddr::Playing(addr) = *i {
                    let room_user = self.session_manager.get_user(addr)?;
                    room_user
                        .borrow_mut()
                        .make_send_packet(&mut self.socket, Protocol::new(JOIN_GAME, data.clone()))
                        .await?;
                }
            }
        }

        Ok(())
    }
    pub async fn fun_quit_game(&mut self, user: Rc<RefCell<User>>) -> anyhow::Result<()> {
        if user.borrow().game_room_id.is_none() {
            anyhow::bail!("not exist in room")
        }

        // 게임방을 나갈 때, 게임 중인 경우와 아닌 경우를 다르게 구분해서 처리해야 함.
        // 싱크 갈림 현상을 대처하기 위해서 게임 중에는 플레이어 수를 변경하지 않으려 함.
        let room_id = match user.borrow().game_room_id {
            Some(id) => id,
            None => {
                return Err(KailleraError::NotFound.into());
            }
        };

        let user_room = self.session_manager.get_room(room_id)?;
        if user_room.borrow().game_status != GAME_STATUS_WAITING {
            let index = user.borrow().player_index as usize;
            {
                // version2
                let players = &mut user_room.borrow_mut().players;
                players
                    .get_mut(index)
                    .map(|player| *player = PlayerAddr::None);
            }
        } else {
            user_room.borrow_mut().players.retain(|&x| {
                let delete = {
                    match x {
                        PlayerAddr::Idle(i) | PlayerAddr::Playing(i) => i == user.borrow().ip_addr,
                        PlayerAddr::None => false,
                    }
                };
                !delete
            });
        }
        let mut close_game = false;
        if user_room.borrow().player_some_count() == 0 {
            self.session_manager.delete_room(room_id)?;
            close_game = true;
        }
        if close_game {
            info!("close game");
            // game close noti
            let mut data = Vec::new();
            data.push(0u8);
            data.append(&mut bincode::serialize(&user_room.borrow().game_id)?);
            for (_addr, u) in &self.session_manager.users {
                u.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(CLOSE_GAME, data.clone()))
                    .await?;
            }
        } else {
            info!("keep game room");
            // send game status noti to all
            let data = UpdateGameStatus2Client::new(
                user_room.borrow().game_id,
                user_room.borrow().game_status,
                user_room.borrow().players.len() as u8,
                4,
            )
            .packetize()?;
            for (_addr, u) in &self.session_manager.users {
                u.borrow_mut()
                    .make_send_packet(
                        &mut self.socket,
                        Protocol::new(UPDATE_GAME_STATUS, data.clone()),
                    )
                    .await?;
            }
        }
        let data =
            QuitGame2Client::new(user.borrow().name.clone(), user.borrow().user_id).packetize()?;
        for i in &user_room.borrow().players {
            if let PlayerAddr::Idle(addr) | PlayerAddr::Playing(addr) = *i {
                let u = self.session_manager.get_user(addr)?;
                u.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(QUIT_GAME, data.clone()))
                    .await?;
            }
        }
        user.borrow_mut().game_room_id = None;
        Ok(())
    }
    pub async fn svc_quit_game(
        &mut self,
        _buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        self.fun_quit_game(user).await
    }

    pub async fn svc_start_game(
        &mut self,
        _buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        let room_id = match user.borrow().game_room_id {
            Some(id) => id,
            None => {
                return Err(KailleraError::NotFound.into());
            }
        };
        let user_room = self.session_manager.get_room(room_id)?;
        user_room.borrow_mut().game_status = GAME_STATUS_NET_SYNC;
        // send UPDATE_GAME_STATUS to all
        let data = UpdateGameStatus2Client::new(
            user_room.borrow().game_id,
            user_room.borrow().game_status,
            user_room.borrow().players.len() as u8,
            4,
        )
        .packetize()?;
        for (_addr, u) in &self.session_manager.users {
            u.borrow_mut()
                .make_send_packet(
                    &mut self.socket,
                    Protocol::new(UPDATE_GAME_STATUS, data.clone()),
                )
                .await?;
        }
        // send GAME_START to room players
        let mut order = 0u8;

        let mut max_frame_delay = 0u16;
        for i in &user_room.borrow().players {
            let u = match i {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => self.session_manager.get_user(*i),
                PlayerAddr::None => continue,
            }?;
            let u = u.borrow();
            let frame_delay = Self::cal_frame_delay(u.connect_type, u.ping);
            if max_frame_delay < frame_delay {
                max_frame_delay = frame_delay;
            }
        }
        // show frame delay to all
        let mut delay_messages: Vec<Vec<u8>> = Vec::new();
        for i in &user_room.borrow().players {
            let u = match i {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => self.session_manager.get_user(*i),
                PlayerAddr::None => continue,
            }?;
            let mut u = u.borrow_mut();
            u.player_index = order;
            u.room_order = order;
            u.player_status = Playing;

            let real_frame_delay = Self::cal_frame_delay(u.connect_type, u.ping);
            let frame_delay = if user_room.borrow().same_delay {
                max_frame_delay
            } else {
                real_frame_delay
            };
            info!("frame_delay: {}", frame_delay);
            let mut notice_message = u.name.clone();
            if user_room.borrow().same_delay {
                notice_message.append(
                    &mut format!(
                        ", [samedelay mode] index {} -> {}",
                        real_frame_delay, max_frame_delay
                    )
                    .into_bytes(),
                );
            } else {
                notice_message.append(&mut ", frame delay(index): ".to_string().into_bytes());
                notice_message.append(&mut real_frame_delay.to_string().into_bytes());
            }
            notice_message.push(0u8);
            delay_messages.push(notice_message.clone());
            let data = StartGame2Client::new(
                frame_delay,
                order + 1,
                user_room.borrow().players.len() as u8,
            )
            .packetize()?;
            u.reset_outcoming();
            u.players_input
                .resize(user_room.borrow().players.len(), Vec::new());
            u.make_send_packet(&mut self.socket, Protocol::new(START_GAME, data))
                .await?;
            order += 1;
        }
        for i in delay_messages {
            self.session_manager
                .send_game_chat_to_players(
                    &mut self.socket,
                    user_room.clone(),
                    "SERVER".to_string(),
                    i.clone(),
                )
                .await?;
        }
        Ok(())
    }
    pub fn cal_frame_delay(connection_type: u8, ping: u32) -> u16 {
        match connection_type {
            1 => match ping {
                0..=16 => 1,
                17..=33 => 2,
                34..=49 => 3,
                50..=66 => 4,
                67..=83 => 5,
                84..=99 => 6,
                100..=116 => 7,
                117..=133 => 8,
                134..=149 => 9,
                150..=166 => 10,
                167..=183 => 11,
                184..=199 => 12,
                200..=216 => 13,
                217..=233 => 14,
                234..=249 => 15,
                250..=266 => 16,
                267..=283 => 17,
                284..=299 => 18,
                300..=316 => 19,
                317..=333 => 20,
                334..=349 => 21,
                _ => 22,
            },
            2 => match ping {
                0..=33 => 1,
                34..=66 => 2,
                67..=99 => 3,
                100..=133 => 4,
                134..=166 => 5,
                167..=199 => 6,
                200..=233 => 7,
                234..=266 => 8,
                267..=299 => 9,
                300..=333 => 10,
                _ => 11,
            },
            3 => match ping {
                0..=49 => 1,
                50..=99 => 2,
                100..=149 => 3,
                150..=199 => 4,
                200..=249 => 5,
                250..=299 => 6,
                300..=349 => 7,
                _ => 8,
            },
            4 => match ping {
                0..=66 => 1,
                67..=133 => 2,
                134..=199 => 3,
                200..=266 => 4,
                267..=333 => 5,
                _ => 6,
            },
            5 => match ping {
                0..=83 => 1,
                84..=166 => 2,
                167..=249 => 3,
                250..=333 => 4,
                _ => 5,
            },
            6 => match ping {
                0..=99 => 1,
                100..=199 => 2,
                200..=299 => 3,
                300..=399 => 4,
                _ => 5,
            },
            _ => 1_u16,
        }
    }

    pub async fn svc_game_data(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        let room_id = match user.borrow().game_room_id {
            Some(i) => i,
            None => {
                return Err(KailleraError::NotFound.into());
            }
        };
        let game_data_length = (bincode::deserialize::<u16>(&buf[1..3])?) as usize;
        if buf.len() < (3 + game_data_length) {
            anyhow::bail!("..");
        }
        let game_data = &buf[3..3 + game_data_length];
        let conntype = user.borrow().connect_type as u8;
        user.borrow_mut().atomic_input_size = game_data.len() as u8 / conntype;
        info!("atomic_input_size: {}", user.borrow().atomic_input_size);
        info!("game_data: {:?}", game_data);

        let user_room = self.session_manager.get_room(room_id)?;
        let target_user_index = user.borrow().player_index as usize;
        user.borrow_mut().cache_system.put_data(game_data.to_vec());

        // user 입력 game_data 을 방에 모든 인원의 메모리에 넣어야 함.
        for pi in &user_room.borrow().players {
            let u = match pi {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => {
                    self.session_manager.get_user(*i)?
                }
                PlayerAddr::None => continue,
            };
            u.borrow_mut().players_input[target_user_index].append(&mut game_data.to_vec().clone());
        }
        // InputProcess
        self.input_process(buf.clone(), user.clone()).await?;
        Ok(())
    }
    pub async fn svc_game_cache(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        let room_id = match user.borrow().game_room_id {
            Some(i) => i,
            None => {
                return Err(KailleraError::NotFound.into());
            }
        };
        if buf.len() != 2 {
            anyhow::bail!("!= 2");
        }
        let cache_position = buf[1];
        let input_data = user.borrow().cache_system.get_data(cache_position)?;
        let user_room = self.session_manager.get_room(room_id)?;
        let target_user_index = user.borrow().player_index as usize;

        for pi in &user_room.borrow().players {
            let u = match pi {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => {
                    self.session_manager.get_user(*i)?
                }
                PlayerAddr::None => continue,
            };
            u.borrow_mut().players_input[target_user_index].append(&mut input_data.clone());
        }
        self.input_process(buf.clone(), user.clone()).await?;

        Ok(())
    }
    pub async fn input_process(
        &mut self,
        _buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        let room_id = match user.borrow().game_room_id {
            Some(i) => i,
            None => {
                return Err(KailleraError::NotFound.into());
            }
        };
        let user_room = self.session_manager.get_room(room_id)?;
        // create packet each player
        for i in user_room.borrow().players.iter() {
            // select user to send data
            let u = match i {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => {
                    self.session_manager.get_user(*i)?
                }
                PlayerAddr::None => continue,
            };
            let data_to_send_to_user = UserRoom::gen_input(u.clone(), user_room.clone());
            if let Ok(data_to_send_to_user) = data_to_send_to_user {
                if !data_to_send_to_user.is_empty() {
                    let t = u
                        .borrow()
                        .put_cache
                        .get_cache_position(data_to_send_to_user.clone());
                    match t {
                        Ok(cache_position) => {
                            let data = GameCache2Client::new(cache_position).packetize()?;
                            u.borrow_mut()
                                .make_send_packet(&mut self.socket, Protocol::new(GAME_CACHE, data))
                                .await?;
                        }
                        Err(_e) => {
                            u.borrow_mut()
                                .put_cache
                                .put_data(data_to_send_to_user.clone());
                            trace!(
                                "cache len : {}",
                                u.borrow().put_cache.incoming_data_vec.len()
                            );
                            let data = GameData2Client::new(
                                data_to_send_to_user.len() as u16,
                                data_to_send_to_user,
                            )
                            .packetize()?;
                            u.borrow_mut()
                                .make_send_packet(&mut self.socket, Protocol::new(GAME_DATA, data))
                                .await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub async fn svc_drop_game(
        &mut self,
        _buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        let room_id = match user.borrow().game_room_id {
            Some(i) => i,
            None => {
                return Err(KailleraError::NotFound.into());
            }
        };
        let room = self.session_manager.get_room(room_id)?;
        // send UPDATE_GAME_STATUS to session_manager.users all
        let data = UpdateGameStatus2Client::new(
            room_id,
            room.borrow().game_status,
            room.borrow().players.len() as u8,
            4,
        )
        .packetize()?;
        for (_addr, u) in &self.session_manager.users {
            u.borrow_mut()
                .make_send_packet(
                    &mut self.socket,
                    Protocol::new(UPDATE_GAME_STATUS, data.clone()),
                )
                .await?;
        }

        // send DROP_GAME to room's users
        let data = GameDrop2Client::new(user.borrow().name.clone(), user.borrow().player_index + 1)
            .packetize()?;
        for i in room.borrow().players.iter() {
            let u = match i {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => {
                    self.session_manager.get_user(*i)?
                }
                PlayerAddr::None => continue,
            };
            u.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(DROP_GAME, data.clone()))
                .await?;
        }

        user.borrow_mut().player_status = Idle;

        let players = &mut room.borrow_mut().players;
        if let Some(PlayerAddr::Playing(u)) | Some(PlayerAddr::Idle(u)) =
            players.get_mut(user.borrow().player_index as usize)
        {
            *players
                .get_mut(user.borrow().player_index as usize)
                .unwrap() = PlayerAddr::Idle(*u);
        }
        Ok(())
    }
    pub async fn svc_kick_user(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        let room_id = match user.borrow().game_room_id {
            Some(i) => i,
            None => {
                return Err(KailleraError::NotFound.into());
            }
        };
        let room = self.session_manager.get_room(room_id)?;
        let target_user_id = bincode::deserialize::<u16>(&buf[1..3])?;

        // get user in room using target_user_id == User's user_id
        let target_user = {
            let mut target_user = None;
            for i in room.borrow().players.iter() {
                let u = match i {
                    PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => {
                        self.session_manager.get_user(*i)?
                    }
                    PlayerAddr::None => continue,
                };
                if u.borrow().user_id == target_user_id {
                    target_user = Some(u);
                    break;
                }
            }
            match target_user {
                Some(i) => i,
                None => anyhow::bail!("target user not found"),
            }
        };

        target_user.borrow_mut().game_room_id = None;
        let data =
            QuitGame2Client::new(target_user.borrow().name.clone(), target_user_id).packetize()?;
        // send QUIT_GAME data to room's users
        for i in room.borrow().players.iter() {
            let u = match i {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => {
                    self.session_manager.get_user(*i)?
                }
                PlayerAddr::None => continue,
            };
            u.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(QUIT_GAME, data.clone()))
                .await?;
        }

        // remove ip_addr target_user.borrow().ip_addr in room's players
        room.borrow_mut().players.retain(|i| {
            let delete = match i {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => *i == target_user.borrow().ip_addr,
                PlayerAddr::None => false,
            };
            !delete
        });

        // send UPDATE_GAME_STATUS to session_manager.users all
        let data = UpdateGameStatus2Client::new(
            room_id,
            room.borrow().game_status,
            room.borrow().players.len() as u8,
            4,
        )
        .packetize()?;
        for (_addr, u) in &self.session_manager.users {
            u.borrow_mut()
                .make_send_packet(
                    &mut self.socket,
                    Protocol::new(UPDATE_GAME_STATUS, data.clone()),
                )
                .await?;
        }
        Ok(())
    }
    pub async fn svc_ready_to_playsignal(
        &mut self,
        _buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        let room_id = match user.borrow().game_room_id {
            Some(i) => i,
            None => {
                return Err(KailleraError::NotFound.into());
            }
        };
        let user_room = self.session_manager.get_room(room_id)?;
        user_room.borrow_mut().game_status = GAME_STATUS_PLAYING;
        user_room.borrow_mut().players[user.borrow().player_index as usize] =
            PlayerAddr::Playing(user.borrow().ip_addr);
        let data = UpdateGameStatus2Client::new(
            room_id,
            user_room.borrow().game_status,
            user_room.borrow().players.len() as u8,
            4,
        )
        .packetize()?;
        for (_addr, u) in &self.session_manager.users {
            u.borrow_mut()
                .make_send_packet(
                    &mut self.socket,
                    Protocol::new(UPDATE_GAME_STATUS, data.clone()),
                )
                .await?;
        }
        for i in &user_room.borrow().players {
            let u = match i {
                PlayerAddr::Playing(i) | PlayerAddr::Idle(i) => self.session_manager.get_user(*i),
                PlayerAddr::None => continue,
            }?;
            let mut u = u.borrow_mut();
            u.make_send_packet(
                &mut self.socket,
                Protocol::new(READY_TO_PLAY_SIGNAL, b"\x00".to_vec()),
            )
            .await?;
        }
        Ok(())
    }
}
