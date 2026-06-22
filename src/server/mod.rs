pub mod ollama;
pub mod state;

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use colored::Colorize;
use futures::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::signal;
use tokio::sync::broadcast;
use tokio_util::codec::{Framed, LinesCodec};

use crate::protocol::{ClientToServer, ServerToClient};
use state::{ServerState, StoredFile};

#[macro_export]
macro_rules! server_log {
    ($level:ident, $($arg:tt)*) => {
        let prefix = match stringify!($level) {
            "Info" => "Info".green().bold(),
            "Warn" => "Warn".yellow().bold(),
            "Error" => "Error".red().bold(),
            other => other.white().bold(),
        };
        println!("   {} {}", prefix, format!($($arg)*));
    };
}

fn copy_to_clipboard(text: &str) -> bool {
    use std::process::{Command, Stdio};
    use std::io::Write;

    if let Ok(mut child) = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(text.as_bytes()).is_err() {
                return false;
            }
        }
        return child.wait().map(|status| status.success()).unwrap_or(false);
    }
    false
}

pub async fn run(name: String, ip: String, port: u16, debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("   {}", r"████████╗ ██████╗    ███████╗███████╗██████╗ ██╗   ██╗███████╗██████╗ ".truecolor(236, 110, 93).bold());
    println!("   {}", r"╚══██╔══╝██╔════╝    ██╔════╝██╔════╝██╔══██╗██║   ██║██╔════╝██╔══██╗".truecolor(236, 110, 93).bold());
    println!("   {}", r"   ██║   ██║         ███████╗█████╗  ██████╔╝██║   ██║█████╗  ██████╔╝".truecolor(236, 110, 93).bold());
    println!("   {}", r"   ██║   ██║         ╚════██║██╔══╝  ██╔══██╗╚██╗ ██╔╝██╔══╝  ██╔══██╗".truecolor(236, 110, 93).bold());
    println!("   {}", r"   ██║   ╚██████╗    ███████║███████╗██║  ██║ ╚████╔╝ ███████╗██║  ██║".truecolor(236, 110, 93).bold());
    println!("   {}", r"   ╚═╝    ╚═════╝    ╚══════╝╚══════╝╚═╝  ╚═╝  ╚═══╝  ╚══════╝╚═╝  ╚═╝".truecolor(236, 110, 93).bold());
    println!();

    let addr = format!("{}:{}", ip, port);
    let listener = TcpListener::bind(&addr).await?;

    let token = generate_token();

    server_log!(Info, "Server '{}' initialized", name);
    server_log!(Info, "Listening on {}", addr);

    let copied_auto = copy_to_clipboard(&token);
    server_log!(Info, "Secure token: {}{}", token, if copied_auto { " (copied to clipboard)" } else { "" });

    let (tx, _) = broadcast::channel::<ServerToClient>(100);
    let state = Arc::new(ServerState {
        server_name: name.clone(),
        token: token.clone(),
        tx,
        users: tokio::sync::Mutex::new(HashSet::new()),
        debug,
        history: tokio::sync::Mutex::new(std::collections::VecDeque::with_capacity(20)),
        files: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        user_colors: tokio::sync::Mutex::new(std::collections::HashMap::new()),
    });

    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<String>(10);
    let token_clone = token.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            if let Ok(line) = line {
                let trimmed = line.trim();
                if trimmed.eq_ignore_ascii_case("c") || trimmed.eq_ignore_ascii_case("copy") {
                    if input_tx.blocking_send(token_clone.clone()).is_err() {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    });

    loop {
        tokio::select! {
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((stream, peer_addr)) => {
                                server_log!(Info, "Connection attempt received from {}", peer_addr);

                                let state_clone = Arc::clone(&state);
                                tokio::spawn(async move {
                                    if let Err(e) = handle_client(stream, state_clone).await {
                                        server_log!(Warn, "Connection with {} dropped: {}", peer_addr, e);
                                    }
                                });
                            }
                            Err(e) => {
                                server_log!(Error, "Failed to accept connection: {}", e);
                            }
                        }
                    }
                    Some(token_to_copy) = input_rx.recv() => {
                        if copy_to_clipboard(&token_to_copy) {
                            server_log!(Info, "Secure token copied to clipboard successfully!");
                        } else {
                            server_log!(Warn, "Failed to copy token to clipboard.");
                        }
                    }
                    _ = signal::ctrl_c() => {
                        server_log!(Info, "Stopping server...");

                        let shutdown_alert = ServerToClient::SystemAlert {
                            content: "Server is shutting down...".to_string(),
                            timestamp: chrono::Utc::now(),
                        };
                        let _ = state.tx.send(shutdown_alert);

                        break;
                    }
                }
    }

    server_log!(Info, "Success! Stopped server successfully.");
    Ok(())
}

