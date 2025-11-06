use crate::*;
use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::kaillera::message_types as msg;
use crate::simplest_game_sync::CachedGameSync;
/*
'     0x11 = Start Game
'            Client Request:
'            NB : Empty String [00]
'            2B : 0xFF
'            1B : 0xFF
'            1B : 0xFF
'
'            Server Notification:
'            NB : Empty String [00]
'            2B : Frame Delay (eg. (connectionType * (frameDelay + 1) <-Block on that frame
'            1B : Your Player Number (eg. if you're player 1 or 2...)
'            1B : Total Players
- **Client**: Sends **Start Game Request** `[0x11]`
- **Server**: Sends **Update Game Status** `[0x0E]`
- **Server**: Sends **Start Game Notification** `[0x11]`
- **Client**: Enters **Netsync Mode** and waits for all players to send **Ready to Play Signal** `[0x15]`
- **Server**: Sends **Update Game Status** for whole server players`[0x0E]`
- **Server**: Enters **Playing Mode** after receiving **Ready to Play Signal Notification** `[0x15]` from all players in room
- **Client(s)**: Exchange data using **Game Data Send** `[0x12]` or **Game Cache Send** `[0x13]`
- **Server**: Sends data accordingly using **Game Data Notify** `[0x12]` or **Game Cache Notify** `[0x13]`

 */
pub async fn handle_start_game(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = util::read_string(&mut buf); // Empty String
    let _ = buf.get_u32_le(); // 0xFFFF 0xFF 0xFF

    let client = state.get_client(src).await.ok_or("Client not found")?;
    let requester_username = client.username.clone();
    let requester_user_id = client.user_id;
    let game_id = client.game_id.ok_or("Client not in a game")?;

    // Check if requester is the game owner (using user_id to prevent nickname abuse)
    let game_info = state.get_game(game_id).await.ok_or("Game not found")?;
    if game_info.sync_manager.is_some() {
        warn!(
            { fields::USER_NAME } = requester_username.as_str(),
            { fields::USER_ID } = requester_user_id,
            { fields::GAME_ID } = game_id,
            "Game not started"
        );
        // let chat_message = util::make_game_chat(requester_username, game_id, "Game not started");
        return Ok(()); // Silently ignore invalid request
    }
    if game_info.owner_user_id != requester_user_id {
        warn!(
            { fields::USER_NAME } = requester_username.as_str(),
            { fields::USER_ID } = requester_user_id,
            { fields::GAME_ID } = game_id,
            owner_user_id = game_info.owner_user_id,
            "Non-owner attempted to start game"
        );
        return Ok(()); // Silently ignore invalid request
    }

    // Get game info first to get player list
    let game_info_before = util::fetch_game_info(src, &state).await?;
    let players = game_info_before.players.clone();

    // Initialize SimpleGameSync when game starts
    util::with_game_mut(&state, src, |game_info| {
        game_info.game_status = GAME_STATUS_PLAYING; // Playing

        // Initialize CachedGameSync with player delays (derived from conn_type)
        let delays: Vec<usize> = game_info
            .players
            .iter()
            .map(|p| p.conn_type as usize)
            .collect();
        game_info.sync_manager = Some(CachedGameSync::new(delays));
    })
    .await?;

    // Update all players' status to NET_SYNC when game starts
    for player in &players {
        util::with_client_mut(&state, &player.addr, |client_info| {
            client_info.player_status = PLAYER_STATUS_NET_SYNC;
        })
        .await?;
    }

    let game_info = util::fetch_game_info(src, &state).await?;

    info!(
        { fields::GAME_ID } = game_id,
        { fields::PLAYER_COUNT } = game_info.players.len(),
        { fields::GAME_STATUS } = "playing",
        "Game started"
    );

    // Update game status
    let status_data = util::make_update_game_status(&game_info)?;
    util::broadcast_packet(&state, msg::UPDATE_GAME_STATUS, status_data).await?;

    // Broadcast server status update to all clients to reflect player status changes
    // This ensures that all clients see the updated player_status (NET_SYNC/PLAYING) in the server list
    // let all_client_addrs = state.get_all_client_addrs().await;
    // for client_addr in &all_client_addrs {
    //     if let Ok(data) = util::make_server_status(client_addr, &state).await {
    //         util::send_packet(&state, client_addr, msg::SERVER_STATUS, data).await?;
    //     }
    // }

    // Send start game notification with each player's delay
    for (i, player) in game_info.players.iter().enumerate() {
        let player_delay = player.conn_type as usize;
        let mut data = BytesMut::new();
        data.put_u8(0);
        data.put_u16_le(player_delay as u16); // Frame Delay (player's connection_type)
        debug!(
            player_number = i + 1,
            frame_delay = player_delay,
            { fields::ADDR } = %player.addr,
            "Sending start game notification"
        );
        data.put_u8((i + 1) as u8); // Player Number (1-indexed)
        data.put_u8(game_info.players.len() as u8); // Total Players
        util::send_packet(&state, &player.addr, msg::START_GAME, data.to_vec()).await?;
    }
    Ok(())
}
