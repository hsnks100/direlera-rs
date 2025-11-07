use bytes::{Buf, BufMut, BytesMut};
use color_eyre::eyre::eyre;
use tracing::debug;

use crate::{packet_util, state, Message};

use state::{AppState, ClientInfo, GameInfo};

pub fn build_join_game_response(user: &ClientInfo) -> Vec<u8> {
    let mut data = BytesMut::new();
    packet_util::put_empty_string(&mut data);
    data.put_u32_le(0); // Pointer to Game on Server Side
    packet_util::put_string_with_null(&mut data, &user.username);
    data.put_u32_le(user.ping);
    data.put_u16_le(user.user_id);
    data.put_u8(user.conn_type);
    data.to_vec()
}

pub fn build_new_game_notification(
    username: &str,
    game_name: &str,
    emulator_name: &str,
    game_id: u32,
) -> Vec<u8> {
    let mut data = BytesMut::new();
    packet_util::put_strings_with_null(&mut data, &[username, game_name, emulator_name]);
    data.put_u32_le(game_id);
    data.to_vec()
}

// '     0x0D = Player Information
// '            Server Notification:
// '            NB : Empty String [00]
// '            4B : Number of Users in Room [not including you]
// '            NB : Username
// '            4B : Ping
// '            2B : UserID
// '            1B : Connection Type (6=Bad, 5=Low, 4=Average, 3=Good, 2=Excellent, & 1=LAN)
pub async fn make_player_information(
    src: &std::net::SocketAddr,
    state: &AppState,
    game_info: &GameInfo,
) -> color_eyre::Result<Vec<u8>> {
    // Prepare response data
    let mut data = BytesMut::new();
    packet_util::put_empty_string(&mut data);
    data.put_u32_le((game_info.players.len() - 1) as u32);

    debug!(
        PLAYER_COUNT = game_info.players.len(),
        "Building player information"
    );

    for player in &game_info.players {
        if player.addr != *src {
            // Get current ping from ClientInfo (it can change during game)
            let ping = state
                .get_client(&player.addr)
                .await
                .map(|c| c.ping)
                .unwrap_or(0);
            packet_util::put_string_with_null(&mut data, &player.username);
            data.put_u32_le(ping);
            data.put_u16_le(player.user_id);
            data.put_u8(player.conn_type);
        }
    }
    Ok(data.to_vec())
}

// '     0x0E = Update Game Status
// '            Server Notification:
// '            NB : Empty String [00]
// '            4B : GameID
// '            1B : Game Status (0=Waiting, 1=Playing, 2=Netsync)
// '            1B : Number of Players in Room
// '            1B : Maximum Players
pub fn make_update_game_status(game_info: &GameInfo) -> color_eyre::Result<Vec<u8>> {
    let mut data = BytesMut::new();
    packet_util::put_empty_string(&mut data);
    data.put_u32_le(game_info.game_id);
    data.put_u8(game_info.game_status);
    data.put_u8(game_info.num_players);
    data.put_u8(game_info.max_players);
    Ok(data.to_vec())
}

pub async fn make_user_joined(
    src: &std::net::SocketAddr,
    state: &AppState,
) -> color_eyre::Result<Vec<u8>> {
    let client_info = state
        .get_client(src)
        .await
        .ok_or_else(|| eyre!("Client not found"))?;

    let mut data = BytesMut::new();
    packet_util::put_string_with_null(&mut data, &client_info.username);
    data.put_u16_le(client_info.user_id);
    data.put_u32_le(client_info.ping);
    data.put_u8(client_info.conn_type);
    Ok(data.to_vec())
}

// Helper functions
pub async fn send_packet(
    state: &AppState,
    addr: &std::net::SocketAddr,
    packet_type: u8,
    data: Vec<u8>,
) -> color_eyre::Result<()> {
    let response_packet = state
        .update_client::<_, Vec<u8>, color_eyre::Report>(addr, |client| {
            Ok(client.packet_generator.make_send_packet(packet_type, data))
        })
        .await?;

    state
        .tx
        .send(Message {
            data: response_packet,
            addr: *addr,
        })
        .await?;
    Ok(())
}

pub async fn broadcast_packet(
    state: &AppState,
    packet_type: u8,
    data: Vec<u8>,
) -> color_eyre::Result<()> {
    // Get all client addresses first
    let client_addrs = state.get_all_client_addrs().await;

    // Generate packets for all clients
    let packets: Vec<_> = {
        let addr_map = state.clients_by_addr.read().await;
        let mut id_map = state.clients_by_id.write().await;

        client_addrs
            .iter()
            .filter_map(|addr| {
                let session_id = addr_map.get(addr)?;
                let client = id_map.get_mut(session_id)?;
                let packet = client
                    .packet_generator
                    .make_send_packet(packet_type, data.clone());
                Some((*addr, packet))
            })
            .collect()
    };

    // Send packets without holding the lock
    for (addr, packet) in packets {
        state.tx.send(Message { data: packet, addr }).await?;
    }
    Ok(())
}

