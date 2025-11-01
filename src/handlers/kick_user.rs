use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::kaillera::message_types as msg;
use crate::*;
/*
 **Client to Server**:
  - Empty String
  - `2B`: UserID
*/
pub async fn handle_kick_user(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = util::read_string(&mut buf); // Empty String
    let user_id = buf.get_u16_le(); // UserID

    // Check if requester is the game owner (using user_id to prevent nickname abuse)
    let requester_info = state.get_client(src).await.ok_or("Requester not found")?;
    let requester_username = requester_info.username.clone();
    let requester_user_id = requester_info.user_id;
    let requester_game_id = requester_info.game_id.ok_or("Requester not in a game")?;

    let game_info = state
        .get_game(requester_game_id)
        .await
        .ok_or("Game not found")?;
    if game_info.owner_user_id != requester_user_id {
        warn!(
            { fields::USER_NAME } = requester_username.as_str(),
            { fields::USER_ID } = requester_user_id,
            { fields::GAME_ID } = requester_game_id,
            owner_user_id = game_info.owner_user_id,
            "Non-owner attempted to kick user"
        );
        return Ok(()); // Silently ignore invalid request
    }

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
                error!(
                    { fields::KICKED_USER_ID } = user_id,
                    "Client not found during kick user"
                );
                return Ok(());
            }
        }
    };

    // Verify the kicked user is in the same game as requester
    let game_id = {
        let client_info = state.get_client(&client_addr).await;
        match client_info {
            Some(client_info) => match client_info.game_id {
                Some(game_id) => {
                    if game_id != requester_game_id {
                        warn!(
                            { fields::USER_NAME } = requester_username.as_str(),
                            { fields::GAME_ID } = requester_game_id,
                            kicked_user_game_id = game_id,
                            "Attempted to kick user from different game"
                        );
                        return Ok(());
                    }
                    game_id
                }
                None => {
                    error!(
                        { fields::USER_ID } = client_user_id,
                        "Game ID not found during kick user"
                    );
                    return Ok(());
                }
            },
            None => {
                error!(
                    { fields::KICKED_USER_ID } = user_id,
                    "Client not found during kick user"
                );
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
                error!(
                    { fields::GAME_ID } = game_id,
                    "Game not found during kick user"
                );
                return Ok(());
            }
        }
    };

    info!(
        { fields::USER_NAME } = username.as_str(),
        { fields::USER_ID } = client_user_id,
        { fields::GAME_ID } = game_id,
        "User kicked from game"
    );

    // Update game status
    let status_data = util::make_update_game_status(&game_info_clone)?;
    util::broadcast_packet(&state, msg::UPDATE_GAME_STATUS, status_data).await?;

    for player_addr in game_info_clone.players.iter() {
        let mut data = BytesMut::new();
        data.put(username.as_bytes());
        data.put_u8(0);
        data.put_u16_le(client_user_id);
        util::send_packet(&state, player_addr, msg::QUIT_GAME, data.to_vec()).await?;
    }

    Ok(())
}
