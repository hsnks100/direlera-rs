use std::error::Error;
use std::net::SocketAddr;
use std::{env, io};
use tokio::join;
use tokio::net::UdpSocket;
pub struct AcceptServer {
    pub socket: UdpSocket,
    pub buf: Vec<u8>,
    pub to_send: Option<(usize, SocketAddr)>,
}

impl AcceptServer {
    pub async fn run(self) -> Result<(), io::Error> {
        println!("Accept Run");
        let AcceptServer {
            socket,
            mut buf,
            mut to_send,
        } = self;

        loop {
            // First we check to see if there's a message we need to echo back.
            // If so then we try to send it back to the original source, waiting
            // until it's writable and we're able to do so.
            if let Some((size, peer)) = to_send {
                println!("size: {}", size);
                if size == 5 {
                    let ping = b"PING\x00";
                    if ping == &buf[..size] {
                        let amt = socket.send_to("PONG\x00".as_bytes(), &peer).await?;
                    }
                } else if size > 5 && &buf[..5] == "HELLO".as_bytes() {
                    let amt = socket
                        .send_to("HELLOD00D27999\x00".as_bytes(), &peer)
                        .await?;
                }
            }

            // If we're here then `to_send` is `None`, so we take a look for the
            // next message we're going to echo back.
            to_send = Some(socket.recv_from(&mut buf).await?);
        }
    }
}
