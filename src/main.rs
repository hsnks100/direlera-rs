use direlera_rs::accept_server::AcceptServer;
use direlera_rs::room::*;
use direlera_rs::service_server::{self, *};
use std::error::Error;
use std::net::SocketAddr;
use std::{env, io};
use tokio::join;
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // let mut ur = UserRoom::new();
    // let r = Room::new();

    // ur.add_room(1, r)?;
    // match ur.rooms.get_mut(&1) {
    //     Some(s) => {
    //         s.game_name = "hihi".to_string();
    //     }
    //     None => {}
    // }

    // for u in ur.rooms {
    //     println!("room: {}, {:?}", u.0, u.1);
    // }

    // return Ok(());
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
    };
    tokio::join!(server.run(), service_server.run());

    Ok(())
}
