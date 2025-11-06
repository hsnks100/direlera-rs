use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::debug;

use crate::kaillera::message_types as msg;
use crate::simplest_game_sync;
use crate::*;

pub async fn handle_game_cache(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    debug!("Game cache received");
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = buf.get_u8(); // Empty String
    let cache_position = buf.get_u8();

    let client = state.get_client(src).await.ok_or("Client not found")?;
    let game_id = client.game_id.ok_or("Game ID not found")?;

    // Find player_id from address
    let game_info = state.get_game(game_id).await.ok_or("Game not found")?;
    let player_id = game_info
        .players
        .iter()
        .position(|p| p.addr == *src)
        .ok_or("Player not in game")?;

    // Process with SimpleGameSync
    let outputs = {
        let mut games = state.games.write().await;
        let game_info = games.get_mut(&game_id).ok_or("Game not found")?;

        let sync_manager = game_info
            .sync_manager
            .as_mut()
            .ok_or("SimpleGameSync not initialized")?;

        // Process input using CachedGameSync
        sync_manager
            .process_client_input(
                player_id,
                simplest_game_sync::ClientInput::GameCache(cache_position),
            )
            .map_err(|e| format!("Game sync error: {}", e))?
    };

    // Send outputs to respective players
    let game_info = state.get_game(game_id).await.ok_or("Game not found")?;
    for output in outputs {
        let target_addr = &game_info.players[output.player_id].addr;

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

        debug!(
            { fields::PLAYER_ID } = output.player_id,
            message_type = msg::message_type_name(message_type),
            { fields::DATA_LENGTH } = data_to_send.len(),
            "Sending cache data to player"
        );
        util::send_packet(&state, target_addr, message_type, data_to_send).await?;
    }

    Ok(())
}
