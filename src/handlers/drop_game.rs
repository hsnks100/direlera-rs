use bytes::{BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;

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
2. Server: Update Game Status [0x0E] (game_status = 0: Waiting)
3. Server: Drop Game Notification [0x14] (to all players in the room)

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
    let (was_playing, player_number, game_players) = {
        let mut games = state.games.write().await;
        let game_info = games.get_mut(&game_id).ok_or("Game not found")?;

        // Check if game is actually playing
        let was_playing = game_info.game_status == 1;
        if !was_playing {
            return Ok(false); // Not playing, nothing to drop
        }

        // Find player number (1-indexed)
        let player_number = game_info
            .player_addrs
            .iter()
            .position(|addr| addr == src)
            .map(|idx| (idx + 1) as u8)
            .ok_or("Player not in game")?;

        println!(
            "[DropGame] Player {} ({}) ending game {} (room stays open)",
            player_number, username, game_id
        );

        // End the game but keep the room
        game_info.game_status = 0; // Back to Waiting
        game_info.sync_manager = None; // Clear sync manager

        // Get all players for notification (including the one who dropped)
        let game_players: Vec<_> = game_info.players.iter().cloned().collect();

        (was_playing, player_number, game_players)
    };

    // Update all players' status back to IDLE (waiting in room)
    for player_addr in &game_players {
        util::with_client_mut(state, player_addr, |client_info| {
            client_info.player_status = PLAYER_STATUS_IDLE;
        })
        .await?;
    }

    // Update game status for all clients
    let game_info = state.get_game(game_id).await.ok_or("Game not found")?;
    let status_data = util::make_update_game_status(&game_info)?;
    util::broadcast_packet(state, 0x0E, status_data).await?;

    // Send drop game notification to all players (including sender)
    let mut notification_data = BytesMut::new();
    notification_data.put(username.as_bytes());
    notification_data.put_u8(0); // Null terminator
    notification_data.put_u8(player_number);

    for player_addr in &game_players {
        util::send_packet(state, player_addr, 0x14, notification_data.clone().to_vec()).await?;
    }

    println!(
        "[DropGame] Game ended, room remains with {} players",
        game_players.len()
    );

    Ok(was_playing)
}

pub async fn handle_drop_game(
    _message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    println!("[0x14] Drop Game from {:?}", src);

    // Get game_id
    let client = state.get_client(src).await.ok_or("Client not found")?;
    let game_id = client.game_id.ok_or("Client not in a game")?;

    execute_drop_game(game_id, src, &state).await?;

    Ok(())
}
