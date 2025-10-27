use crate::*;
use bytes::{BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::info;

pub async fn handle_game_chat(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    let mut buf = BytesMut::from(&message.data[..]);

    // NB: Empty String
    let _empty = util::read_string(&mut buf);
    // NB: Message
    let chat_message = util::read_string(&mut buf);

    // Get username and game ID
    let (username, game_id) = if let Some(client_info) = state.get_client(src).await {
        (client_info.username.clone(), client_info.game_id)
    } else {
        ("Unknown".to_string(), None)
    };

    if let Some(game_id) = game_id {
        info!(
            { fields::GAME_ID } = game_id,
            { fields::USER_NAME } = username.as_str(),
            { fields::CHAT_MESSAGE } = chat_message.as_str(),
            "Game chat message"
        );

        // Response creation
        let mut data = BytesMut::new();
        data.put(username.as_bytes());
        data.put_u8(0);
        data.put(chat_message.as_bytes());
        data.put_u8(0);

        // Send to all clients in the same game
        let packets: Vec<_> = {
            let addr_map = state.clients_by_addr.read().await;
            let mut id_map = state.clients_by_id.write().await;

            addr_map
                .iter()
                .filter_map(|(addr, session_id)| {
                    let client = id_map.get_mut(session_id)?;
                    if client.game_id != Some(game_id) {
                        return None;
                    }
                    let packet = client
                        .packet_generator
                        .make_send_packet(0x08, data.to_vec());
                    Some((*addr, packet))
                })
                .collect()
        };

        for (addr, packet) in packets {
            state.tx.send(Message { data: packet, addr }).await?;
        }
    } else {
        tracing::warn!(
            { fields::USER_NAME } = username.as_str(),
            "Client attempted game chat but not in a game"
        );
    }

    Ok(())
}
