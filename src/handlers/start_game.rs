use crate::*;
use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;

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
    println!("Start Game");
    let mut buf = BytesMut::from(&message.data[..]);
    let _ = util::read_string(&mut buf); // Empty String
    let _ = buf.get_u32_le(); // 0xFFFF 0xFF 0xFF

    // Initialize SimpleGameSync when game starts
    util::with_game_mut(&state, src, |game_info| {
        game_info.game_status = 1; // Playing

        // Initialize SimpleGameSync with player delays
        let delays = game_info.player_delays.clone();
        game_info.sync_manager = Some(simple_game_sync::SimpleGameSync::new_without_padding(
            delays,
        ));

        println!(
            "[StartGame] Initialized SimpleGameSync with {} players",
            game_info.player_addrs.len()
        );
    })
    .await?;

    let game_info = util::fetch_game_info(src, &state).await?;

    // Update game status
    let status_data = util::make_update_game_status(&game_info)?;
    util::broadcast_packet(&state, 0x0E, status_data).await?;

    // Send start game notification with each player's delay
    for (i, player_addr) in game_info.player_addrs.iter().enumerate() {
        let player_delay = game_info.player_delays[i];
        let mut data = BytesMut::new();
        data.put_u8(0);
        data.put_u16_le(player_delay as u16); // Frame Delay (player's connection_type)
        println!(
            "Sending StartGame to Player {} with FrameDelay={}",
            i, player_delay
        );
        data.put_u8((i + 1) as u8); // Player Number (1-indexed)
        data.put_u8(game_info.player_addrs.len() as u8); // Total Players
        util::send_packet(&state, player_addr, 0x11, data.to_vec()).await?;
    }
    Ok(())
}
