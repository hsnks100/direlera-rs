// Session-based UDP handling - TCP-like session management for UDP
// Each client gets its own session handler, similar to TCP connections

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::{fields, AppState};

/// Configuration for session timeout behavior
const SESSION_TIMEOUT: Duration = Duration::from_secs(120);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

/// Represents a single UDP "session" - simulating TCP connection
struct UdpSession {
    last_seen: Instant,
    tx: mpsc::Sender<Vec<u8>>,
}

/// Manages all active UDP sessions
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<SocketAddr, UdpSession>>>,
    packet_tx: mpsc::Sender<(SocketAddr, Vec<u8>)>,
}

impl SessionManager {
    pub fn new() -> (Self, mpsc::Receiver<(SocketAddr, Vec<u8>)>) {
        let (packet_tx, packet_rx) = mpsc::channel(1000);

        let manager = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            packet_tx,
        };

        (manager, packet_rx)
    }

    /// Get the sender for the main UDP dispatcher to send packets
    pub fn packet_sender(&self) -> mpsc::Sender<(SocketAddr, Vec<u8>)> {
        self.packet_tx.clone()
    }

    /// Start the session manager - spawns session handlers as needed
    pub async fn run(
        self: Arc<Self>,
        mut packet_rx: mpsc::Receiver<(SocketAddr, Vec<u8>)>,
        global_state: Arc<AppState>,
    ) {
        info!("Session manager started");

        while let Some((addr, data)) = packet_rx.recv().await {
            let sessions = self.sessions.read().await;

            if let Some(session) = sessions.get(&addr) {
                // Existing session - forward packet
                if let Err(e) = session.tx.send(data).await {
                    warn!(
                        { fields::ADDR } = %addr,
                        { fields::ERROR } = %e,
                        "Failed to forward packet to session"
                    );
                }
            } else {
                // New session - spawn handler
                drop(sessions);
                self.spawn_session_handler(addr, data, global_state.clone())
                    .await;
            }
        }
    }

    /// Spawn a new session handler for a client address
    async fn spawn_session_handler(
        &self,
        addr: SocketAddr,
        initial_data: Vec<u8>,
        global_state: Arc<AppState>,
    ) {
        let (tx, rx) = mpsc::channel(100);

        let session = UdpSession {
            last_seen: Instant::now(),
            tx: tx.clone(),
        };

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(addr, session);
        }

        info!({ fields::ADDR } = %addr, "New session spawned");

        // Send initial packet to the session channel
        if let Err(e) = tx.send(initial_data).await {
            warn!(
                { fields::ADDR } = %addr,
                { fields::ERROR } = %e,
                "Failed to send initial packet to session"
            );
        }

        // Spawn session handler task
        let sessions = self.sessions.clone();
        tokio::spawn(async move {
            handle_session(addr, rx, sessions, global_state).await;
        });
    }

    /// Start the periodic cleanup task
    pub fn start_cleanup_task(self: Arc<Self>, global_state: Arc<AppState>) {
        tokio::spawn(async move {
            info!("Session cleanup task started");

            loop {
                tokio::time::sleep(CLEANUP_INTERVAL).await;

                let now = Instant::now();
                let mut sessions = self.sessions.write().await;

                let mut to_remove = Vec::new();

                for (addr, session) in sessions.iter() {
                    let elapsed = now.duration_since(session.last_seen);
                    if elapsed > SESSION_TIMEOUT {
                        info!(
                            { fields::ADDR } = %addr,
                            inactive_duration = ?elapsed,
                            "Session timeout"
                        );
                        to_remove.push(*addr);
                    }
                }

                for addr in &to_remove {
                    sessions.remove(addr);
                }

                drop(sessions);

                // Clean up from global state
                for addr in to_remove {
                    global_state.remove_client(&addr).await;
                }
            }
        });
    }
}

/// Handle a single session - this is like handling a TCP connection
async fn handle_session(
    addr: SocketAddr,
    mut rx: mpsc::Receiver<Vec<u8>>,
    sessions: Arc<RwLock<HashMap<SocketAddr, UdpSession>>>,
    global_state: Arc<AppState>,
) {
    let span = tracing::info_span!("session", { fields::ADDR } = %addr);
    let _enter = span.enter();

    info!("Session handler started");

    // Session loop - similar to TCP recv loop
    loop {
        match timeout(SESSION_TIMEOUT, rx.recv()).await {
            Ok(Some(data)) => {
                // Update last_seen
                {
                    let mut sessions_lock = sessions.write().await;
                    if let Some(session) = sessions_lock.get_mut(&addr) {
                        session.last_seen = Instant::now();
                    }
                }

                debug!({ fields::PACKET_SIZE } = data.len(), "Received packet");

                // Process the packet
                crate::process_packet_in_session(data, addr, global_state.clone()).await;
            }
            Ok(None) => {
                info!("Session channel closed");
                break;
            }
            Err(_) => {
                warn!(timeout_duration = ?SESSION_TIMEOUT, "Session timeout");
                break;
            }
        }
    }

    // Clean up session
    {
        let mut sessions_lock = sessions.write().await;
        sessions_lock.remove(&addr);
    }

    // Clean up from global state
    global_state.remove_client(&addr).await;

    info!("Session terminated");
}
