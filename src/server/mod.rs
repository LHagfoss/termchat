pub mod state;

use colored::Colorize;
use futures::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::signal;
use tokio::sync::broadcast;
use tokio_util::codec::{Framed, LinesCodec};

use crate::protocol::{ClientToServer, ServerToClient};
use state::ServerState;

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

    server_log!(Info, "'{}' successfully authenticated", username);

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
                                    let broadcast_msg = ServerToClient::Broadcast {
                                        sender: username.clone(),
                                        content: ask_display,
                                        timestamp: chrono::Utc::now(),
                                    };
                                    let _ = state.tx.send(broadcast_msg);

                                    let tx = state.tx.clone();
                                    let user_log = username.clone();
                                    tokio::spawn(async move {
                                        let ollama_api = std::env::var("OLLAMA_API_URL")
                                            .unwrap_or_else(|_| "http://192.168.1.254:11434/v1".to_string());
                                        let client = reqwest::Client::new();
                                        
                                        let models_url = format!("{}/models", ollama_api.trim_end_matches('/'));
                                        let model = match client.get(&models_url).send().await {
                                            Ok(resp) => {
                                                #[derive(serde::Deserialize)]
                                                struct ModelData { id: String }
                                                #[derive(serde::Deserialize)]
                                                struct ModelsResponse { data: Vec<ModelData> }
                                                if let Ok(models) = resp.json::<ModelsResponse>().await {
                                                    models.data.first().map(|m| m.id.clone()).unwrap_or_else(|| "llama3".to_string())
                                                } else {
                                                    "llama3".to_string()
                                                }
                                            }
                                            Err(_) => "llama3".to_string(),
                                        };

                                        #[derive(serde::Serialize)]
                                        struct Message {
                                            role: String,
                                            content: String,
                                        }
                                        #[derive(serde::Serialize)]
                                        struct ChatRequest {
                                            model: String,
                                            messages: Vec<Message>,
                                        }

                                        let completions_url = format!("{}/chat/completions", ollama_api.trim_end_matches('/'));
                                        let req_body = ChatRequest {
                                            model: model.clone(),
                                            messages: vec![
                                                Message {
                                                    role: "system".to_string(),
                                                    content: "You are a helpful chat assistant inside a terminal chatroom. Keep your response brief, max 200 characters or words. Do not output any <think> tags or internal thinking steps; respond directly and concisely.".to_string(),
                                                },
                                                Message {
                                                    role: "user".to_string(),
                                                    content: question,
                                                }
                                            ],
                                        };

                                        match client.post(&completions_url)
                                            .json(&req_body)
                                            .send()
                                            .await 
                                        {
                                            Ok(resp) => {
                                                #[derive(serde::Deserialize)]
                                                struct ChatChoice {
                                                    message: MessageContent,
                                                }
                                                #[derive(serde::Deserialize)]
                                                struct MessageContent {
                                                    content: String,
                                                }
                                                #[derive(serde::Deserialize)]
                                                struct ChatResponse {
                                                    choices: Vec<ChatChoice>,
                                                }

                                                if let Ok(chat_resp) = resp.json::<ChatResponse>().await {
                                                    if let Some(choice) = chat_resp.choices.first() {
                                                        let reply = choice.message.content.trim();
                                                        server_log!(Info, "Ollama successfully answered to '{}' using model '{}'", user_log, model);
                                                        let response_content = format!("🤖 [Ollama ({})]:\n{}", model, reply);
                                                        let response_msg = ServerToClient::Broadcast {
                                                            sender: "🤖 Ollama".to_string(),
                                                            content: response_content,
                                                            timestamp: chrono::Utc::now(),
                                                        };
                                                        let _ = tx.send(response_msg);
                                                    } else {
                                                        server_log!(Warn, "Ollama query for '{}' returned empty choices using model '{}'", user_log, model);
                                                        let error_msg = ServerToClient::Broadcast {
                                                            sender: "🤖 Ollama".to_string(),
                                                            content: "Error: No completion choices returned from model.".to_string(),
                                                            timestamp: chrono::Utc::now(),
                                                        };
                                                        let _ = tx.send(error_msg);
                                                    }
                                                } else {
                                                    server_log!(Error, "Ollama query for '{}' failed to parse response using model '{}'", user_log, model);
                                                    let error_msg = ServerToClient::Broadcast {
                                                        sender: "🤖 Ollama".to_string(),
                                                        content: "Error: Failed to parse response from Ollama API.".to_string(),
                                                        timestamp: chrono::Utc::now(),
                                                    };
                                                    let _ = tx.send(error_msg);
                                                }
                                            }
                                            Err(e) => {
                                                server_log!(Error, "Ollama query for '{}' failed to connect using model '{}': {}", user_log, model, e);
                                                let error_msg = ServerToClient::Broadcast {
                                                    sender: "🤖 Ollama".to_string(),
                                                    content: format!("Error: Failed to connect to Ollama: {}", e),
                                                    timestamp: chrono::Utc::now(),
                                                };
                                                let _ = tx.send(error_msg);
                                            }
                                        }
                                    });
                                }
                                continue;
                            }

                            server_log!(Info, "User '{}' sent chat message: '{}'", username, content);
                            let broadcast_msg = ServerToClient::Broadcast {
                                sender: username.clone(),
                                content,
                                timestamp: chrono::Utc::now(),
                            };
                            if state.debug {
                                server_log!(Debug, "Broadcasting from '{}': {:?}", username, broadcast_msg);
                            }
                            let _ = state.tx.send(broadcast_msg);
                        }
                        ClientToServer::Typing { is_typing } => {
                            let broadcast_msg = ServerToClient::UserTyping {
                                sender: username.clone(),
                                is_typing,
                            };
                            let _ = state.tx.send(broadcast_msg);
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
