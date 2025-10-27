use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::{debug, info};

use crate::*;
/*
'            Server Notification:
'            NB : Username
'            2B : UserID
'            NB : Message
 */
pub async fn handle_user_quit(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    let mut buf = BytesMut::from(&message.data[..]);

    // NB: Empty String
    let _empty = util::read_string(&mut buf);
    // 2B: 0xFF
    let _code = if buf.len() >= 2 { buf.get_u16_le() } else { 0 };
    // NB: Message
    let user_message = util::read_string(&mut buf);

    // Handle quit game first
    handlers::quit_game::handle_quit_game(vec![0x00, 0xFF, 0xFF], src, state.clone()).await?;

    // Remove client from list
    if let Some(client_info) = state.remove_client(src).await {
        info!(
            { fields::USER_NAME } = client_info.username.as_str(),
            { fields::USER_ID } = client_info.user_id,
            quit_message = user_message.as_str(),
            "User quit"
        );
        let mut data = BytesMut::new();
        data.put(client_info.username.as_bytes());
        data.put_u8(0);
        data.put_u16_le(client_info.user_id);
        data.put(user_message.as_bytes());
        data.put_u8(0);
        util::broadcast_packet(&state, 0x01, data.to_vec()).await?;
    } else {
        debug!(
            quit_message = user_message.as_str(),
            "Unknown client quit"
        );
    }
    Ok(())
}
