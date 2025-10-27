use crate::*;
use bytes::{BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::info;

pub async fn handle_global_chat(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    let mut buf = BytesMut::from(&message.data[..]);

    // NB: Empty String
    let _empty = util::read_string(&mut buf);
    // NB: Message
    let chat_message = util::read_string(&mut buf);

    // Get username from clients list
    let username = if let Some(client_info) = state.get_client(src).await {
        client_info.username.clone()
    } else {
        "Unknown".to_string()
    };

    info!(
        { fields::USER_NAME } = username.as_str(),
        { fields::CHAT_MESSAGE } = chat_message.as_str(),
        "Global chat message"
    );

    // Server notification creation
    let mut data = BytesMut::new();
    data.put(username.as_bytes());
    data.put_u8(0); // NULL terminator
    data.put(chat_message.as_bytes());
    data.put_u8(0); // NULL terminator

    // Send message to all clients
    util::broadcast_packet(&state, 0x07, data.to_vec()).await?;

    Ok(())
}
