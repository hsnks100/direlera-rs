use std::error::Error;

use bytes::{Buf, BufMut, BytesMut};
use tracing::debug;

use crate::{GameInfo, Message};

use super::data::{AppState, ClientInfo};

pub fn build_join_game_response(user: &ClientInfo) -> Vec<u8> {
    let mut data = Vec::new();
    data.put_u8(0); // Empty string [00]
    data.put_u32_le(0); // Pointer to Game on Server Side
    data.put(user.username.as_bytes());
    data.put_u8(0);
    data.put_u32_le(user.ping);
    data.put_u16_le(user.user_id);
    data.put_u8(user.conn_type);
    data
}

pub fn build_new_game_notification(
    username: &str,
    game_name: &str,
    emulator_name: &str,
    game_id: u32,
) -> Vec<u8> {
    let mut data = Vec::new();
    data.put(username.as_bytes());
    data.put_u8(0);
    data.put(game_name.as_bytes());
    data.put_u8(0);
    data.put(emulator_name.as_bytes());
    data.put_u8(0);
    data.put_u32_le(game_id);
    data
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
) -> Result<Vec<u8>, Box<dyn Error>> {
    // Prepare response data
    let mut data = BytesMut::new();
    data.put_u8(0); // Empty string [00]
    data.put_u32_le((game_info.players.len() - 1) as u32);

    debug!(
        PLAYER_COUNT = game_info.players.len(),
        "Building player information"
    );

    for player_addr in game_info.players.iter() {
        if player_addr != src {
            if let Some(client_info) = state.get_client(player_addr).await {
                data.put(client_info.username.as_bytes());
                data.put_u8(0); // NULL terminator
                data.put_u32_le(client_info.ping);
                data.put_u16_le(client_info.user_id);
                data.put_u8(client_info.conn_type);
            }
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
pub fn make_update_game_status(game_info: &GameInfo) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut data = BytesMut::new();
    data.put_u8(0); // Empty string [00]
    data.put_u32_le(game_info.game_id);
    data.put_u8(game_info.game_status);
    data.put_u8(game_info.num_players);
    data.put_u8(game_info.max_players);
    Ok(data.to_vec())
}

pub async fn make_user_joined(
    src: &std::net::SocketAddr,
    state: &AppState,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let client_info = state.get_client(src).await.ok_or("Client not found.")?;

    let mut data = BytesMut::new();
    data.put(client_info.username.as_bytes());
    data.put_u8(0);
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
) -> Result<(), Box<dyn Error>> {
    let response_packet = state
        .update_client::<_, Vec<u8>, Box<dyn Error>>(addr, |client| {
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
) -> Result<(), Box<dyn Error>> {
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

pub fn make_server_information() -> Result<Vec<u8>, Box<dyn Error>> {
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
) -> Result<Vec<u8>, Box<dyn Error>> {
    let addr_map = state.clients_by_addr.read().await;
    let id_map = state.clients_by_id.read().await;
    let games_lock = state.games.read().await;

    // Prepare response data
    let mut data = BytesMut::new();
    data.put_u8(0); // Empty string [00]

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
                data.put(client_info.username.as_bytes());
                data.put_u8(0); // NULL terminator
                data.put_u32_le(client_info.ping);
                data.put_u8(client_info.player_status);
                data.put_u16_le(client_info.user_id);
                data.put_u8(client_info.conn_type);
            }
        }
    }

    // Game list
    for game_info in games_lock.values() {
        data.put(game_info.game_name.as_bytes());
        data.put_u8(0); // NULL terminator
        data.put_u32_le(game_info.game_id);
        data.put(game_info.emulator_name.as_bytes());
        data.put_u8(0); // NULL terminator
        data.put(game_info.owner.as_bytes());
        data.put_u8(0); // NULL terminator
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
) -> Result<(String, String, u8, u16), String> {
    match state.get_client(src).await {
        Some(client_info) => Ok((
            client_info.username.clone(),
            client_info.emulator_name.clone(),
            client_info.conn_type,
            client_info.user_id,
        )),
        None => Err(format!("Client not found: addr={}", src)),
    }
}

pub async fn fetch_game_info(
    src: &std::net::SocketAddr,
    state: &AppState,
) -> Result<GameInfo, String> {
    // Retrieve game_id from client information
    let client_info = state
        .get_client(src)
        .await
        .ok_or_else(|| format!("Client not found: addr={}", src))?;

    let game_id = client_info
        .game_id
        .ok_or_else(|| format!("Game ID not found for client: addr={}", src))?;

    // Retrieve game information using game_id
    state
        .get_game(game_id)
        .await
        .ok_or_else(|| format!("Game not found: game_id={}", game_id))
}

pub async fn with_client_mut<F, R>(
    state: &AppState,
    src: &std::net::SocketAddr,
    f: F,
) -> Result<R, Box<dyn Error>>
where
    F: FnOnce(&mut ClientInfo) -> R,
{
    state
        .update_client::<_, R, Box<dyn Error>>(src, |client_info| Ok(f(client_info)))
        .await
}

pub async fn with_game_mut<F, R>(
    state: &AppState,
    src: &std::net::SocketAddr,
    f: F,
) -> Result<R, Box<dyn Error>>
where
    F: FnOnce(&mut GameInfo) -> R,
{
    // Retrieve game_id from client information
    let client_info = state.get_client(src).await.ok_or("Client not found")?;
    let game_id = client_info.game_id.ok_or("Game ID not found for client")?;

    // Update game information
    state
        .update_game::<_, R, Box<dyn Error>>(game_id, |game_info| Ok(f(game_info)))
        .await
}
