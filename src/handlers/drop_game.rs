use bytes::{BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::{debug, info};

use crate::kaillera::message_types as msg;
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

/// Execute drop game logic - marks player as dropped, game continues, and sends outputs
/// Returns true if game was playing, false otherwise
pub async fn execute_drop_game(
    game_id: u32,
    src: &std::net::SocketAddr,
    state: &Arc<AppState>,
) -> Result<bool, Box<dyn Error>> {
    // Get client info
    let client = state.get_client(src).await.ok_or("Client not found")?;
    let username = client.username.clone();

    // Mark player as dropped and get outputs
    let (was_playing, outputs) = {
        let mut games = state.games.write().await;
        let game_info = games.get_mut(&game_id).ok_or("Game not found")?;

        // Check if game is actually playing
        let was_playing = game_info.game_status == 1;

        let outputs = if was_playing {
            // Find dropper's player_id
            let dropper_player_id = game_info
                .player_addrs
                .iter()
                .position(|addr| addr == src)
                .ok_or("Dropper not found in game")?;

            // Mark player as dropped - other players will handle their empty inputs
            game_info.dropped_players[dropper_player_id] = true;

            let outputs = if let Some(sync_manager) = &mut game_info.sync_manager {
                let dropper_delay = sync_manager.get_player_delay(dropper_player_id);
                let empty_input = vec![0x00; dropper_delay * 2];
                let outputs = sync_manager.process_client_input(
                    dropper_player_id,
                    simple_game_sync::ClientInput::GameData(empty_input),
                );
                info!(
                    "Dropped player's input processed, dropper id: {}, outputs count: {}",
                    dropper_player_id,
                    outputs.len()
                );
                outputs
            } else {
                Vec::new()
            };

            info!(
                { fields::USER_NAME } = username.as_str(),
                { fields::GAME_ID } = game_id,
                { fields::PLAYER_ID } = dropper_player_id,
                "Player marked as dropped - game continues"
            );

            outputs
        } else {
            info!(
                { fields::USER_NAME } = username.as_str(),
                { fields::GAME_ID } = game_id,
                "Drop requested but game not playing"
            );
            Vec::new()
        };

        (was_playing, outputs)
    };

    // Send outputs to remaining players
    if !outputs.is_empty() {
        let game_info = state.get_game(game_id).await.ok_or("Game not found")?;

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
                simple_game_sync::ServerResponse::GameCache(position) => {
                    (msg::GAME_CACHE, vec![0x00, position])
                }
            };

            debug!(
                { fields::PLAYER_ID } = output.player_id,
                message_type = msg::message_type_name(message_type),
                { fields::DATA_LENGTH } = data_to_send.len(),
                "Sending output from dropped player's input to remaining player"
            );
            util::send_packet(state, target_addr, message_type, data_to_send).await?;
        }
    }

    // Update dropper's status to IDLE
    util::with_client_mut(state, src, |client_info| {
        client_info.player_status = PLAYER_STATUS_IDLE;
    })
    .await?;

    info!(
        { fields::DROPPER_USERNAME } = username.as_str(),
        { fields::GAME_ID } = game_id,
        "Player dropped - remaining players continue"
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
