use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;

use crate::*;
/*
 **Client to Server**:
  - Empty String
  - `2B`: UserID
*/
pub async fn handle_kick_user(
    message: kaillera::protocol::ParsedMessage,
    _src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = util::read_string(&mut buf); // Empty String
    let user_id = buf.get_u16_le(); // UserID

    let (username, client_user_id, client_addr) = {
        let addr_map = state.clients_by_addr.read().await;
        let id_map = state.clients_by_id.read().await;

        let client_info = addr_map.iter().find_map(|(addr, session_id)| {
            let client = id_map.get(session_id)?;
            if client.user_id == user_id {
                Some((addr, client))
            } else {
                None
            }
        });

        match client_info {
            Some((addr, client_info)) => (client_info.username.clone(), client_info.user_id, *addr),
            None => {
                eprintln!("Client not found during kick user: user_id={}", user_id);
                return Ok(());
            }
        }
    };

    let game_id = {
        let client_info = state.get_client(&client_addr).await;
        match client_info {
            Some(client_info) => match client_info.game_id {
                Some(game_id) => game_id,
                None => {
                    eprintln!(
                        "Game ID not found during kick user: user_id={}",
                        client_user_id
                    );
                    return Ok(());
                }
            },
            None => {
                eprintln!("Client not found during kick user: user_id={}", user_id);
                return Ok(());
            }
        }
    };

    let game_info_clone = {
        let mut games_lock = state.games.write().await;
        let game_info = games_lock.get_mut(&game_id);
        match game_info {
            Some(game_info) => {
                game_info.players.remove(&client_addr);
                game_info.num_players -= 1;
                game_info.clone()
            }
            None => {
                eprintln!("Game not found during kick user: game_id={}", game_id,);
                return Ok(());
            }
        }
    };

    // Update game status
    let status_data = util::make_update_game_status(&game_info_clone)?;
    util::broadcast_packet(&state, 0x0E, status_data).await?;

    for player_addr in game_info_clone.players.iter() {
        let mut data = BytesMut::new();
        data.put(username.as_bytes());
        data.put_u8(0);
        data.put_u16_le(client_user_id);
        util::send_packet(&state, player_addr, 0x0B, data.to_vec()).await?;
    }

    Ok(())
}
