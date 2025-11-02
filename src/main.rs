use handlerf::*;
use packet_util::*;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

mod fields;
mod kaillera;
mod packet_util;

mod handlers;
use handlers::*;

mod session_manager;

mod game_sync;
mod simple_game_sync;
mod simplest_game_sync;
use handlers::data::*;
use session_manager::SessionManager;

// Configuration structures
#[derive(Debug, Deserialize, Clone)]
struct Config {
    #[serde(default = "default_main_port")]
    main_port: u16,
    #[serde(default = "default_sub_port")]
    control_port: u16,
    #[serde(default)]
    tracing: TracingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            main_port: default_main_port(),
            control_port: default_sub_port(),
            tracing: TracingConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct TracingConfig {
    #[serde(default = "default_format")]
    format: String,
    #[serde(default = "default_level")]
    level: String,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            level: default_level(),
        }
    }
}

fn default_main_port() -> u16 {
    27888
}

fn default_sub_port() -> u16 {
    8080
}

fn default_format() -> String {
    "compact".to_string()
}

fn default_level() -> String {
    "info".to_string()
}

// Load configuration from direlera.toml
fn load_config() -> Config {
    match fs::read_to_string("direlera.toml") {
        Ok(contents) => match toml::from_str(&contents) {
            Ok(config) => {
                eprintln!("Configuration loaded from direlera.toml");
                config
            }
            Err(e) => {
                eprintln!("Failed to parse direlera.toml: {}", e);
                eprintln!("Using default configuration");
                Config::default()
            }
        },
        Err(_) => {
            eprintln!("direlera.toml not found, using default configuration");
            Config::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    color_eyre::install()?;
    // Load configuration from direlera.toml
    let config = load_config();

    // Parse log level
    let log_level = match config.tracing.level.to_lowercase().as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => {
            eprintln!("Invalid log level '{}', using INFO", config.tracing.level);
            tracing::Level::INFO
        }
    };

    // Initialize tracing subscriber based on config
    match config.tracing.format.to_lowercase().as_str() {
        "pretty" => {
            tracing_subscriber::fmt()
                .with_max_level(log_level)
                .with_target(false)
                .pretty()
                .init();
        }
        "json" => {
            tracing_subscriber::fmt()
                .with_max_level(log_level)
                .with_target(false)
                .json()
                .init();
        }
        "compact" | _ => {
            tracing_subscriber::fmt()
                .with_max_level(log_level)
                .with_target(false)
                .init();
        }
    }

    info!(
        { fields::CONFIG_SOURCE } = "direlera.toml",
        { fields::PORT } = config.main_port,
        control_port = config.control_port,
        tracing_format = config.tracing.format.as_str(),
        tracing_level = config.tracing.level.as_str(),
        "Server configuration loaded"
    );

    // Bind two UDP sockets using configured ports
    let main_socket = Arc::new(
        UdpSocket::bind(format!("0.0.0.0:{}", config.main_port))
            .await
            .map_err(|e| {
                error!(
                    { fields::PORT } = config.main_port,
                    { fields::ERROR } = %e,
                    "Failed to bind main socket"
                );
                e
            })?,
    );

    let control_socket = Arc::new(
        UdpSocket::bind(format!("0.0.0.0:{}", config.control_port))
            .await
            .map_err(|e| {
                error!(
                    { fields::PORT } = config.control_port,
                    { fields::ERROR } = %e,
                    "Failed to bind control socket"
                );
                e
            })?,
    );

    info!(
        { fields::PORT } = config.main_port,
        control_port = config.control_port,
        bind_address = "0.0.0.0",
        "Sockets bound successfully - server listening"
    );

    let (tx, mut rx) = mpsc::channel::<Message>(100);

    // Centralized AppState with RwLock for efficiency (shared by all sessions)
    let global_state = Arc::new(AppState::new(tx.clone()));

    // Initialize Session Manager for TCP-like session handling
    let (session_manager, packet_rx) = SessionManager::new();
    let session_manager = Arc::new(session_manager);

    // Start periodic session cleanup task
    session_manager
        .clone()
        .start_cleanup_task(global_state.clone());

    // Start session manager (spawns handlers for each client)
    let manager_for_run = session_manager.clone();
    let state_for_sessions = global_state.clone();
    tokio::spawn(async move {
        manager_for_run.run(packet_rx, state_for_sessions).await;
    });

    info!("Server initialization complete");

    // Task to send responses in the main socket
    let main_socket_send = main_socket.clone();
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if let Err(e) = main_socket_send.send_to(&message.data, &message.addr).await {
                warn!(
                    { fields::ADDR } = %message.addr,
                    { fields::ERROR } = %e,
                    "Failed to send response"
                );
            }
        }
    });

    // Control logic for HELLO0.83 and PING requests (Port 27888)
    let main_port_for_control = config.main_port;
    tokio::spawn(handle_control_socket(
        control_socket.clone(),
        main_port_for_control,
    ));

    info!("Server ready to accept connections");

    // Main UDP dispatcher - forwards packets to session manager
    let packet_sender = session_manager.packet_sender();
    let mut buf = [0u8; 4096];

    loop {
        let recv_result = main_socket.recv_from(&mut buf).await;
        let (len, src) = match recv_result {
            Ok(ok) => ok,
            Err(e) => {
                // recv_from errors are usually system-level issues, not client-specific
                // Log at debug level to avoid spam, as these are often expected
                debug!(
                    { fields::ERROR } = %e,
                    "recv_from failed, continuing"
                );
                continue;
            }
        };
        let data = buf[..len].to_vec();

        debug!(
            { fields::ADDR } = %src,
            { fields::PACKET_SIZE } = len,
            "Packet received - forwarding to session manager"
        );

        // Forward to session manager (will create session if needed)
        if let Err(e) = packet_sender.send((src, data)).await {
            warn!(
                { fields::ADDR } = %src,
                { fields::ERROR } = %e,
                "Failed to forward packet to session manager"
            );
        }
    }
}

