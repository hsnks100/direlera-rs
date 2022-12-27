use log::{info};
use std::collections::HashMap;

use std::net::SocketAddr;
use std::{io};

use tokio::net::UdpSocket;
pub struct AcceptServer {
    pub socket: UdpSocket,
    pub buf: Vec<u8>,
    pub to_send: Option<(usize, SocketAddr)>,
    pub config_obj: HashMap<String, String>,
}

impl AcceptServer {
    pub async fn run(self) -> Result<(), io::Error> {
        info!("Accept Run");
        let AcceptServer {
            socket,
            mut buf,
            mut to_send,
            config_obj,
        } = self;

        loop {
            // First we check to see if there's a message we need to echo back.
            // If so then we try to send it back to the original source, waiting
            // until it's writable and we're able to do so.
            if let Some((size, peer)) = to_send {
                info!("size: {}", size);
                if size == 5 {
                    let ping = b"PING\x00";
                    if ping == &buf[..size] {
                        let _amt = socket.send_to("PONG\x00".as_bytes(), &peer).await?;
                    }
                } else if size > 5 && &buf[..5] == "HELLO".as_bytes() {
                    let sub_port = config_obj.get("sub_port").unwrap();
                    let _amt = socket
                        .send_to(format!("HELLOD00D{}\x00", sub_port).as_bytes(), &peer)
                        .await?;
                }
            }
            to_send = Some(socket.recv_from(&mut buf).await?);
        }
    }
}