async fn handle_client(
    stream: TcpStream,
    state: Arc<ServerState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(16 * 1024 * 1024));

    let Some(Ok(line)) = framed.next().await else {
        return Err("Client disconnected before sending a handshake".into());
    };

    let client_msg: ClientToServer = serde_json::from_str(&line)?;

    let (username, user_color) = match client_msg {
        ClientToServer::Handshake { name, token, color } => {
            if token != state.token {
                let err_msg = ServerToClient::Error {
                    message: "Invalid token provided".to_string(),
                };
                let err_json = serde_json::to_string(&err_msg)?;
                let _ = framed.send(err_json).await;
                return Err("Invalid token provided".into());
            }
            (name, color)
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

    server_log!(Info, "'{}' successfully authenticated", username);

    state.users.lock().await.insert(username.clone());
    if let Some(ref color) = user_color {
        state.user_colors.lock().await.insert(username.clone(), color.clone());
    }

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

    let active_users = {
        let u = state.users.lock().await;
        u.iter().cloned().collect::<Vec<String>>()
    };
    let _ = state.tx.send(ServerToClient::UsersList { users: active_users });

    loop {
        tokio::select! {
            result = framed.next() => {
                let Some(Ok(line)) = result else {
                    break;
                };

                if let Ok(client_msg) = serde_json::from_str::<ClientToServer>(&line) {
                    if state.debug {
                        server_log!(Debug, "Received from '{}': {:?}", username, client_msg);
                    }

                    match client_msg {
                        ClientToServer::ChatMessage { content } => {
                            if content.trim() == "/users" {
                                server_log!(Info, "User '{}' ran command '/users'", username);
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

                            if content.starts_with("/ask ") || content.trim() == "/ask" {
                                let question = if content.trim() == "/ask" {
                                    "".to_string()
                                } else {
                                    content[5..].trim().to_string()
                                };

                                if question.is_empty() {
                                    server_log!(Info, "User '{}' ran command '/ask' with empty question", username);
                                    let alert = ServerToClient::SystemAlert {
                                        content: "Usage: /ask <your question>".to_string(),
                                        timestamp: chrono::Utc::now(),
                                    };
                                    if let Ok(json) = serde_json::to_string(&alert) {
                                        let _ = framed.send(json).await;
                                    }
                                } else {
                                    server_log!(Info, "User '{}' ran command '/ask' with question: '{}'", username, question);
                                    let ask_display = format!("asked Ollama: \"{}\"", question);
                                    let sender_color = state.user_colors.lock().await.get(&username).cloned();
                                    let broadcast_msg = ServerToClient::Broadcast {
                                        sender: username.clone(),
                                        content: ask_display,
                                        timestamp: chrono::Utc::now(),
                                        sender_color,
                                    };
                                    let _ = state.tx.send(broadcast_msg);

                                    let thinking_msg = ServerToClient::SystemAlert {
                                         content: format!("Info: Ollama is thinking (generating response for '{}')...", username),
                                         timestamp: chrono::Utc::now(),
                                     };
                                     let _ = state.tx.send(thinking_msg);

                                    // Add the ask to history
                                    {
                                        let mut hist = state.history.lock().await;
                                        if hist.len() >= 20 {
                                            hist.pop_front();
                                        }
                                        hist.push_back(format!("{}: asked Ollama: \"{}\"", username, question));
                                    }

                                    tokio::spawn(ollama::handle_ask(question, username.clone(), state.clone()));
                                }
                                continue;
                            }

                            // Add regular message to history
                            {
                                let mut hist = state.history.lock().await;
                                if hist.len() >= 20 {
                                    hist.pop_front();
                                }
                                hist.push_back(format!("{}: {}", username, content));
                            }

                            server_log!(Info, "User '{}' sent chat message: '{}'", username, content);
                             let sender_color = state.user_colors.lock().await.get(&username).cloned();
                             let broadcast_msg = ServerToClient::Broadcast {
                                 sender: username.clone(),
                                 content: content.clone(),
                                 timestamp: chrono::Utc::now(),
                                 sender_color,
                             };
                             if state.debug {
                                 server_log!(Debug, "Broadcasting from '{}': {:?}", username, broadcast_msg);
                             }
                             let _ = state.tx.send(broadcast_msg);

                            // Detect @mentions and send private notifications to targeted users
                            {
                                let online_users = state.users.lock().await.clone();
                                let mentioned_targets: Vec<String> = content.split_whitespace()
                                    .filter_map(|token| {
                                        let chars: Vec<char> = token.chars().collect();
                                        if let Some(at_idx) = chars.iter().position(|&c| c == '@') {
                                            if at_idx > 0 {
                                                return None;
                                            }
                                            if at_idx + 1 < chars.len() && chars[at_idx + 1].is_alphanumeric() {
                                                let leading_ok = chars[..at_idx]
                                                    .iter()
                                                    .all(|&c| !c.is_alphanumeric() && c != '@');
                                                if leading_ok {
                                                    let mut username_chars = Vec::new();
                                                    for &c in &chars[at_idx + 1..] {
                                                        if c.is_alphanumeric() || c == '_' || c == '-' {
                                                            username_chars.push(c);
                                                        } else {
                                                            break;
                                                        }
                                                    }
                                                    return Some(username_chars.into_iter().collect::<String>());
                                                }
                                            }
                                        }
                                        None
                                    })
                                    .filter(|mentioned| {
                                        online_users.iter()
                                            .any(|u| u.eq_ignore_ascii_case(mentioned))
                                            && mentioned != username.as_str()
                                    })
                                    .collect();

                                if !mentioned_targets.is_empty() {
                                    let notification = ServerToClient::Notification {
                                        targets: mentioned_targets,
                                        content: format!("[{}]: {}", username, content),
                                        timestamp: chrono::Utc::now(),
                                    };
                                    let _ = state.tx.send(notification);
                                }
                            }
                        }
                        ClientToServer::Typing { is_typing } => {
                            let broadcast_msg = ServerToClient::UserTyping {
                                sender: username.clone(),
                                is_typing,
                            };
                            let _ = state.tx.send(broadcast_msg);
                        }
                        ClientToServer::Ping => {
                            let pong = ServerToClient::Pong;
                            if let Ok(json) = serde_json::to_string(&pong) {
                                let _ = framed.send(json).await;
                            }
                        }
                        ClientToServer::FileUpload { filename, data } => {
                            const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
                            match B64.decode(&data) {
                                Ok(raw) if raw.len() <= MAX_FILE_BYTES => {
                                    let safe_name = std::path::Path::new(&filename)
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("file")
                                        .to_string();
                                    let size = raw.len();
                                    let id = generate_file_id();
                                    server_log!(Info, "User '{}' uploaded '{}' ({} bytes) → id {}", username, safe_name, size, id);
                                    state.files.lock().await.insert(id.clone(), StoredFile {
                                        filename: safe_name.clone(),
                                        data: raw,
                                    });
                                    let _ = state.tx.send(ServerToClient::FileAvailable {
                                        id,
                                        filename: safe_name,
                                        size_bytes: size,
                                        sender: username.clone(),
                                        timestamp: chrono::Utc::now(),
                                    });
                                }
                                Ok(_) => {
                                    let err = ServerToClient::Error {
                                        message: format!("File too large (max {}MB)", MAX_FILE_BYTES / 1024 / 1024),
                                    };
                                    if let Ok(json) = serde_json::to_string(&err) {
                                        let _ = framed.send(json).await;
                                    }
                                }
                                Err(_) => {
                                    let err = ServerToClient::Error {
                                        message: "Failed to decode file data".to_string(),
                                    };
                                    if let Ok(json) = serde_json::to_string(&err) {
                                        let _ = framed.send(json).await;
                                    }
                                }
                            }
                        }
                        ClientToServer::FileRequest { id } => {
                            let files = state.files.lock().await;
                            if let Some(file) = files.get(&id) {
                                let msg = ServerToClient::FileData {
                                    id: id.clone(),
                                    filename: file.filename.clone(),
                                    data: B64.encode(&file.data),
                                };
                                drop(files);
                                if let Ok(json) = serde_json::to_string(&msg) {
                                    let _ = framed.send(json).await;
                                }
                            } else {
                                drop(files);
                                let err = ServerToClient::Error {
                                    message: format!("File '{}' not found (session may have ended)", id),
                                };
                                if let Ok(json) = serde_json::to_string(&err) {
                                    let _ = framed.send(json).await;
                                }
                            }
                        }
                        ClientToServer::SetColor { color } => {
                            let mut colors = state.user_colors.lock().await;
                            if let Some(ref c) = color {
                                colors.insert(username.clone(), c.clone());
                                server_log!(Info, "User '{}' set color to '{}'", username, c);
                                let info = ServerToClient::SystemAlert {
                                    content: format!("Success: Changed your name color to '{}'", c),
                                    timestamp: chrono::Utc::now(),
                                };
                                if let Ok(json) = serde_json::to_string(&info) {
                                    let _ = framed.send(json).await;
                                }
                            } else {
                                colors.remove(&username);
                                server_log!(Info, "User '{}' reset their color", username);
                                let info = ServerToClient::SystemAlert {
                                    content: "Success: Reset your name color".to_string(),
                                    timestamp: chrono::Utc::now(),
                                };
                                if let Ok(json) = serde_json::to_string(&info) {
                                    let _ = framed.send(json).await;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if state.debug {
                            server_log!(Debug, "Sending to '{}': {:?}", username, msg);
                        }
                        if let Ok(json) = serde_json::to_string(&msg) {
                            if framed.send(json).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        server_log!(Warn, "Connection for '{}' lagged by {} messages", username, skipped);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }

    state.users.lock().await.remove(&username);

    server_log!(Info, "'{}' disconnected", username);

    let leave_alert = ServerToClient::SystemAlert {
        content: format!("{} has left the chat", username),
        timestamp: chrono::Utc::now(),
    };
    let _ = state.tx.send(leave_alert);

    let active_users_after = {
        let u = state.users.lock().await;
        u.iter().cloned().collect::<Vec<String>>()
    };
    let _ = state.tx.send(ServerToClient::UsersList { users: active_users_after });

    Ok(())
}

fn generate_file_id() -> String {
    let charset = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut id = String::new();
    for _ in 0..8 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        id.push(charset.chars().nth((seed % charset.len() as u128) as usize).unwrap());
    }
    id
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
