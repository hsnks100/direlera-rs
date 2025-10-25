use bytes::{Buf, BytesMut};
use std::error::Error;
use std::sync::Arc;

use crate::util::*;
use crate::*;

pub async fn handle_join_game(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    // Parse message
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = read_string(&mut buf);
    let game_id = buf.get_u32_le();
    let _ = read_string(&mut buf);
    let _ = buf.get_u32_le();
    let _ = buf.get_u16_le();
    let _conn_type = buf.get_u8();

    // Get joining player's connection type
    let client = state.get_client(src).await.ok_or("Client not found")?;
    let conn_type = client.conn_type;

    util::with_client_mut(&state, src, |client_info| {
        client_info.game_id = Some(game_id);
    })
    .await?;

    util::with_game_mut(&state, src, |game_info| {
        game_info.players.insert(*src);
        game_info.num_players += 1;
        game_info.player_addrs.push(*src);
        game_info.player_delays.push(conn_type as usize); // Use player's connection_type as delay
    })
    .await?;

    // Generate game status data
    let game_info = state.get_game(game_id).await.ok_or("Game not found")?;
    let status_data = util::make_update_game_status(&game_info)?;

    println!("Game status updated for game_id={}", game_id);

    // Broadcast game status update to all clients
    let client_addresses = state.get_all_client_addrs().await;
    for addr in client_addresses {
        util::send_packet(&state, &addr, 0x0E, status_data.clone()).await?;
    }

    // Generate player information and send to joining client
    let players_info = util::make_player_information(src, &state, &game_info).await?;
    util::send_packet(&state, src, 0x0D, players_info.clone()).await?;

    // Generate join game response data
    let response_data = {
        let client_info = state.get_client(src).await.ok_or("Client not found")?;
        util::build_join_game_response(&client_info)
    };

    // Send join game response to existing players
    let game_players = game_info.players.clone();
    for player_addr in game_players {
        if &player_addr != src {
            util::send_packet(&state, &player_addr, 0x0C, response_data.clone()).await?;
        }
    }

    Ok(())
}
