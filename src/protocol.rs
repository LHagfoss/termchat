use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServer {
    Handshake { name: String, token: String },
    ChatMessage { content: String },
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
    Error {
        message: String,
    },
}
