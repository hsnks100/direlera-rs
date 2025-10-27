// Kaillera Protocol Message Types
// Reference: kaillera_original.txt

/// User Quit
pub const USER_QUIT: u8 = 0x01;

/// User joined
pub const USER_JOINED: u8 = 0x02;

/// User Login Information
pub const USER_LOGIN: u8 = 0x03;

/// Server Status
pub const SERVER_STATUS: u8 = 0x04;

/// Server to Client ACK
pub const SERVER_TO_CLIENT_ACK: u8 = 0x05;

/// Client to Server ACK
pub const CLIENT_TO_SERVER_ACK: u8 = 0x06;

/// Global Chat
pub const GLOBAL_CHAT: u8 = 0x07;

/// Game Chat
pub const GAME_CHAT: u8 = 0x08;

/// Client Keep Alive
pub const CLIENT_KEEP_ALIVE: u8 = 0x09;

/// Create Game
pub const CREATE_GAME: u8 = 0x0A;

/// Quit Game
pub const QUIT_GAME: u8 = 0x0B;

/// Join Game
pub const JOIN_GAME: u8 = 0x0C;

/// Player Information
pub const PLAYER_INFORMATION: u8 = 0x0D;

/// Update Game Status
pub const UPDATE_GAME_STATUS: u8 = 0x0E;

/// Kick User from Game
pub const KICK_USER: u8 = 0x0F;

/// Close game
pub const CLOSE_GAME: u8 = 0x10;

/// Start Game
pub const START_GAME: u8 = 0x11;

/// Game Data
pub const GAME_DATA: u8 = 0x12;

/// Game Cache
pub const GAME_CACHE: u8 = 0x13;

/// Drop Game
pub const DROP_GAME: u8 = 0x14;

/// Ready to Play Signal
pub const READY_TO_PLAY: u8 = 0x15;

/// Connection Rejected
pub const CONNECTION_REJECTED: u8 = 0x16;

/// Server Information Message
pub const SERVER_INFORMATION: u8 = 0x17;

/// Convert message type to human-readable name
pub fn message_type_name(msg_type: u8) -> &'static str {
    match msg_type {
        USER_QUIT => "UserQuit",
        USER_JOINED => "UserJoined",
        USER_LOGIN => "UserLogin",
        SERVER_STATUS => "ServerStatus",
        SERVER_TO_CLIENT_ACK => "ServerToClientACK",
        CLIENT_TO_SERVER_ACK => "ClientToServerACK",
        GLOBAL_CHAT => "GlobalChat",
        GAME_CHAT => "GameChat",
        CLIENT_KEEP_ALIVE => "ClientKeepAlive",
        CREATE_GAME => "CreateGame",
        QUIT_GAME => "QuitGame",
        JOIN_GAME => "JoinGame",
        PLAYER_INFORMATION => "PlayerInformation",
        UPDATE_GAME_STATUS => "UpdateGameStatus",
        KICK_USER => "KickUser",
        CLOSE_GAME => "CloseGame",
        START_GAME => "StartGame",
        GAME_DATA => "GameData",
        GAME_CACHE => "GameCache",
        DROP_GAME => "DropGame",
        READY_TO_PLAY => "ReadyToPlay",
        CONNECTION_REJECTED => "ConnectionRejected",
        SERVER_INFORMATION => "ServerInformation",
        _ => "Unknown",
    }
}
