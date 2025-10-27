use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::kaillera::message_types as msg;
use crate::simple_game_sync;
use crate::*;

pub async fn handle_message(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    match message.message_type {
        msg::USER_QUIT => user_quit::handle_user_quit(message, src, state).await?,
        msg::USER_LOGIN => user_login::handle_user_login(message, src, state).await?,
        // msg::SERVER_STATUS => handle_server_status(src, state).await?,
        // msg::SERVER_TO_CLIENT_ACK => handle_server_to_client_ack(message, src, state).await?,
        msg::CLIENT_TO_SERVER_ACK => handle_client_to_server_ack(src, state).await?,
        msg::GLOBAL_CHAT => global_chat::handle_global_chat(message, src, state).await?,
        msg::GAME_CHAT => game_chat::handle_game_chat(message, src, state).await?,
        msg::CLIENT_KEEP_ALIVE => handle_client_keep_alive(message, src).await?,
        msg::CREATE_GAME => create_game::handle_create_game(message, src, state).await?,
        msg::QUIT_GAME => handlers::quit_game::handle_quit_game(message.data, src, state).await?,
        msg::JOIN_GAME => join_game::handle_join_game(message, src, state).await?,
        msg::KICK_USER => {
            kick_user::handle_kick_user(message, src, state).await?;
        }
        msg::START_GAME => start_game::handle_start_game(message, src, state).await?,
        msg::GAME_DATA => {
            debug!(
                message_type = msg::message_type_name(message.message_type),
                "Game sync request received"
            );
            handle_game_data(message, src, state).await?;
        }
        msg::GAME_CACHE => handle_game_cache(message, src, state).await?,
        msg::DROP_GAME => drop_game::handle_drop_game(message, src, state).await?,
        msg::READY_TO_PLAY => handle_ready_to_play_signal(message, src, state).await?,

        _ => {
            warn!(
                message_type = msg::message_type_name(message.message_type),
                "Unknown message type received"
            );
        }
    }
    Ok(())
}

pub async fn handle_client_to_server_ack(
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    // Client to Server ACK [0x06]
    // Calculate ping and update ack count
    let ack_count = state
        .update_client::<_, u16, Box<dyn Error>>(src, |client_info| {
            if let Some(last_ping_time) = client_info.last_ping_time {
                let ping = last_ping_time.elapsed().as_millis() as u32;
                client_info.ping = ping;
                client_info.last_ping_time = Some(Instant::now());
                client_info.ack_count += 1;
            }
            Ok(client_info.ack_count)
        })
        .await?;

    if ack_count >= 3 {
        let data = util::make_server_status(src, &state).await?;
        util::send_packet(&state, src, msg::SERVER_STATUS, data).await?;

        let data = util::make_user_joined(src, &state).await?;
        util::broadcast_packet(&state, msg::USER_JOINED, data).await?;

        let data = util::make_server_information()?;
        util::send_packet(&state, src, msg::SERVER_INFORMATION, data).await?;
    } else {
        // Server notification creation
        let mut data = BytesMut::new();
        data.put_u8(0);
        data.put_u32_le(0);
        data.put_u32_le(1);
        data.put_u32_le(2);
        data.put_u32_le(3);
        util::send_packet(&state, src, msg::SERVER_TO_CLIENT_ACK, data.to_vec()).await?;
    }

    Ok(())
}

pub async fn handle_client_keep_alive(
    _message: kaillera::protocol::ParsedMessage,
    _src: &std::net::SocketAddr,
) -> Result<(), Box<dyn Error>> {
    // No additional handling needed
    Ok(())
}

pub async fn handle_ready_to_play_signal(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    debug!("Ready to play signal received");
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = buf.get_u8(); // Empty String

    state
        .update_client::<_, (), Box<dyn Error>>(src, |client_info| {
            client_info.player_status = PLAYER_STATUS_NET_SYNC; // Ready to play
            Ok(())
        })
        .await?;

    let game_info_clone = util::fetch_game_info(src, &state).await?;

    // Update game status
    {
        let status_data = util::make_update_game_status(&game_info_clone)?;
        util::broadcast_packet(&state, msg::UPDATE_GAME_STATUS, status_data).await?;
    }

    // Check if all users are ready
    let all_user_ready_to_signal = {
        let addr_map = state.clients_by_addr.read().await;
        let id_map = state.clients_by_id.read().await;

        let all_ready = game_info_clone.players.iter().all(|player_addr| {
            if let Some(session_id) = addr_map.get(player_addr) {
                if let Some(client_info) = id_map.get(session_id) {
                    debug!(
                        { fields::ADDR } = %player_addr,
                        player_status = client_info.player_status,
                        "Checking player status"
                    );
                    return client_info.player_status == PLAYER_STATUS_NET_SYNC;
                }
            }
            debug!(
                { fields::ADDR } = %player_addr,
                "Client info not found"
            );
            false
        });
        all_ready
    };

    // If all ready, update all players' status
    if all_user_ready_to_signal {
        for player_addr in game_info_clone.players.iter() {
            let _ = state
                .update_client::<_, (), Box<dyn Error>>(player_addr, |client_info| {
                    client_info.player_status = PLAYER_STATUS_PLAYING;
                    Ok(())
                })
                .await;
        }
    }

    // Send ready to play signal notification
    if all_user_ready_to_signal {
        info!(
            { fields::PLAYER_COUNT } = game_info_clone.players.len(),
            "All users ready to signal - starting game"
        );
        for player_addr in game_info_clone.players.iter() {
            let mut data = BytesMut::new();
            data.put_u8(0);
            util::send_packet(&state, player_addr, msg::READY_TO_PLAY, data.to_vec()).await?;
        }
    }
    Ok(())
}

