use std::sync::Arc;

use crate::*;
use bytes::{Buf, BytesMut};
use tokio::io;
use tokio::net::UdpSocket;

pub async fn handle_control_socket(control_socket: Arc<UdpSocket>) -> io::Result<()> {
    let mut buf = [0u8; 4096];
    loop {
        let (len, src) = control_socket.recv_from(&mut buf).await?;
        let data = &buf[..len];

        // Handle the HELLO0.83 message
        if data == b"HELLO0.83\x00" {
            let response = format!("HELLOD00D{}\0", crate::MAIN_PORT).into_bytes();
            control_socket.send_to(&response, src).await?;
        }
        // Handle the PING message
        else if data == b"PING\x00" {
            let response = b"PONG\x00".to_vec();
            control_socket.send_to(&response, src).await?;
        } else {
            let ascii_string: String = data
                .iter()
                .map(|&b| if b.is_ascii() { b as char } else { '.' }) // Replace invalid with '.'
                .collect();
            eprintln!(
                "Unknown message on control socket from {}: {:?}, {}",
                src, data, ascii_string
            );
        }
    }
}

pub fn parse_packet(data: &[u8]) -> Result<Vec<kaillera::protocol::ParsedMessage>, String> {
    let mut buf = BytesMut::from(data);
    if buf.is_empty() {
        return Err("Packet is empty.".to_string());
    }
    let num_messages = buf.get_u8();

    let mut messages = Vec::new();

    for _ in 0..num_messages {
        if buf.len() < 5 {
            println!("Current buffer content: {:02X?}", buf);
            return Err("Incomplete message header.".to_string());
        }

        let message_number = buf.get_u16_le();
        let message_length = buf.get_u16_le();
        let message_type = buf.get_u8();

        if buf.len() < (message_length - 1) as usize {
            return Err("Incomplete message data.".to_string());
        }

        let message_data = buf.split_to((message_length - 1) as usize);

        messages.push(kaillera::protocol::ParsedMessage {
            message_number,
            message_length,
            message_type,
            data: message_data.to_vec(),
        });
    }

    Ok(messages)
}
