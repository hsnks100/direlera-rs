use crate::*;
use bytes::BytesMut;
use color_eyre::eyre::{eyre, WrapErr};
use std::sync::Arc;
use tracing::info;

use super::util;
use crate::kaillera::message_types as msg;

pub async fn handle_global_chat(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> color_eyre::Result<()> {
    let mut buf = BytesMut::from(&message.data[..]);

    // NB: Empty String
    let _empty = util::read_string(&mut buf);
    // NB: Message
    let chat_message = util::read_string(&mut buf);

    // Get username from clients list
    let username = if let Some(client_info) = state.get_client(src).await {
        client_info.username.clone()
    } else {
        "Unknown".to_string()
    };

    info!("Global chat message: {}", chat_message);

    // Server notification creation
    let data = packet_util::build_global_chat_packet(&username, &chat_message);

    // Send message to all clients
    util::broadcast_packet(&state, msg::GLOBAL_CHAT, data)
        .await
        .wrap_err("Failed to broadcast global chat message")?;

    Ok(())
}

pub async fn handle_game_chat(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> color_eyre::Result<()> {
    let mut buf = BytesMut::from(&message.data[..]);

    // NB: Empty String
    let _empty = util::read_string(&mut buf);
    // NB: Message
    let chat_message = util::read_string(&mut buf);

    // Check if client exists and is in a game
    let client_info = state
        .get_client(src)
        .await
        .ok_or_else(|| eyre!("Client not found"))?;
    let game_id = client_info
        .game_id
        .ok_or_else(|| eyre!("Client attempted game chat but not in a game"))?;

    // Validate message content
    let message_bytes = chat_message.as_bytes();
    if message_bytes.contains(&0x11) {
        info!("skipping game chat message containing 0x11");
        return Ok(());
    }

    info!("Game chat message: {}", chat_message);

    // Build and broadcast packet to all players in the game
    let data = packet_util::build_game_chat_packet(&client_info.username, &chat_message);
    util::broadcast_packet_to_game(&state, game_id, msg::GAME_CHAT, data)
        .await
        .wrap_err_with(|| format!("Failed to broadcast game chat to game {}", game_id))?;

    Ok(())
}
