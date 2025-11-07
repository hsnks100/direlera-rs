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

    // Validate game is playing and get dropper info
    let dropper_player_id = {
        let games = state.games.read().await;
        let game_info = games.get(&game_id).ok_or("Game not found")?;

        // Check if game is actually playing
        if game_info.game_status != GAME_STATUS_PLAYING {
            info!("Game is not playing, skipping drop game");
            return Ok(false);
        }

        // Find dropper's player_id
        game_info
            .players
            .iter()
            .position(|p| p.addr == *src)
            .ok_or("Dropper not found in game")?
    };

    // Mark player as dropped and get all necessary data (in one lock)
    let (outputs, players, _all_dropped) = {
        let mut games = state.games.write().await;
        let game_info = games.get_mut(&game_id).ok_or("Game not found")?;

        info!(
            { fields::USER_NAME } = username.as_str(),
            { fields::GAME_ID } = game_id,
            "Ending game",
        );

        // Mark player as dropped and get pending outputs
        let outputs = game_info
            .sync_manager
            .as_mut()
            .ok_or("Sync manager not found")?
            .mark_player_dropped(dropper_player_id)
            .map_err(|e| format!("Failed to mark player dropped: {}", e))?;

        info!("Marked player {} as dropped", dropper_player_id);

        let all_dropped = game_info
            .sync_manager
            .as_ref()
            .ok_or("Sync manager not found")?
            .sync
            .all_players_dropped();

        if all_dropped {
            game_info.game_status = GAME_STATUS_WAITING;
            let status_data = util::make_update_game_status(game_info)?;
            util::broadcast_packet(state, msg::UPDATE_GAME_STATUS, status_data).await?;
            game_info.sync_manager = None;
        }

        // Clone data before releasing the lock
        let players = game_info.players.clone();

        (outputs, players, all_dropped)
    };

    // Update all players' status back to IDLE (waiting in room)
    for player in &players {
        util::with_client_mut(state, &player.addr, |client_info| {
            client_info.player_status = PLAYER_STATUS_IDLE;
        })
        .await?;
    }

    let dropper_player_num = (dropper_player_id + 1) as u8;

    let mut notification_data = BytesMut::new();
    notification_data.put(username.as_bytes());
    notification_data.put_u8(0); // Null terminator
    notification_data.put_u8(dropper_player_num);

    for player in &players {
        util::send_packet(
            state,
            &player.addr,
            msg::DROP_GAME,
            notification_data.to_vec(),
        )
        .await?;
    }

    // info!(
    //     { fields::DROPPER_USERNAME } = username.as_str(),
    //     { fields::PLAYER_NUMBER } = dropper_player_num,
    //     { fields::PLAYER_COUNT } = game_players.len(),
    //     "Sent drop notification to all players"
    // );

    // Send any outputs that can now be sent due to the drop
    for output in outputs {
        let target_addr = &players[output.player_id].addr;

        let (message_type, data_to_send) = match output.response {
            simplest_game_sync::ServerResponse::GameData(data) => {
                let mut buf = BytesMut::new();
                buf.put_u8(0); // Empty string
                buf.put_u16_le(data.len() as u16);
                buf.put(data.as_slice());
                (msg::GAME_DATA, buf.to_vec())
            }
            simplest_game_sync::ServerResponse::GameCache(position) => {
                (msg::GAME_CACHE, vec![0x00, position])
            }
        };

        info!(
            { fields::PLAYER_ID } = output.player_id,
            message_type = msg::message_type_name(message_type),
            "Sending game data/cache after drop to player"
        );
        util::send_packet(state, target_addr, message_type, data_to_send).await?;
    }

    info!(
        { fields::GAME_ID } = game_id,
        { fields::PLAYER_COUNT } = players.len(),
        "Game ended, room remains open"
    );

    Ok(true)
}

pub async fn handle_drop_game(
    _message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    debug!("Drop game request received");

    let client = state.get_client(src).await.ok_or("Client not found")?;
    let game_id = client.game_id.ok_or("Client not in a game")?;

    execute_drop_game(game_id, src, &state).await?;

    Ok(())
}
