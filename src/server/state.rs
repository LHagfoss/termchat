use crate::protocol::ServerToClient;
use std::collections::{HashMap, HashSet, VecDeque};
use tokio::sync::{Mutex, broadcast};

pub struct StoredFile {
    pub filename: String,
    pub data: Vec<u8>,
}

pub struct ServerState {
    pub server_name: String,
    pub token: String,
    pub tx: broadcast::Sender<ServerToClient>,
    pub users: Mutex<HashSet<String>>,
    pub debug: bool,
    pub history: Mutex<VecDeque<String>>,
    pub files: Mutex<HashMap<String, StoredFile>>,
    pub user_colors: Mutex<HashMap<String, String>>,
}
