use direlera_rs::accept_server::AcceptServer;
use direlera_rs::room::*;
use direlera_rs::service_server::{self, *};
use std::cell::RefCell;
use std::error::Error;
use std::net::SocketAddr;
use std::rc::Rc;
use std::{env, io};
use tokio::join;
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind(&"0.0.0.0:27888").await?;
    println!("Listening on: {}", socket.local_addr()?);

    let server = AcceptServer {
        socket,
        buf: vec![0; 1024],
        to_send: None,
    };

    let user_room = UserRoom::new();
    let service_sock = UdpSocket::bind(&"0.0.0.0:27999").await?;
    let mut service_server = ServiceServer {
        socket: service_sock,
        buf: vec![0; 1024],
        to_send: None,
        user_room,
        game_id: 0,
    };
    tokio::join!(server.run(), service_server.run());

    Ok(())
}
