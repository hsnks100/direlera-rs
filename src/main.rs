use handlerf::*;
use packet_util::*;
use std::error::Error;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

mod kaillera;
mod packet_util;

mod handlers;
use handlers::*;

mod game_sync;
mod simple_game_sync;
use handlers::data::*;

const MAIN_PORT: u16 = 8080;
const CONTROL_PORT: u16 = 27888;
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Bind two UDP sockets: one for main logic (8080) and one for control logic (27888)
    let main_socket = Arc::new(UdpSocket::bind(format!("0.0.0.0:{}", MAIN_PORT)).await?);
    let control_socket = Arc::new(UdpSocket::bind(format!("0.0.0.0:{}", CONTROL_PORT)).await?);
    println!("Main socket listening on 127.0.0.1:{}...", MAIN_PORT);
    println!("Control socket listening on 127.0.0.1:{}...", CONTROL_PORT);

    let (tx, mut rx) = mpsc::channel::<Message>(100);

    // Centralized AppState with RwLock for efficiency
    let state = Arc::new(AppState::new(tx.clone()));

    // Task to send responses in the main socket
    let main_socket_send = main_socket.clone();
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if let Err(e) = main_socket_send.send_to(&message.data, &message.addr).await {
                eprintln!("Failed to send response to {}: {}", message.addr, e);
            }
        }
    });

    // Control logic for HELLO0.83 and PING requests (Port 27888)
    tokio::spawn(handle_control_socket(control_socket.clone()));

    // Main game loop for game-related messages (Port 8080)
    let mut buf = [0u8; 4096];
    loop {
        let (len, src) = main_socket.recv_from(&mut buf).await?;
        let data = &buf[..len];

        match parse_packet(data) {
            Ok(messages) => {
                for message in messages.iter() {
                    // 0 is special case, it means the first message
                    if message.message_number == 0 && messages.len() == 1 {
                        state.packet_peeker.write().await.insert(src, 0);
                    }
                }
                for message in messages {
                    let mut packet_peeker_lock = state.packet_peeker.write().await;
                    let message_number_to_process = *packet_peeker_lock.get(&src).unwrap_or(&0);

                    if message.message_number == message_number_to_process {
                        // Update message number before processing to release lock quickly
                        packet_peeker_lock.insert(src, message_number_to_process + 1);
                        drop(packet_peeker_lock); // Explicitly release lock before long operation

                        // Handle message and log errors without crashing
                        if let Err(e) = handle_message(message, &src, state.clone()).await {
                            eprintln!("[Error] Failed to handle message from {}: {}", src, e);
                            // Continue processing other messages instead of crashing
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to parse packet from {}: {}", src, e);
            }
        }
    }
}

// Message struct needs to be accessible in both files
pub struct Message {
    pub data: Vec<u8>,
    pub addr: std::net::SocketAddr,
}
