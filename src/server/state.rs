use crate::protocol::ServerToClient;
use std::collections::{HashSet, VecDeque};
use tokio::sync::{Mutex, broadcast};

pub struct ServerState {
    pub server_name: String,
    pub token: String,
    pub tx: broadcast::Sender<ServerToClient>,
    pub users: Mutex<HashSet<String>>,
    pub debug: bool,
    pub history: Mutex<VecDeque<String>>,
}
