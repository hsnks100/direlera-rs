use crate::*;
use bytes::{Buf, BytesMut};
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use tracing::info;

use crate::kaillera::message_types as msg;

// Refactored handle_create_game function
pub async fn handle_create_game(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    // Check if user is already in a game
    if let Some(client_info) = state.get_client(src).await {
        if let Some(existing_game_id) = client_info.game_id {
            tracing::warn!(
                { fields::USER_NAME } = client_info.username.as_str(),
                { fields::GAME_ID } = existing_game_id,
                "User attempted to create game while already in a game"
            );
            return Ok(()); // Silently ignore invalid request
        }
    }

    // Parse the message to extract game_name
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = util::read_string(&mut buf); // Empty String
    let game_name = util::read_string(&mut buf); // Game Name
    let _ = util::read_string(&mut buf); // Empty String
    let _ = if buf.len() >= 4 { buf.get_u32_le() } else { 0 }; // 4B: 0xFF

    // Lock-free ID generation!
    let game_id = state.next_game_id();

    // Get client_info
    let (username, emulator_name, conn_type, user_id) =
        util::fetch_client_info(src, &state).await?;

    // Create new game
    let mut players = HashSet::new();
    players.insert(*src);
    let game_info = GameInfo {
        game_id,
        game_name: game_name.clone(),
        emulator_name: emulator_name.clone(),
        owner: username.clone(),
        owner_user_id: user_id, // Store owner's user_id for authorization
        num_players: 1,
        max_players: 4,
        game_status: 0, // Waiting
        players,
        sync_manager: None, // Will be initialized when game starts
        player_addrs: vec![*src],
        player_delays: vec![conn_type as usize], // Use creator's connection_type as delay
    };

    // Add game
    state.add_game(game_id, game_info.clone()).await;

    util::with_client_mut(&state, src, |client_info| {
        client_info.game_id = Some(game_id);
    })
    .await?;

    info!(
        { fields::GAME_ID } = game_id,
        { fields::GAME_NAME } = game_name.as_str(),
        { fields::USER_NAME } = username.as_str(),
        emulator = emulator_name.as_str(),
        "Game created"
    );

    // Build data for new game notification
    let data = util::build_new_game_notification(&username, &game_name, &emulator_name, game_id);

    // Broadcast new game notification to all clients
    util::broadcast_packet(&state, msg::CREATE_GAME, data).await?;

    // Send game status update to the client
    let status_data = util::make_update_game_status(&game_info)?;
    util::broadcast_packet(&state, msg::UPDATE_GAME_STATUS, status_data).await?;

    // Send player information (empty list for the creator)
    let players_info = util::make_player_information(src, &state, &game_info).await?;
    util::send_packet(&state, src, msg::PLAYER_INFORMATION, players_info).await?;

    // Build and send join game response
    let response_data = {
        let client_info = state.get_client(src).await.ok_or("Client not found")?;
        util::build_join_game_response(&client_info)
    };
    util::send_packet(&state, src, msg::JOIN_GAME, response_data).await?;

    Ok(())
}
