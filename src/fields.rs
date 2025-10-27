// Structured logging field definitions
// This module centralizes all field names used in tracing logs

#![allow(dead_code)]

// Connection & Network fields
pub const ADDR: &str = "addr";
pub const PORT: &str = "port";
pub const CLIENT_IP: &str = "client_ip";
pub const PACKET_SIZE: &str = "packet_size";

// User fields
pub const USER_NAME: &str = "user_name";
pub const USER_ID: &str = "user_id";
pub const CONNECTION_TYPE: &str = "connection_type";
pub const PING: &str = "ping";

// Game fields
pub const GAME_ID: &str = "game_id";
pub const GAME_NAME: &str = "game_name";
pub const GAME_STATUS: &str = "game_status";
pub const PLAYER_COUNT: &str = "player_count";
pub const MAX_PLAYERS: &str = "max_players";

// Message fields
pub const MESSAGE_TYPE: &str = "message_type";
pub const MESSAGE_NUMBER: &str = "message_number";
pub const MESSAGE_LENGTH: &str = "message_length";
pub const CHAT_MESSAGE: &str = "chat_message";

// Operation fields
pub const OPERATION: &str = "operation";
pub const STATUS: &str = "status";
pub const ERROR: &str = "error";
pub const REASON: &str = "reason";

// Performance fields
pub const ELAPSED_MS: &str = "elapsed_ms";
pub const QUEUE_SIZE: &str = "queue_size";

// Server fields
pub const SERVER_VERSION: &str = "server_version";
pub const CONFIG_SOURCE: &str = "config_source";

// Game sync fields
pub const PLAYER_ID: &str = "player_id";
pub const PLAYER_NUMBER: &str = "player_number";
pub const FRAME_DELAY: &str = "frame_delay";
pub const CACHE_POSITION: &str = "cache_position";
pub const DATA_LENGTH: &str = "data_length";

// Kick/Drop fields
pub const KICKED_USER_ID: &str = "kicked_user_id";
pub const DROPPER_USERNAME: &str = "dropper_username";
pub const WAS_PLAYING: &str = "was_playing";
