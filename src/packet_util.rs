use std::sync::Arc;

use crate::*;
use bytes::{Buf, BytesMut};
use tokio::io;
use tokio::net::UdpSocket;
use tracing::{debug, error};

pub async fn handle_control_socket(
    control_socket: Arc<UdpSocket>,
    main_port: u16,
) -> io::Result<()> {
    let mut buf = [0u8; 4096];
    loop {
        let (len, src) = control_socket.recv_from(&mut buf).await?;
        let data = &buf[..len];

        // Handle the HELLO0.83 message
        if data == b"HELLO0.83\x00" {
            debug!(
                { fields::ADDR } = %src,
                { fields::PORT } = main_port,
                "HELLO request received on control socket"
            );
            let response = format!("HELLOD00D{}\0", main_port).into_bytes();
            control_socket.send_to(&response, src).await?;
        }
        // Handle the PING message
        else if data == b"PING\x00" {
            debug!(
                { fields::ADDR } = %src,
                "PING request received on control socket"
            );
            let response = b"PONG\x00".to_vec();
            control_socket.send_to(&response, src).await?;
        } else {
            let ascii_string: String = data
                .iter()
                .map(|&b| if b.is_ascii() { b as char } else { '.' }) // Replace invalid with '.'
                .collect();
            error!(
                { fields::ADDR } = %src,
                { fields::PACKET_SIZE } = data.len(),
                message_preview = &ascii_string[..ascii_string.len().min(50)],
                "Unknown message on control socket"
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
            return Err(format!(
                "Incomplete message header. Buffer size: {}",
                buf.len()
            ));
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
