use bytes::{BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;

use crate::*;
/*

'Quit Game State
'Client: Quit Game Request [0x0B]
'Server: Update Game Status [0x0E]
'Server: Quit Game Notification [0x0B]
'
'Close Game State
'Client: Quit Game Request [0x0B]
'Server: Close Game Notification [0x10]
'Server: Quit Game Notification [0x0B]
'     0x0B = Quit Game
'            Client Request:
'            NB : Empty String [00]
'            2B : 0xFF
'
'            Server Notification:
'            NB : Username
'            2B : UserID

'     0x10 = Close game
'            Server Notification:
'            NB : Empty String [00]
'            4B : GameID
 */
pub async fn handle_quit_game(
    _message: Vec<u8>,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    // Get game_id first and release client lock before acquiring game lock
    let (username, user_id, game_id) = {
        let client_info = match state.get_client(src).await {
            Some(client_info) => client_info,
            None => {
                eprintln!("Client not found during game quit: addr={}", src);
                return Ok(());
            }
        };
        let game_id = match client_info.game_id {
            Some(game_id) => game_id,
            None => {
                eprintln!("Game ID not found during game quit: addr={}", src);
                return Ok(());
            }
        };
        (client_info.username.clone(), client_info.user_id, game_id)
    };

    // If game is in playing state, drop game first for all players
    println!(
        "[QuitGame] Checking if game {} is in playing state",
        game_id
    );
    drop_game::execute_drop_game(game_id, src, &state).await?;

    // Update game info
    let game_info_clone = {
        let mut games_lock = state.games.write().await;
        let game_info = match games_lock.get_mut(&game_id) {
            Some(game_info) => game_info,
            None => {
                eprintln!("Game not found during game quit: game_id={}", game_id);
                return Ok(());
            }
        };
        game_info.players.remove(src);
        game_info.num_players -= 1;

        // Remove from player_addrs and player_delays
        if let Some(idx) = game_info.player_addrs.iter().position(|addr| addr == src) {
            game_info.player_addrs.remove(idx);
            game_info.player_delays.remove(idx);
        }

        game_info.clone()
    };

    // Remove client from game
    util::with_client_mut(&state, src, |client_info| {
        client_info.game_id = None;
        client_info.player_status = PLAYER_STATUS_IDLE;
    })
    .await?;

    if game_info_clone.owner == username {
        // Close the game - Remove game from games list
        state.remove_game(game_info_clone.game_id).await;

        // Update remaining players' status
        for player_addr in game_info_clone.players.iter() {
            util::with_client_mut(&state, player_addr, |client_info| {
                client_info.game_id = None;
                client_info.player_status = PLAYER_STATUS_IDLE;
            })
            .await?;
        }

        // Make close game notification
        let mut data = BytesMut::new();
        data.put_u8(0x00);
        data.put_u32_le(game_info_clone.game_id);
        util::broadcast_packet(&state, 0x10, data.to_vec()).await?;

        // Quit game notification
        for player_addr in game_info_clone.players.iter() {
            let mut data = BytesMut::new();
            data.put(username.as_bytes());
            data.put_u8(0);
            data.put_u16_le(user_id);
            util::send_packet(&state, player_addr, 0x0B, data.to_vec()).await?;
        }
    } else {
        // Update game status
        let status_data = util::make_update_game_status(&game_info_clone)?;
        util::broadcast_packet(&state, 0x0E, status_data).await?;

        for player_addr in game_info_clone.players.iter() {
            let mut data = BytesMut::new();
            data.put(username.as_bytes());
            data.put_u8(0);
            data.put_u16_le(user_id);
            util::send_packet(&state, player_addr, 0x0B, data.to_vec()).await?;
        }
    }
    Ok(())
}