// Message struct needs to be accessible in both files
pub struct Message {
    pub data: Vec<u8>,
    pub addr: std::net::SocketAddr,
}

/// Process a single packet within a session
async fn process_packet_in_session(
    data: Vec<u8>,
    addr: std::net::SocketAddr,
    global_state: Arc<AppState>,
) {
    // Get client info for packet span
    let client_info = global_state.get_client(&addr).await;

    let span = if let Some(ref client) = client_info {
        if let Some(game_id) = client.game_id {
            tracing::info_span!(
                "packet",
                { fields::USER_NAME } = %client.username,
                { fields::USER_ID } = client.user_id,
                { fields::GAME_ID } = game_id,
                { fields::CONNECTION_TYPE } = client.conn_type,
                ping = client.ping,
            )
        } else {
            tracing::info_span!(
                "packet",
                { fields::USER_NAME } = %client.username,
                { fields::USER_ID } = client.user_id,
                { fields::CONNECTION_TYPE } = client.conn_type,
                ping = client.ping,
            )
        }
    } else {
        tracing::info_span!("packet")
    };
    let _enter = span.enter();

    debug!("Processing packet");

    // Parse and handle messages
    match parse_packet(&data) {
        Ok(messages) => {
            for message in messages.iter() {
                // 0 is special case, it means the first message
                if message.message_number == 0 && messages.len() == 1 {
                    global_state.packet_peeker.write().await.insert(addr, 0);
                }
            }

            for message in messages {
                let mut packet_peeker_lock = global_state.packet_peeker.write().await;
                let message_number_to_process = *packet_peeker_lock.get(&addr).unwrap_or(&0);

                if message.message_number == message_number_to_process {
                    // Update message number before processing to release lock quickly
                    packet_peeker_lock.insert(addr, message_number_to_process + 1);
                    drop(packet_peeker_lock); // Explicitly release lock before long operation

                    // Save message_number before moving message
                    let msg_number = message.message_number;
                    let msg_type = message.message_type;

                    // Handle message and log errors without crashing
                    if let Err(e) = handle_message(message, &addr, global_state.clone()).await {
                        error!(
                            { fields::MESSAGE_NUMBER } = msg_number,
                            { fields::MESSAGE_TYPE } = format!("0x{:02X}", msg_type),
                            { fields::ERROR } = %e,
                            "Failed to handle message"
                        );
                    }
                }
            }
        }
        Err(e) => {
            warn!(
                { fields::PACKET_SIZE } = data.len(),
                { fields::ERROR } = %e,
                "Failed to parse packet"
            );
        }
    }
}
