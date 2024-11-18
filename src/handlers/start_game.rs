use crate::*;
use bytes::{Buf, BufMut, BytesMut};
use std::error::Error;
use std::sync::Arc;
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

    util::with_game_mut(&state, src, |game_info| {
        game_info.game_status = 1; // Playing
    })
    .await?;

    let game_info = util::fetch_game_info(src, &state).await?;

    // Update game status
    let status_data = util::make_update_game_status(&game_info)?;
    util::broadcast_packet(&state, 0x0E, status_data).await?;

    // Send start game notification
    for (i, player_addr) in game_info.players.iter().enumerate() {
        let mut data = BytesMut::new();
        data.put_u8(0);
        data.put_u16_le(1); // Frame Delay
        println!("Sending StartGame FrameDelay={}", 1);
        data.put_u8((i + 1) as u8); // Player Number
        data.put_u8(game_info.players.len() as u8); // Total Players
        util::send_packet(&state, player_addr, 0x11, data.to_vec()).await?;
    }
    Ok(())
}
