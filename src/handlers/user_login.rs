use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;
use util::read_string;
use uuid::Uuid;

use crate::kaillera::message_types as msg;
use crate::*;

pub async fn handle_user_login(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    let mut buf = BytesMut::from(&message.data[..]);

    // NB: Username
    let username = read_string(&mut buf);
    // NB: Emulator Name
    let emulator_name = read_string(&mut buf);
    // 1B: Connection Type
    let conn_type = if !buf.is_empty() { buf.get_u8() } else { 0 };

    // Lock-free ID generation
    let user_id = state.next_user_id();

    info!(
        { fields::USER_NAME } = username.as_str(),
        { fields::USER_ID } = user_id,
        emulator = emulator_name.as_str(),
        { fields::CONNECTION_TYPE } = conn_type,
        "User logged in"
    );

    let client = ClientInfo {
        session_id: Uuid::new_v4(),
        username: username.clone(),
        emulator_name: emulator_name.clone(),
        conn_type,
        user_id,
        ping: 0,
        player_status: PLAYER_STATUS_IDLE,
        game_id: None,
        last_ping_time: Some(Instant::now()),
        ack_count: 0,
        packet_generator: kaillera::protocol::UDPPacketGenerator::new(),
    };

    // Encapsulated method
    state.add_client(*src, client).await;

    // Prepare response data
    let mut data = BytesMut::new();
    data.put_u8(0); // Empty string [00]
    data.put_u32_le(0);
    data.put_u32_le(1);
    data.put_u32_le(2);
    data.put_u32_le(3);

    // Send response
    util::send_packet(&state, src, msg::SERVER_TO_CLIENT_ACK, data.to_vec()).await?;

    Ok(())
}