/// Broadcasts a packet to all players in a specific game
pub async fn broadcast_packet_to_game(
    state: &AppState,
    game_id: u32,
    packet_type: u8,
    data: Vec<u8>,
) -> color_eyre::Result<()> {
    // Get game info to get player list
    let game_info = state
        .get_game(game_id)
        .await
        .ok_or_else(|| eyre!("Game not found"))?;

    // Generate packets for all players in the game
    let packets: Vec<_> = {
        let addr_map = state.clients_by_addr.read().await;
        let mut id_map = state.clients_by_id.write().await;

        game_info
            .players
            .iter()
            .filter_map(|player| {
                let session_id = addr_map.get(&player.addr)?;
                let client = id_map.get_mut(session_id)?;
                // Verify client is still in the same game
                if client.game_id != Some(game_id) {
                    return None;
                }
                let packet = client
                    .packet_generator
                    .make_send_packet(packet_type, data.clone());
                Some((player.addr, packet))
            })
            .collect()
    };

    // Send packets without holding the lock
    for (addr, packet) in packets {
        state.tx.send(Message { data: packet, addr }).await?;
    }
    Ok(())
}

pub fn read_string(buf: &mut BytesMut) -> String {
    let mut s = Vec::new();
    while let Some(&b) = buf.first() {
        buf.advance(1);
        if b == 0 {
            break;
        }
        s.push(b);
    }
    String::from_utf8_lossy(&s).to_string()
}

pub fn make_server_information() -> color_eyre::Result<Vec<u8>> {
    // Prepare response data
    // '            NB : "Server\0"
    // '            NB : Message
    let mut data = BytesMut::new();
    data.put("Server\0".as_bytes());
    data.put("Welcome to the Kaillera server!\0".as_bytes());
    Ok(data.to_vec())
}
pub async fn make_server_status(
    src: &std::net::SocketAddr,
    state: &AppState,
) -> color_eyre::Result<Vec<u8>> {
    let addr_map = state.clients_by_addr.read().await;
    let id_map = state.clients_by_id.read().await;
    let games_lock = state.games.read().await;

    // Prepare response data
    let mut data = BytesMut::new();
    packet_util::put_empty_string(&mut data);

    // Number of users (excluding self)
    let num_users = (addr_map.len() - 1) as u32;
    data.put_u32_le(num_users);

    // Number of games
    let num_games = games_lock.len() as u32;
    data.put_u32_le(num_games);

    // User list
    for (addr, session_id) in addr_map.iter() {
        if addr != src {
            if let Some(client_info) = id_map.get(session_id) {
                packet_util::put_string_with_null(&mut data, &client_info.username);
                data.put_u32_le(client_info.ping);
                data.put_u8(client_info.player_status);
                data.put_u16_le(client_info.user_id);
                data.put_u8(client_info.conn_type);
            }
        }
    }

    // Game list
    for game_info in games_lock.values() {
        packet_util::put_string_with_null(&mut data, &game_info.game_name);
        data.put_u32_le(game_info.game_id);
        packet_util::put_string_with_null(&mut data, &game_info.emulator_name);
        packet_util::put_string_with_null(&mut data, &game_info.owner);
        data.put(format!("{}/{}\0", game_info.num_players, game_info.max_players).as_bytes());
        data.put_u8(game_status_to_byte(game_info.game_status));
    }
    Ok(data.to_vec())
}

fn game_status_to_byte(status: u8) -> u8 {
    match status {
        0 => 0, // Waiting
        1 => 1, // Playing
        2 => 2, // Netsync
        _ => 0, // Default to Waiting
    }
}

pub async fn fetch_client_info(
    src: &std::net::SocketAddr,
    state: &AppState,
) -> color_eyre::Result<(String, String, u8, u16)> {
    match state.get_client(src).await {
        Some(client_info) => Ok((
            client_info.username.clone(),
            client_info.emulator_name.clone(),
            client_info.conn_type,
            client_info.user_id,
        )),
        None => Err(eyre!("Client not found: addr={}", src)),
    }
}

pub async fn fetch_game_info(
    src: &std::net::SocketAddr,
    state: &AppState,
) -> color_eyre::Result<GameInfo> {
    // Retrieve game_id from client information
    let client_info = state
        .get_client(src)
        .await
        .ok_or_else(|| eyre!("Client not found: addr={}", src))?;

    let game_id = client_info
        .game_id
        .ok_or_else(|| eyre!("Game ID not found for client: addr={}", src))?;

    // Retrieve game information using game_id
    state
        .get_game(game_id)
        .await
        .ok_or_else(|| eyre!("Game not found: game_id={}", game_id))
}

pub async fn with_client_mut<F, R>(
    state: &AppState,
    src: &std::net::SocketAddr,
    f: F,
) -> color_eyre::Result<R>
where
    F: FnOnce(&mut ClientInfo) -> R,
{
    state
        .update_client::<_, R, color_eyre::Report>(src, |client_info| Ok(f(client_info)))
        .await
}

pub async fn with_game_mut<F, R>(
    state: &AppState,
    src: &std::net::SocketAddr,
    f: F,
) -> color_eyre::Result<R>
where
    F: FnOnce(&mut GameInfo) -> R,
{
    // Retrieve game_id from client information
    let client_info = state
        .get_client(src)
        .await
        .ok_or_else(|| eyre!("Client not found"))?;
    let game_id = client_info
        .game_id
        .ok_or_else(|| eyre!("Game ID not found for client"))?;

    // Update game information
    state
        .update_game::<_, R, color_eyre::Report>(game_id, |game_info| Ok(f(game_info)))
        .await
}
