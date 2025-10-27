use bytes::{BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::{debug, info};

use crate::*;

/*
0x14 = Drop Game

This ends the game session but keeps the room open.
Players remain in the room and can start a new game.

Client Request:
- NB : Empty String [00]
- 1B : 0x00

Server Notification:
- NB : Username (who dropped the game)
- 1B : Player Number (which player number dropped)

Flow:
1. Client: Drop Game Request [0x14]
2. Server: Drop Game Notification [0x14] (to all players in the room)
   - All players receive the username and player number of who dropped
3. Server: Update Game Status [0x0E] (game_status = 0: Waiting)

Note: This is different from Quit Game (0x0B) which removes players from the room.
*/

/// Execute drop game logic - ends the game but keeps the room open
/// Returns true if game was actually dropped, false if game was not in playing state
pub async fn execute_drop_game(
    game_id: u32,
    src: &std::net::SocketAddr,
    state: &Arc<AppState>,
) -> Result<bool, Box<dyn Error>> {
    // Get client info
    let client = state.get_client(src).await.ok_or("Client not found")?;
    let username = client.username.clone();

    // End the game (not the room!)
    let (was_playing, game_players, dropper_player_id, pending_outputs) = {
        let mut games = state.games.write().await;
        let game_info = games.get_mut(&game_id).ok_or("Game not found")?;

        // Check if game is actually playing
        let was_playing = game_info.game_status == 1;

        info!(
            { fields::USER_NAME } = username.as_str(),
            { fields::GAME_ID } = game_id,
            { fields::WAS_PLAYING } = was_playing,
            "Ending game - all players will be dropped (room stays open)"
        );

        // Find dropper's player_id
        let dropper_player_id = game_info
            .player_addrs
            .iter()
            .position(|addr| addr == src)
            .ok_or("Dropper not found in game")?;

        // Fill dropper's future inputs with empty data to unblock other players
        // This allows other players to receive responses for their pending inputs
        let mut pending_outputs = Vec::new();
        if was_playing {
            if let Some(sync_manager) = &mut game_info.sync_manager {
                let player_delay = sync_manager.get_player_delay(dropper_player_id);
                // Fill enough frames to handle any pending inputs from other players
                let frames_to_fill = player_delay * 10; // Conservative estimate

                for _ in 0..frames_to_fill {
                    let outputs = sync_manager.process_client_input(
                        dropper_player_id,
                        crate::simple_game_sync::ClientInput::GameData(vec![0x00, 0x00]),
                    );
                    pending_outputs.extend(outputs);
                }

                debug!(
                    { fields::PLAYER_ID } = dropper_player_id,
                    frames_filled = frames_to_fill,
                    pending_outputs_count = pending_outputs.len(),
                    "Filled empty frames for dropped player"
                );
            }

            // End the game but keep the room
            game_info.game_status = 0; // Back to Waiting
                                       // Note: Keep sync_manager alive to handle any in-flight GC/GD packets
                                       // Clients will stop sending after receiving drop notification
        } else {
            info!(
                { fields::USER_NAME } = username.as_str(),
                { fields::GAME_ID } = game_id,
                "Drop requested but game already ended - sending confirmation"
            );
        }

        // Get all players for notification (including the one who dropped)
        let game_players: Vec<_> = game_info.players.iter().cloned().collect();
        let player_addrs = game_info.player_addrs.clone();

        (
            was_playing,
            game_players,
            dropper_player_id,
            (pending_outputs, player_addrs),
        )
    };

    // Update all players' status back to IDLE (waiting in room)
    for player_addr in &game_players {
        util::with_client_mut(state, player_addr, |client_info| {
            client_info.player_status = PLAYER_STATUS_IDLE;
        })
        .await?;
    }

    // Send pending outputs to unblock waiting players FIRST
    let (pending_outputs, player_addrs) = pending_outputs;
    for output in pending_outputs {
        let target_addr = &player_addrs[output.player_id];

        let (message_type, data_to_send) = match output.response {
            crate::simple_game_sync::ServerResponse::GameData(data) => {
                let mut buf = BytesMut::new();
                buf.put_u8(0); // Null byte
                buf.extend_from_slice(&data);
                (0x12, buf.to_vec())
            }
            crate::simple_game_sync::ServerResponse::GameCache(position) => {
                let mut buf = BytesMut::new();
                buf.put_u8(0); // Null byte
                buf.put_u8(position);
                (0x13, buf.to_vec())
            }
        };

        util::send_packet(state, target_addr, message_type, data_to_send).await?;
    }

    // Send drop game notification to all players
    // All players receive the username of who dropped the game
    let dropper_player_num = (dropper_player_id + 1) as u8;

    let mut notification_data = BytesMut::new();
    notification_data.put(username.as_bytes());
    notification_data.put_u8(0); // Null terminator
    notification_data.put_u8(dropper_player_num);

    for player_addr in &game_players {
        util::send_packet(state, player_addr, 0x14, notification_data.to_vec()).await?;
    }

    info!(
        { fields::DROPPER_USERNAME } = username.as_str(),
        { fields::PLAYER_NUMBER } = dropper_player_num,
        { fields::PLAYER_COUNT } = game_players.len(),
        "Sent drop notification to all players"
    );

    // Update game status for all clients AFTER drop notification
    let game_info = state.get_game(game_id).await.ok_or("Game not found")?;
    let status_data = util::make_update_game_status(&game_info)?;
    util::broadcast_packet(state, 0x0E, status_data).await?;

    info!(
        { fields::GAME_ID } = game_id,
        { fields::PLAYER_COUNT } = game_players.len(),
        "Game ended, room remains open"
    );

    Ok(was_playing)
}

pub async fn handle_drop_game(
    _message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    debug!("Drop game request received");

    // Get game_id
    let client = state.get_client(src).await.ok_or("Client not found")?;
    let game_id = client.game_id.ok_or("Client not in a game")?;

    execute_drop_game(game_id, src, &state).await?;

    Ok(())
}
