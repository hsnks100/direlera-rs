use bytes::{Buf, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::{debug, info};

use crate::kaillera::message_types as msg;
use crate::util::*;
use crate::*;

pub async fn handle_join_game(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    // Parse message
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = read_string(&mut buf);
    let game_id = buf.get_u32_le();
    let _ = read_string(&mut buf);
    let _ = buf.get_u32_le();
    let _ = buf.get_u16_le();
    let _conn_type = buf.get_u8();

    // Get joining player's connection type
    let client = state.get_client(src).await.ok_or("Client not found")?;
    let conn_type = client.conn_type;

    // Prevent joining if user is already in any game (same or different)
    if let Some(current_game_id) = client.game_id {
        tracing::warn!(
            { fields::USER_NAME } = client.username.as_str(),
            { fields::GAME_ID } = game_id,
            current_game_id = current_game_id,
            "User attempted to join game while already in a game"
        );
        return Ok(()); // Silently ignore invalid request
    }

    util::with_client_mut(&state, src, |client_info| {
        client_info.game_id = Some(game_id);
    })
    .await?;

    let username = client.username.clone();

    util::with_game_mut(&state, src, |game_info| {
        // Only add if not already in the game (prevents duplicates)
        if game_info.players.insert(*src) {
            game_info.num_players += 1;
            game_info.player_addrs.push(*src);
            game_info.player_delays.push(conn_type as usize);
        } else {
            debug!(
                { fields::GAME_ID } = game_id,
                "Player already in game, skipping duplicate"
            );
        }
    })
    .await?;

    // Generate game status data
    let game_info = state.get_game(game_id).await.ok_or("Game not found")?;
    let status_data = util::make_update_game_status(&game_info)?;

    info!(
        { fields::GAME_ID } = game_id,
        { fields::USER_NAME } = username.as_str(),
        { fields::PLAYER_COUNT } = game_info.num_players,
        "Player joined game"
    );

    // Broadcast game status update to all clients
    let client_addresses = state.get_all_client_addrs().await;
    for addr in client_addresses {
        util::send_packet(&state, &addr, msg::UPDATE_GAME_STATUS, status_data.clone()).await?;
    }

    // Generate player information and send to joining client
    let players_info = util::make_player_information(src, &state, &game_info).await?;
    util::send_packet(&state, src, msg::PLAYER_INFORMATION, players_info.clone()).await?;

    // Generate join game response data
    let response_data = {
        let client_info = state.get_client(src).await.ok_or("Client not found")?;
        util::build_join_game_response(&client_info)
    };

    // Send join game notification to ALL players (including the joining player)
    // Each player manages their own list, so we send the new player info to everyone
    let game_players = game_info.players.clone();
    for player_addr in game_players {
        util::send_packet(&state, &player_addr, msg::JOIN_GAME, response_data.clone()).await?;
    }

    Ok(())
}
