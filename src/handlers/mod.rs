pub mod chat;
pub mod game;
pub mod sync;
pub mod user;
pub mod util;

use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, warn};

use crate::kaillera::message_types as msg;
use crate::*;

pub async fn handle_message(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> color_eyre::Result<()> {
    match message.message_type {
        msg::USER_QUIT => user::handle_user_quit(message, src, state).await?,
        msg::USER_LOGIN => user::handle_user_login(message, src, state).await?,
        msg::CLIENT_TO_SERVER_ACK => handle_client_to_server_ack(src, state).await?,
        msg::GLOBAL_CHAT => chat::handle_global_chat(message, src, state).await?,
        msg::GAME_CHAT => chat::handle_game_chat(message, src, state).await?,
        msg::CLIENT_KEEP_ALIVE => handle_client_keep_alive(message, src).await?,
        msg::CREATE_GAME => game::handle_create_game(message, src, state).await?,
        msg::QUIT_GAME => game::handle_quit_game(message.data, src, state).await?,
        msg::JOIN_GAME => game::handle_join_game(message, src, state).await?,
        msg::KICK_USER => game::handle_kick_user(message, src, state).await?,
        msg::START_GAME => game::handle_start_game(message, src, state).await?,
        msg::GAME_DATA => {
            debug!(
                message_type = msg::message_type_name(message.message_type),
                "Game sync request received"
            );
            sync::handle_game_data(message, src, state).await?;
        }
        msg::GAME_CACHE => sync::handle_game_cache(message, src, state).await?,
        msg::DROP_GAME => game::handle_drop_game(message, src, state).await?,
        msg::READY_TO_PLAY => sync::handle_ready_to_play_signal(message, src, state).await?,

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
) -> color_eyre::Result<()> {
    // Client to Server ACK [0x06]
    // Calculate ping and update ack count
    let ack_count = state
        .update_client::<_, u16, color_eyre::Report>(src, |client_info| {
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
        let data = packet_util::build_server_to_client_ack_packet();
        util::send_packet(&state, src, msg::SERVER_TO_CLIENT_ACK, data).await?;
    }

    Ok(())
}

pub async fn handle_client_keep_alive(
    _message: kaillera::protocol::ParsedMessage,
    _src: &std::net::SocketAddr,
) -> color_eyre::Result<()> {
    // No additional handling needed
    Ok(())
}
