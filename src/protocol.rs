use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServer {
    Handshake { name: String, token: String },
    ChatMessage { content: String },
    Typing { is_typing: bool },
    Ping,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClient {
    Welcome {
        server_name: String,
    },
    Broadcast {
        sender: String,
        content: String,
        timestamp: DateTime<Utc>,
    },
    SystemAlert {
        content: String,
        timestamp: DateTime<Utc>,
    },
    UserTyping {
        sender: String,
        is_typing: bool,
    },
    Error {
        message: String,
    },
    UsersList {
        users: Vec<String>,
    },
    Pong,
}
