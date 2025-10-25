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
   - When one player drops, ALL players receive drop notification
   - Each player receives their OWN username/number (appears as if they dropped themselves)
   - This forces all players to exit the game simultaneously

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
    let (was_playing, game_players) = {
        let mut games = state.games.write().await;
        let game_info = games.get_mut(&game_id).ok_or("Game not found")?;

        // Check if game is actually playing
        let was_playing = game_info.game_status == 1;
        if !was_playing {
            return Ok(false); // Not playing, nothing to drop
        }

        println!(
            "[DropGame] {} ending game {} - all players will be dropped (room stays open)",
            username, game_id
        );

        // End the game but keep the room
        game_info.game_status = 0; // Back to Waiting
        game_info.sync_manager = None; // Clear sync manager

        // Get all players for notification (including the one who dropped)
        let game_players: Vec<_> = game_info.players.iter().cloned().collect();

        (was_playing, game_players)
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

    // Send drop game notification to all players
    // Each player receives their own username and player number
    // so it appears as if they dropped the game themselves
    for (idx, player_addr) in game_players.iter().enumerate() {
        let player_client = state
            .get_client(player_addr)
            .await
            .ok_or("Player not found")?;
        let player_username = player_client.username.clone();
        let player_num = (idx + 1) as u8;

        let mut notification_data = BytesMut::new();
        notification_data.put(player_username.as_bytes());
        notification_data.put_u8(0); // Null terminator
        notification_data.put_u8(player_num);

        util::send_packet(state, player_addr, 0x14, notification_data.to_vec()).await?;

        println!(
            "[DropGame] Sent drop notification to {} (player {})",
            player_username, player_num
        );
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
