use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::atomic::{AtomicU16, AtomicU32, Ordering},
    sync::Arc,
    time::Instant,
};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::simple_game_sync;

type PlayerStatus = u8;
pub const PLAYER_STATUS_PLAYING: PlayerStatus = 0;
pub const PLAYER_STATUS_IDLE: PlayerStatus = 1;
pub const PLAYER_STATUS_NET_SYNC: PlayerStatus = 2;

type GameStatus = u8;
pub const GAME_STATUS_WAITING: GameStatus = 0;
pub const GAME_STATUS_PLAYING: GameStatus = 1;
#[allow(dead_code)]
pub const GAME_STATUS_NETSYNC: GameStatus = 2;

// AppState - centralized state with RwLock for efficiency
pub struct AppState {
    // RwLock: multiple readers, exclusive writer
    pub clients_by_addr: Arc<RwLock<HashMap<SocketAddr, Uuid>>>,
    pub clients_by_id: Arc<RwLock<HashMap<Uuid, ClientInfo>>>,
    pub games: Arc<RwLock<HashMap<u32, GameInfo>>>,
    pub packet_peeker: Arc<RwLock<HashMap<SocketAddr, u16>>>,

    // Atomic: lock-free counter increment
    pub next_game_id: Arc<AtomicU32>,
    pub next_user_id: Arc<AtomicU16>,

    pub tx: mpsc::Sender<crate::Message>,
}

impl AppState {
    pub fn new(tx: mpsc::Sender<crate::Message>) -> Self {
        Self {
            clients_by_addr: Arc::new(RwLock::new(HashMap::new())),
            clients_by_id: Arc::new(RwLock::new(HashMap::new())),
            games: Arc::new(RwLock::new(HashMap::new())),
            packet_peeker: Arc::new(RwLock::new(HashMap::new())),
            next_game_id: Arc::new(AtomicU32::new(1)),
            next_user_id: Arc::new(AtomicU16::new(1)),
            tx,
        }
    }

    // Lock-free ID generation
    pub fn next_game_id(&self) -> u32 {
        self.next_game_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn next_user_id(&self) -> u16 {
        self.next_user_id.fetch_add(1, Ordering::SeqCst)
    }

    // Read - multiple threads can read simultaneously
    pub async fn get_client(&self, addr: &SocketAddr) -> Option<ClientInfo> {
        let addr_map = self.clients_by_addr.read().await;
        let session_id = addr_map.get(addr)?;
        let id_map = self.clients_by_id.read().await;
        id_map.get(session_id).cloned()
    }

    // Write - exclusive lock
    pub async fn add_client(&self, addr: SocketAddr, client: ClientInfo) {
        let session_id = client.session_id;

        let mut id_map = self.clients_by_id.write().await;
        id_map.insert(session_id, client);

        let mut addr_map = self.clients_by_addr.write().await;
        addr_map.insert(addr, session_id);
    }

    pub async fn remove_client(&self, addr: &SocketAddr) -> Option<ClientInfo> {
        let mut addr_map = self.clients_by_addr.write().await;
        let session_id = addr_map.remove(addr)?;

        let mut id_map = self.clients_by_id.write().await;
        id_map.remove(&session_id)
    }

    // Get all client addresses
    pub async fn get_all_client_addrs(&self) -> Vec<SocketAddr> {
        let addr_map = self.clients_by_addr.read().await;
        addr_map.keys().cloned().collect()
    }

    // Game operations
    pub async fn add_game(&self, game_id: u32, game: GameInfo) {
        let mut games = self.games.write().await;
        games.insert(game_id, game);
    }

    pub async fn get_game(&self, game_id: u32) -> Option<GameInfo> {
        let games = self.games.read().await;
        games.get(&game_id).cloned()
    }

    pub async fn remove_game(&self, game_id: u32) -> Option<GameInfo> {
        let mut games = self.games.write().await;
        games.remove(&game_id)
    }

    pub async fn update_game<F, R, E>(&self, game_id: u32, f: F) -> Result<R, E>
    where
        F: FnOnce(&mut GameInfo) -> Result<R, E>,
    {
        let mut games = self.games.write().await;
        let game = games.get_mut(&game_id).ok_or_else(|| {
            // This will be converted to the error type E by the caller
            panic!("Game not found")
        })?;
        f(game)
    }

    pub async fn update_client<F, R, E>(&self, addr: &SocketAddr, f: F) -> Result<R, E>
    where
        F: FnOnce(&mut ClientInfo) -> Result<R, E>,
    {
        let addr_map = self.clients_by_addr.read().await;
        let session_id = addr_map
            .get(addr)
            .cloned()
            .ok_or_else(|| panic!("Client not found"))?;
        drop(addr_map);

        let mut id_map = self.clients_by_id.write().await;
        let client = id_map
            .get_mut(&session_id)
            .ok_or_else(|| panic!("Client not found"))?;
        f(client)
    }
}

// ClientInfo and GameInfo structs need to be accessible in both files
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub session_id: Uuid,
    pub username: String,
    pub emulator_name: String,
    pub conn_type: u8,
    pub user_id: u16,
    pub ping: u32,
    pub player_status: PlayerStatus,
    pub game_id: Option<u32>,
    pub last_ping_time: Option<Instant>,
    pub ack_count: u16,
    //////////////////
    /// Packet generator for this client (handles sequence numbers and redundancy)
    pub packet_generator: crate::kaillera::protocol::UDPPacketGenerator,
}

#[derive(Debug, Clone)]
pub struct GameInfo {
    pub game_id: u32,
    pub game_name: String,
    pub emulator_name: String,
    pub owner: String,      // Display name (for backward compatibility)
    pub owner_user_id: u16, // Owner's user_id for authorization checks
    pub num_players: u8,
    pub max_players: u8,
    pub game_status: u8, // 0=Waiting, 1=Playing, 2=Netsync
    pub players: HashSet<std::net::SocketAddr>,
    // New: SimpleGameSync for frame synchronization
    pub sync_manager: Option<simple_game_sync::SimpleGameSync>,
    // Player addresses in order (indexed by player_id)
    pub player_addrs: Vec<std::net::SocketAddr>,
    // Player delays (indexed by player_id)
    pub player_delays: Vec<usize>,
    pub dropped_players: Vec<bool>,
}
