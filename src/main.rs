use config::Config;
use direlera_rs::accept_server::AcceptServer;
use direlera_rs::room::*;
use direlera_rs::service_server::{self, *};
use log::{error, info, log_enabled, trace, warn, Level, LevelFilter};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::io::Write;
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // let mut v = vec![vec![1, 2, 3, 4], vec![5, 6, 7, 8]];
    // let v2 = v.get_mut(0);
    // match v2 {
    //     Some(i) => {
    //         let newV = i[0..2].to_vec();
    //         *i = i[2..].to_vec();
    //     }
    //     None => {}
    // }
    // println!("{:?}", v);

    // return Ok(());
    env::set_var("RUST_LOG", "info");
    let settings = Config::builder()
        // Add in `./Settings.toml`
        .add_source(config::File::with_name("./direlera"))
        // Add in settings from the environment (with a prefix of APP)
        // Eg.. `APP_DEBUG=1 ./target/app` would set the `debug` key
        .add_source(config::Environment::with_prefix("APP"))
        .build()
        .unwrap();

    // Print out our settings (as a HashMap)
    let config_obj = settings.try_deserialize::<HashMap<String, String>>()?;
    println!("{:?}", config_obj);
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}:{} {} [{}] - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter_level(LevelFilter::Info)
        .init();
    // env_logger::init();
    if log_enabled!(Level::Info) {
        let x = 3 * 4; // expensive computation
        info!("the answer was: {}", x);
    }
    let main_port = config_obj.get("main_port").unwrap();
    let socket = UdpSocket::bind(&format!("0.0.0.0:{}", main_port)).await?;
    error!("Listening on: {}", socket.local_addr()?);

    let server = AcceptServer {
        socket,
        buf: vec![0; 1024],
        to_send: None,
        config_obj: config_obj.clone(),
    };

    let session_manager = UserRoom::new();
    let sub_port = config_obj.get("sub_port").unwrap();
    let service_sock = UdpSocket::bind(&format!("0.0.0.0:{}", sub_port)).await?;
    let mut service_server = ServiceServer {
        config: config_obj,
        socket: service_sock,
        buf: vec![0; 1024],
        to_send: None,
        session_manager,
        game_id: 0,
    };
    tokio::join!(server.run(), service_server.run());

    Ok(())
}
