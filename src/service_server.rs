use crate::protocol::*;
use crate::room::*;

use log::{info, trace, warn};
use std::cell::RefCell;
use std::error::Error;
use std::io::Read;
use std::net::SocketAddr;
use std::rc::Rc;
use std::{cmp::*, io};
use tokio::net::UdpSocket;

pub struct ServiceServer {
    pub socket: UdpSocket,
    pub buf: Vec<u8>,
    pub to_send: Option<(usize, SocketAddr)>,
    pub user_room: UserRoom,
    pub game_id: u32,
}

impl ServiceServer {
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        info!("Service Run");
        loop {
            if let Some((size, peer)) = self.to_send {
                let result = self.service_proc(size, peer).await;
                if result.is_err() {
                    info!("err content: {:?}", result.err());
                }
            }
            self.to_send = Some(self.socket.recv_from(&mut self.buf).await?);
        }
    }
    pub async fn service_proc(
        &mut self,
        size: usize,
        peer: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        // info!("service size: {}, ", size);
        let r = get_protocol_from_bytes(&self.buf[..size].to_vec())?;
        let user = {
            match self.user_room.users.get(&peer) {
                Some(i) => i.clone(),
                None => Rc::new(RefCell::new(User::new(peer))),
            }
        };
        let messages: Vec<_> = r
            .iter()
            .filter(|&n| n.header.seq == user.borrow().cur_seq)
            .collect();
        info!(
            "message len: {}, want seq: {}, r: {}",
            messages.len(),
            user.borrow().cur_seq,
            r.len(),
        );

        let message = messages.get(0).ok_or(KailleraError::NotFound)?;
        let user = user.clone();
        user.borrow_mut().cur_seq += 1;
        info!(
            "recv message_type: {:?}, content: {:?}",
            message.header.message_type, message.data,
        );
        if message.header.message_type == USER_QUIT {
            self.svc_user_quit(message.data.clone(), user).await?;
        } else if message.header.message_type == USER_LOGIN_INFO {
            self.user_room.users.insert(peer, user.clone());
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
            self.svc_game_chat(message.data.clone(), peer).await?;
        } else if message.header.message_type == CREATE_GAME {
            self.svc_create_game(message.data.clone(), peer).await?;
        } else if message.header.message_type == QUIT_GAME {
            self.svc_quit_game(message.data.clone(), user).await?;
        } else if message.header.message_type == JOIN_GAME {
            self.svc_join_game(message.data.clone(), peer).await?;
        } else if message.header.message_type == KICK_USER_FROM_GAME {
            // self.svc_join_game(message.data.clone(), peer).await?;
        } else if message.header.message_type == START_GAME {
            self.svc_start_game(message.data.clone(), user).await?;
        } else if message.header.message_type == GAME_DATA {
            self.svc_game_data(message.data.clone(), user).await?;
        } else if message.header.message_type == GAME_CACHE {
            self.svc_game_cache(message.data.clone(), user).await?;
        } else if message.header.message_type == DROP_GAME {
            // self.svc_join_game(message.data.clone(), peer).await?;
        } else if message.header.message_type == READY_TO_PLAY_SIGNAL {
            self.svc_ready_to_playsignal(message.data.clone(), user)
                .await?;
        }

        Ok(())
    }
    pub async fn svc_user_quit(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> Result<(), Box<dyn Error>> {
        info!("== svc_user_quit ==");
        let client_message = &buf[3..];
        // send quit message to all
        for (addr, u) in &self.user_room.users {
            let mut data = Vec::new();
            data.append(&mut user.borrow().name.clone().into_bytes());
            data.push(0u8);
            data.append(&mut bincode::serialize(&user.borrow().user_id)?);
            data.append(&mut client_message.to_vec().clone());
        }
        self.user_room.users.remove(&user.borrow().ip_addr);
        Ok(())
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
        info!("login info: {} {} {}", user_name, emul_name, conn_type);

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
        info!("on svc_ack");
        let user_room = &mut self.user_room;
        let user = user_room.get_user(ip_addr)?;
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
                let mut name = user.borrow().name.clone().as_bytes().to_vec();
                data.append(&mut name);
                data.push(0u8);
                data.append(&mut bincode::serialize::<u16>(&user.borrow().user_id)?);
                data.append(&mut bincode::serialize::<u32>(&user.borrow().ping)?);
                data.push(user.borrow().connect_type);
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
        let message = buf[1..].to_vec();
        let mut data = Vec::new();
        data.append(&mut user.borrow().name.clone().as_bytes().to_vec());
        data.push(0u8);
        data.append(&mut message.clone());
        // data.append(&mut "TEST string\x00".to_string().into_bytes());
        for i in &self.user_room.users {
            i.1.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(GLOBAL_CHAT, data.clone()))
                .await?;
        }
        println!(
            "chat message: {}",
            String::from_utf8_lossy(&message.clone())
        );

        if String::from_utf8_lossy(&message.clone()) == "ts\x00" {
            println!("admin mode");
            let d = self.user_room.to_string();
            for i in &self.user_room.users {
                let d = d.clone();
                let split_data: Vec<_> = d.split('\n').collect();
                for each_data in split_data {
                    if each_data.len() > 0 {
                        let mut data = Vec::new();
                        data.append(&mut user.borrow().name.clone().as_bytes().to_vec());
                        data.push(0u8);
                        data.append(&mut each_data.to_string().into_bytes());
                        data.push(0u8);
                        i.1.borrow_mut()
                            .make_send_packet(
                                &mut self.socket,
                                Protocol::new(GLOBAL_CHAT, data.clone()),
                            )
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }
    pub async fn svc_game_chat(
        &mut self,
        buf: Vec<u8>,
        ip_addr: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        // let user_room = &self.user_room;
        let user = self.user_room.get_user(ip_addr)?;
        if user.borrow().in_room {
            let room = self.user_room.get_room(user.borrow().game_room_id)?;
            let mut ips = Vec::new();
            for i in &room.borrow().players {
                ips.push(*i);
            }

            for i in ips {
                match i {
                    Some(s) => {
                        let u = self.user_room.get_user(s)?;
                        let mut data = Vec::new();
                        data.append(&mut user.borrow().name.clone().as_bytes().to_vec());
                        data.push(0u8);
                        data.append(&mut buf.clone()[1..].to_vec());
                        u.borrow_mut()
                            .make_send_packet(&mut self.socket, Protocol::new(GAME_CHAT, data))
                            .await?;
                    }
                    None => {}
                }
                if i.is_some() {}
            }
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
        // create game packet
        {
            let game_name = iter.get(1).ok_or(KailleraError::NotFound)?.to_vec();
            let mut data = Vec::new();
            data.append(&mut user.borrow().name.clone().as_bytes().to_vec());
            data.push(0u8);
            data.append(&mut game_name.clone());
            data.push(0u8);
            data.append(&mut user.borrow().emul_name.clone().as_bytes().to_vec());
            data.push(0u8);
            data.append(&mut bincode::serialize::<u32>(&self.game_id)?);
            for (_, user) in &self.user_room.users {
                user.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(CREATE_GAME, data.clone()))
                    .await?;
            }
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
            for (_, u) in &self.user_room.users {
                let mut data = Vec::new();
                data.push(0u8);
                data.append(&mut bincode::serialize::<u32>(&new_room.game_id)?);
                data.append(&mut user.borrow().name.clone().as_bytes().to_vec());
                data.push(0u8);
                data.append(&mut bincode::serialize::<u32>(&user.borrow().ping)?);
                data.append(&mut bincode::serialize::<u16>(&user.borrow().user_id)?);
                data.push(user.borrow().connect_type);
                u.borrow_mut()
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
    pub async fn svc_join_game(
        self: &mut Self,
        buf: Vec<u8>,
        ip_addr: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        info!("on svc_join_game");
        let user_room = &mut self.user_room;
        let game_id = bincode::deserialize::<u32>(&buf[1..5])?;
        let user = user_room.get_user(ip_addr)?;
        let conn_type = buf.get(12).ok_or(KailleraError::NotFound);
        let join_room = self.user_room.get_room(game_id)?;
        join_room
            .borrow_mut()
            .players
            .push(Some(user.borrow().ip_addr));
        user.borrow_mut().game_room_id = game_id;
        user.borrow_mut().in_room = true;

        // send join message to all users.
        for (addr, user) in &self.user_room.users {
            let mut data = Vec::new();
            data.push(0u8);
            data.append(&mut bincode::serialize::<u32>(&game_id)?);
            data.push(join_room.borrow().game_status);
            data.push(join_room.borrow().players.len() as u8);
            data.push(4 as u8);
            user.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(UPDATE_GAME_STATUS, data))
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
                if let Some(addr) = *i {
                    let room_user = self.user_room.get_user(addr)?;
                    let room_user = room_user.borrow();
                    data.append(&mut room_user.name.clone().as_bytes().to_vec());
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
            let mut data = Vec::new();
            data.push(0u8);
            data.append(&mut bincode::serialize::<u32>(&game_id)?);
            data.append(&mut user.borrow().name.clone().as_bytes().to_vec());
            data.push(0u8);
            data.append(&mut bincode::serialize::<u32>(&user.borrow().ping)?);
            data.append(&mut bincode::serialize::<u16>(&user.borrow().user_id)?);
            data.push(user.borrow().connect_type);
            for (addr, u) in &self.user_room.users {
                u.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(JOIN_GAME, data.clone()))
                    .await?;
            }
        }

        Ok(())
    }
    pub async fn svc_quit_game(
        self: &mut Self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        if !user.borrow().in_room {
            anyhow::bail!("not exist in room")
        }

        // 게임방을 나갈 때, 게임 중인 경우와 아닌 경우를 다르게 구분해서 처리해야 함.
        // 싱크 갈림 현상을 대처하기 위해서 게임 중에는 플레이어 수를 변경하지 않으려 함.

        let user_room = self.user_room.get_room(user.borrow().game_room_id)?;
        if user_room.borrow().game_status != GameStatusWaiting {
            let index = user.borrow().player_order as usize - 1;
            {
                // version2
                let players = &mut user_room.borrow_mut().players;
                players.get_mut(index).map(|player| *player = None);
            }
        } else {
            user_room.borrow_mut().players.retain(|&x| {
                let delete = {
                    match x {
                        Some(i) => i == user.borrow().ip_addr,
                        None => false,
                    }
                };
                !delete
            });
        }
        let mut close_game = false;
        if user_room.borrow().player_count() == 0 {
            self.user_room.delete_room(user.borrow().game_room_id)?;
            close_game = true;
        }
        if close_game {
            info!("close game");
            // game close noti
            let mut data = Vec::new();
            data.push(0u8);
            data.append(&mut bincode::serialize(&user_room.borrow().game_id)?);
            for (addr, u) in &self.user_room.users {
                u.borrow_mut()
                    .make_send_packet(&mut self.socket, Protocol::new(CLOSE_GAME, data.clone()))
                    .await?;
            }
        } else {
            info!("keep game room");
            // send game status noti to all
            let mut data = Vec::new();
            data.push(0u8);
            data.append(&mut bincode::serialize(&user_room.borrow().game_id)?);
            data.push(user_room.borrow().game_status);
            data.push(user_room.borrow().players.len() as u8);
            data.push(4u8);
            for (addr, u) in &self.user_room.users {
                u.borrow_mut()
                    .make_send_packet(
                        &mut self.socket,
                        Protocol::new(UPDATE_GAME_STATUS, data.clone()),
                    )
                    .await?;
            }
        }
        // send quit game to all
        // 		for _, u := range s.userChannel.Users {
        // 	data := make([]byte, 0)
        // 	data = append(data, []byte(user.Name+"\x00")...)
        // 	data = append(data, Uint16ToBytes(user.UserId)...)
        // 	u.SendPacket(server, *NewProtocol(MessageTypeQuitGame, data))
        // }
        // user.InRoom = false
        let mut data = Vec::new();
        data.append(&mut user.borrow().name.clone().into_bytes());
        data.push(0u8);
        data.append(&mut bincode::serialize(&user.borrow().user_id)?);
        for (addr, u) in &self.user_room.users {
            u.borrow_mut()
                .make_send_packet(&mut self.socket, Protocol::new(QUIT_GAME, data.clone()))
                .await?;
        }
        user.borrow_mut().in_room = false;
        Ok(())
    }

    pub async fn svc_start_game(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        let user_room = self.user_room.get_room(user.borrow().game_room_id)?;
        user_room.borrow_mut().game_status = GameStatusNetSync;
        // send UPDATE_GAME_STATUS to all
        let mut data = Vec::new();
        data.push(0u8);
        data.push(user_room.borrow().game_status);
        data.append(&mut bincode::serialize(
            &(user_room.borrow().players.len() as u8),
        )?);
        data.push(4u8);
        for (addr, u) in &self.user_room.users {
            u.borrow_mut()
                .make_send_packet(
                    &mut self.socket,
                    Protocol::new(UPDATE_GAME_STATUS, data.clone()),
                )
                .await?;
        }
        // send UPDATE_GAME_STATUS to room users
        let mut order = 0u8;
        for i in &user_room.borrow().players {
            let u = match i {
                Some(i) => self.user_room.get_user(*i),
                None => continue,
            }?;
            let mut u = u.borrow_mut();
            u.player_order = order;
            u.room_order = order;
            u.player_status = Playing;

            let mut data = Vec::new();
            data.push(0u8);
            data.append(&mut bincode::serialize(&(1 as u16))?);
            data.push(order);
            data.push(user_room.borrow().players.len() as u8);
            u.reset_outcoming();
            u.make_send_packet(&mut self.socket, Protocol::new(START_GAME, data))
                .await?;
            order += 1;
        }
        Ok(())
    }

    pub async fn svc_game_data(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    pub async fn svc_game_cache(
        &mut self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    pub async fn svc_ready_to_playsignal(
        &self,
        buf: Vec<u8>,
        user: Rc<RefCell<User>>,
    ) -> anyhow::Result<()> {
        //
        Ok(())
    }
}
