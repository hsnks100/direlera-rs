use crate::*;
use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tracing::{debug, info};

use crate::kaillera::message_types as msg;
use crate::simple_game_sync;
/*
'     0x11 = Start Game
'            Client Request:
'            NB : Empty String [00]
'            2B : 0xFF
'            1B : 0xFF
'            1B : 0xFF
'
'            Server Notification:
'            NB : Empty String [00]
'            2B : Frame Delay (eg. (connectionType * (frameDelay + 1) <-Block on that frame
'            1B : Your Player Number (eg. if you're player 1 or 2...)
'            1B : Total Players
- **Client**: Sends **Start Game Request** `[0x11]`
- **Server**: Sends **Update Game Status** `[0x0E]`
- **Server**: Sends **Start Game Notification** `[0x11]`
- **Client**: Enters **Netsync Mode** and waits for all players to send **Ready to Play Signal** `[0x15]`
- **Server**: Sends **Update Game Status** for whole server players`[0x0E]`
- **Server**: Enters **Playing Mode** after receiving **Ready to Play Signal Notification** `[0x15]` from all players in room
- **Client(s)**: Exchange data using **Game Data Send** `[0x12]` or **Game Cache Send** `[0x13]`
- **Server**: Sends data accordingly using **Game Data Notify** `[0x12]` or **Game Cache Notify** `[0x13]`

 */
pub async fn handle_start_game(
    message: kaillera::protocol::ParsedMessage,
    src: &std::net::SocketAddr,
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = util::read_string(&mut buf); // Empty String
    let _ = buf.get_u32_le(); // 0xFFFF 0xFF 0xFF

    let game_id = {
        let client = state.get_client(src).await.ok_or("Client not found")?;
        client.game_id.ok_or("Client not in a game")?
    };

    // Initialize SimpleGameSync when game starts
    util::with_game_mut(&state, src, |game_info| {
        game_info.game_status = 1; // Playing

        // Initialize SimpleGameSync with player delays
        let delays = game_info.player_delays.clone();
        game_info.sync_manager = Some(simple_game_sync::SimpleGameSync::new_without_padding(
            delays,
        ));
    })
    .await?;

    let game_info = util::fetch_game_info(src, &state).await?;

    info!(
        { fields::GAME_ID } = game_id,
        { fields::PLAYER_COUNT } = game_info.player_addrs.len(),
        { fields::GAME_STATUS } = "playing",
        "Game started"
    );

    // Update game status
    let status_data = util::make_update_game_status(&game_info)?;
    util::broadcast_packet(&state, msg::UPDATE_GAME_STATUS, status_data).await?;

    // Send start game notification with each player's delay
    for (i, player_addr) in game_info.player_addrs.iter().enumerate() {
        let player_delay = game_info.player_delays[i];
        let mut data = BytesMut::new();
        data.put_u8(0);
        data.put_u16_le(player_delay as u16); // Frame Delay (player's connection_type)
        debug!(
            player_number = i + 1,
            frame_delay = player_delay,
            { fields::ADDR } = %player_addr,
            "Sending start game notification"
        );
        data.put_u8((i + 1) as u8); // Player Number (1-indexed)
        data.put_u8(game_info.player_addrs.len() as u8); // Total Players
        util::send_packet(&state, player_addr, msg::START_GAME, data.to_vec()).await?;
    }
    Ok(())
}