/*
- **NB**: Empty String `[00]`
- **2B**: Length of Game Data
- **NB**: Game Data
 */
pub async fn handle_game_data(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    debug!("Game data received");
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = buf.get_u8(); // Empty String
    let data_length = buf.get_u16_le() as usize;
    let game_data = buf.split_to(data_length).to_vec();

    let client = state.get_client(src).await.ok_or("Client not found")?;
    let game_id = client.game_id.ok_or("Game ID not found")?;

    // Find player_id from address
    let game_info = state.get_game(game_id).await.ok_or("Game not found")?;
    let player_id = game_info
        .player_addrs
        .iter()
        .position(|addr| addr == src)
        .ok_or("Player not in game")?;

    debug!(
        { fields::PLAYER_ID } = player_id,
        { fields::DATA_LENGTH } = game_data.len(),
        "Player sent game data"
    );

    // Process with SimpleGameSync
    let outputs = {
        let mut games = state.games.write().await;
        let game_info = games.get_mut(&game_id).ok_or("Game not found")?;

        let sync_manager = game_info
            .sync_manager
            .as_mut()
            .ok_or("SimpleGameSync not initialized")?;

        // Process input using SimpleGameSync
        sync_manager.process_client_input(
            player_id,
            simple_game_sync::ClientInput::GameData(game_data),
        )
    };

    // Send outputs to respective players
    for output in outputs {
        let target_addr = &game_info.player_addrs[output.player_id];

        let (message_type, data_to_send) = match output.response {
            simple_game_sync::ServerResponse::GameData(data) => {
                let mut buf = BytesMut::new();
                buf.put_u8(0); // Empty string
                buf.put_u16_le(data.len() as u16);
                buf.put(data.as_slice());
                (msg::GAME_DATA, buf.to_vec())
            }
            simple_game_sync::ServerResponse::GameCache(position) => (msg::GAME_CACHE, vec![0x00, position]),
        };

        debug!(
            { fields::PLAYER_ID } = output.player_id,
            message_type = msg::message_type_name(message_type),
            { fields::DATA_LENGTH } = data_to_send.len(),
            "Sending game data to player"
        );
        util::send_packet(&state, target_addr, message_type, data_to_send).await?;
    }

    Ok(())
}

pub async fn handle_game_cache(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    debug!("Game cache received");
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = buf.get_u8(); // Empty String
    let cache_position = buf.get_u8();

    let client = state.get_client(src).await.ok_or("Client not found")?;
    let game_id = client.game_id.ok_or("Game ID not found")?;

    // Find player_id from address
    let game_info = state.get_game(game_id).await.ok_or("Game not found")?;
    let player_id = game_info
        .player_addrs
        .iter()
        .position(|addr| addr == src)
        .ok_or("Player not in game")?;

    debug!(
        { fields::PLAYER_ID } = player_id,
        { fields::CACHE_POSITION } = cache_position,
        "Player sent cache position"
    );

    // Process with SimpleGameSync
    let outputs = {
        let mut games = state.games.write().await;
        let game_info = games.get_mut(&game_id).ok_or("Game not found")?;

        let sync_manager = game_info
            .sync_manager
            .as_mut()
            .ok_or("SimpleGameSync not initialized")?;

        // Process input using SimpleGameSync
        sync_manager.process_client_input(
            player_id,
            simple_game_sync::ClientInput::GameCache(cache_position),
        )
    };

    // Send outputs to respective players
    for output in outputs {
        let target_addr = &game_info.player_addrs[output.player_id];

        let (message_type, data_to_send) = match output.response {
            simple_game_sync::ServerResponse::GameData(data) => {
                let mut buf = BytesMut::new();
                buf.put_u8(0); // Empty string
                buf.put_u16_le(data.len() as u16);
                buf.put(data.as_slice());
                (msg::GAME_DATA, buf.to_vec())
            }
            simple_game_sync::ServerResponse::GameCache(position) => (msg::GAME_CACHE, vec![0x00, position]),
        };

        debug!(
            { fields::PLAYER_ID } = output.player_id,
            message_type = msg::message_type_name(message_type),
            { fields::DATA_LENGTH } = data_to_send.len(),
            "Sending cache data to player"
        );
        util::send_packet(&state, target_addr, message_type, data_to_send).await?;
    }

    Ok(())
}
