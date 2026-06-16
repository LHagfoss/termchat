pub mod state;

use futures::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::signal;
use tokio::sync::broadcast;
use tokio_util::codec::{Framed, LinesCodec};

use crate::protocol::{ClientToServer, ServerToClient};
use lagos_logger::{Level::*, logger};
use state::ServerState;

pub async fn run(name: String, ip: String, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", ip, port);
    let listener = TcpListener::bind(&addr).await?;

    let token = generate_token();

    logger!(Info, "Server '{}' initialized", name);
    logger!(Info, "Listening on {}", addr);
    logger!(Info, "Secure token: {}", token);

    let (tx, _) = broadcast::channel::<ServerToClient>(100);
    let state = Arc::new(ServerState {
        server_name: name.clone(),
        token,
        tx,
        users: tokio::sync::Mutex::new(HashSet::new()),
    });

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, peer_addr)) => {
                        logger!(Info, "Connection attempt received from {}", peer_addr);

                        let state_clone = Arc::clone(&state);
                        tokio::spawn(async move {
                            if let Err(e) = handle_client(stream, state_clone).await {
                                logger!(Warn, "Connection with {} dropped: {}", peer_addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        logger!(Error, "Failed to accept connection: {}", e);
                    }
                }
            }
            _ = signal::ctrl_c() => {
                logger!(Info, "Stopping server...");
                break;
            }
        }
    }

    logger!(Info, "Success! Stopped server successfully.");
    Ok(())
}

async fn handle_client(
    stream: TcpStream,
    state: Arc<ServerState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut framed = Framed::new(stream, LinesCodec::new());

    let Some(Ok(line)) = framed.next().await else {
        return Err("Client disconnected before sending a handshake".into());
    };

    let client_msg: ClientToServer = serde_json::from_str(&line)?;

    let username = match client_msg {
        ClientToServer::Handshake { name, token } => {
            if token != state.token {
                let err_msg = ServerToClient::Error {
                    message: "Invalid token provided".to_string(),
                };
                let err_json = serde_json::to_string(&err_msg)?;
                let _ = framed.send(err_json).await;
                return Err("Invalid token provided".into());
            }
            name
        }
        _ => {
            let err_msg = ServerToClient::Error {
                message: "First message must be a Handshake".to_string(),
            };
            let err_json = serde_json::to_string(&err_msg)?;
            let _ = framed.send(err_json).await;
            return Err("First message must be a Handshake".into());
        }
    };

    logger!(Info, "'{}' successfully authenticated", username);

    state.users.lock().await.insert(username.clone());

    let welcome_msg = ServerToClient::Welcome {
        server_name: state.server_name.clone(),
    };
    let welcome_json = serde_json::to_string(&welcome_msg)?;
    framed.send(welcome_json).await?;

    let join_alert = ServerToClient::SystemAlert {
        content: format!("{} has joined the chat", username),
        timestamp: chrono::Utc::now(),
    };
    let _ = state.tx.send(join_alert);

    let mut rx = state.tx.subscribe();

    loop {
        tokio::select! {
            result = framed.next() => {
                let Some(Ok(line)) = result else {
                    break;
                };

                if let Ok(ClientToServer::ChatMessage { content }) = serde_json::from_str(&line) {
                    if content.trim() == "/users" {
                        let active_users = state.users.lock().await;
                        let user_list = active_users.iter().cloned().collect::<Vec<_>>().join(", ");

                        let alert = ServerToClient::SystemAlert {
                            content: format!("Online users: {}", user_list),
                            timestamp: chrono::Utc::now(),
                        };

                        if let Ok(json) = serde_json::to_string(&alert) {
                            let _ = framed.send(json).await;
                        }
                        continue;
                    }

                    let broadcast_msg = ServerToClient::Broadcast {
                        sender: username.clone(),
                        content,
                        timestamp: chrono::Utc::now(),
                    };
                    let _ = state.tx.send(broadcast_msg);
                }
            }

            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if let Ok(json) = serde_json::to_string(&msg) {
                            if framed.send(json).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }

    state.users.lock().await.remove(&username);

    logger!(Info, "'{}' disconnected", username);

    let leave_alert = ServerToClient::SystemAlert {
        content: format!("{} has left the chat", username),
        timestamp: chrono::Utc::now(),
    };
    let _ = state.tx.send(leave_alert);

    Ok(())
}

fn generate_token() -> String {
    let charset = "abcdefghijklmnopqrstuvwxyz0123456789";
    let mut token = String::new();
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let mut seed = current_time;
    for _ in 0..6 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let idx = (seed % charset.len() as u128) as usize;
        token.push(charset.chars().nth(idx).unwrap());
    }
    token.to_uppercase()
}
