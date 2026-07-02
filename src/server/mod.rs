pub mod ollama;
pub mod state;

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use futures::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_util::codec::{Framed, LinesCodec};
use std::time::{Duration, Instant};
use crossterm::terminal;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, BorderType},
    Frame, Terminal,
};

use crate::protocol::{ClientToServer, ServerToClient};
use state::{ServerState, StoredFile};

// Thread-safe global server log storage
pub static SERVER_LOGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[macro_export]
macro_rules! server_log {
    ($level:ident, $($arg:tt)*) => {
        let formatted = format!($($arg)*);
        let prefix = stringify!($level);
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        let log_line = format!("[{}] [{}] {}", timestamp, prefix, formatted);
        if let Ok(mut logs) = $crate::server::SERVER_LOGS.lock() {
            logs.push(log_line);
        }
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

pub fn get_hash_color(name: &str) -> Color {
    let mut hash = 0u32;
    for c in name.chars() {
        hash = hash.wrapping_add(c as u32).wrapping_mul(31);
    }
    let colors = [
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
    ];
    colors[(hash as usize) % colors.len()]
}

pub async fn run(name: String, ip: String, port: u16, debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", ip, port);
    let listener = TcpListener::bind(&addr).await?;
    let token = generate_token();

    // Initialize logs
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
        total_messages: std::sync::atomic::AtomicUsize::new(0),
    });



    // Setup TUI mode
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    crossterm::execute!(stdout, terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let startup_time = Instant::now();
    let mut tick_interval = tokio::time::interval(Duration::from_millis(250));

    // Spawn Crossterm event reader thread for server to handle keyboard inputs (Ctrl+C)
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<crossterm::event::Event>(100);
    std::thread::spawn(move || {
        loop {
            match crossterm::event::read() {
                Ok(evt) => {
                    if event_tx.blocking_send(evt).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    loop {
        // Fetch snapshot of state for drawing
        let active_users = {
            let u = state.users.lock().await;
            u.iter().cloned().collect::<Vec<String>>()
        };
        let files_count = {
            let f = state.files.lock().await;
            f.len()
        };
        let chat_history = {
            let h = state.history.lock().await;
            h.iter().cloned().collect::<Vec<String>>()
        };
        let logs = {
            if let Ok(l) = SERVER_LOGS.lock() {
                l.clone()
            } else {
                Vec::new()
            }
        };
        let total_msg = state.total_messages.load(std::sync::atomic::Ordering::Relaxed);

        // Draw Dashboard Frame
        let server_state_clone = Arc::clone(&state);
        let addr_clone = addr.clone();
        terminal.draw(|f| {
            draw_server_ui(
                f,
                &server_state_clone,
                startup_time,
                &active_users,
                files_count,
                &chat_history,
                &logs,
                &addr_clone,
                total_msg,
            );
        })?;

        tokio::select! {
            _ = tick_interval.tick() => {
                // Tick interval redraw
            }

            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, peer_addr)) => {
                        server_log!(Info, "Connection attempt received from peer: {}", peer_addr);

                        let state_clone = Arc::clone(&state);
                        tokio::spawn(async move {
                            if let Err(e) = handle_client(stream, state_clone).await {
                                server_log!(Warn, "Connection with peer {} dropped: {}", peer_addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        server_log!(Error, "Failed to accept connection: {}", e);
                    }
                }
            }

            Some(evt) = event_rx.recv() => {
                if let crossterm::event::Event::Key(key) = evt {
                    if key.code == crossterm::event::KeyCode::Char('c') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
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

            _ = tokio::signal::ctrl_c() => {
                server_log!(Info, "Stopping server (external signal)...");

                let shutdown_alert = ServerToClient::SystemAlert {
                    content: "Server is shutting down...".to_string(),
                    timestamp: chrono::Utc::now(),
                };
                let _ = state.tx.send(shutdown_alert);
                break;
            }
        }
    }

    // Restore terminal
    terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    println!("Server shutdown successfully.");
    Ok(())
}

fn draw_server_ui(
    f: &mut Frame,
    state: &ServerState,
    startup_time: Instant,
    active_users: &[String],
    files_count: usize,
    chat_history: &[String],
    logs: &[String],
    addr: &str,
    total_messages: usize,
) {
    let title_color = Color::Rgb(236, 110, 93); // Sunset Coral theme

    // Layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(5),    // Main Panels
            Constraint::Length(3), // Stats bottom panel
        ])
        .split(f.area());

    // 1. Header Block
    let uptime = format_uptime(startup_time.elapsed());
    let header_text = format!(
        "  TermChat Server Dashboard  |  Host: {}  |  Uptime: {}  |  Secure Token: {}",
        addr, uptime, state.token
    );
    let header = Paragraph::new(Line::from(vec![
        Span::styled(header_text, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(title_color)),
    );
    f.render_widget(header, chunks[0]);

    // 2. Main horizontal panels
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(38), // Chat History
            Constraint::Percentage(44), // Server Logs
            Constraint::Percentage(18), // Connected Users
        ])
        .split(chunks[1]);

    // Chat History Box
    let chat_lines: Vec<ListItem> = chat_history
        .iter()
        .map(|msg| {
            // Trim newlines if any
            let clean_msg = msg.replace("\r\n", " ").replace('\n', " ");
            ListItem::new(Line::from(vec![Span::raw(clean_msg)]))
        })
        .collect();
    let chat_list = List::new(chat_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(title_color))
            .title(" Recent Chat History "),
    );
    f.render_widget(chat_list, main_layout[0]);

    // Server Logs Box
    let log_lines: Vec<ListItem> = logs
        .iter()
        .map(|log| {
            let style = if log.contains("[Error]") {
                Style::default().fg(Color::Red)
            } else if log.contains("[Warn]") {
                Style::default().fg(Color::Yellow)
            } else if log.contains("[Debug]") {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Line::from(vec![Span::styled(log.to_string(), style)]))
        })
        .collect();
    let log_list = List::new(log_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(title_color))
            .title(" Server Event Logs "),
    );
    f.render_widget(log_list, main_layout[1]);

    // Connected Users Box
    let user_items: Vec<ListItem> = active_users
        .iter()
        .map(|user| {
            let color = get_hash_color(user);
            ListItem::new(Line::from(vec![
                Span::styled("• ", Style::default().fg(Color::DarkGray)),
                Span::styled(user.to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD)),
            ]))
        })
        .collect();
    let users_list = List::new(user_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(title_color))
            .title(format!(" Active ({}) ", active_users.len())),
    );
    f.render_widget(users_list, main_layout[2]);

    // 3. Stats Panel
    let stats_text = format!(
        "  Online Users: {}   |   Files Shared: {}   |   Total Messages Exchanged: {}   |   AI Ollama Debug: {}",
        active_users.len(),
        files_count,
        total_messages,
        if state.debug { "ENABLED" } else { "DISABLED" }
    );
    let stats_para = Paragraph::new(Line::from(vec![
        Span::styled(stats_text, Style::default().fg(Color::White)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(title_color))
            .title(" Live Metrics "),
    );
    f.render_widget(stats_para, chunks[2]);
}

fn format_uptime(duration: Duration) -> String {
    let secs = duration.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

use std::io;

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
                                    continue;
                                }

                                server_log!(Info, "User '{}' ran command '/ask' with question: '{}'", username, question);
                                state.total_messages.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                                let broadcast_msg = ServerToClient::Broadcast {
                                    sender: username.clone(),
                                    content: content.clone(),
                                    timestamp: chrono::Utc::now(),
                                    sender_color: state.user_colors.lock().await.get(&username).cloned(),
                                };

                                {
                                    let mut hist = state.history.lock().await;
                                    if hist.len() >= 20 {
                                        hist.pop_front();
                                    }
                                    hist.push_back(format!("{}: {}", username, content));
                                }

                                let _ = state.tx.send(broadcast_msg);

                                let state_clone = Arc::clone(&state);
                                let username_clone = username.clone();
                                tokio::spawn(async move {
                                    ollama::handle_ask(question, username_clone, state_clone).await;
                                });
                            } else {
                                server_log!(Info, "User '{}' sent chat message: '{}'", username, content);
                                state.total_messages.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                                let broadcast_msg = ServerToClient::Broadcast {
                                    sender: username.clone(),
                                    content,
                                    timestamp: chrono::Utc::now(),
                                    sender_color: state.user_colors.lock().await.get(&username).cloned(),
                                };

                                {
                                    let mut hist = state.history.lock().await;
                                    if hist.len() >= 20 {
                                        hist.pop_front();
                                    }
                                    hist.push_back(format!("{}: {}", username, broadcast_msg.clone().content()));
                                }

                                if state.debug {
                                     server_log!(Debug, "Broadcasting from '{}': {:?}", username, broadcast_msg);
                                }
                                let _ = state.tx.send(broadcast_msg);
                            }
                        }
                        ClientToServer::Typing { is_typing } => {
                            let typing_msg = ServerToClient::UserTyping {
                                sender: username.clone(),
                                is_typing,
                            };
                            let _ = state.tx.send(typing_msg);
                        }
                        ClientToServer::Ping => {
                            let pong = ServerToClient::Pong;
                            if let Ok(json) = serde_json::to_string(&pong) {
                                let _ = framed.send(json).await;
                            }
                        }
                        ClientToServer::FileUpload { filename, data } => {
                            let clean_filename = std::path::Path::new(&filename)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("file")
                                .to_string();
                            let safe_name = clean_filename.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "");
                            if let Ok(raw) = B64.decode(&data) {
                                let id = generate_file_id();
                                let size = raw.len();

                                state.files.lock().await.insert(id.clone(), StoredFile {
                                    filename: safe_name.clone(),
                                    data: raw,
                                });

                                server_log!(Info, "User '{}' uploaded '{}' ({} bytes) → id {}", username, safe_name, size, id);

                                let file_alert = ServerToClient::FileAvailable {
                                    id,
                                    filename: safe_name,
                                    size_bytes: size,
                                    sender: username.clone(),
                                    timestamp: chrono::Utc::now(),
                                };
                                let _ = state.tx.send(file_alert);
                            }
                        }
                        ClientToServer::FileRequest { id } => {
                            let file_opt = {
                                let files = state.files.lock().await;
                                files.get(&id).map(|f| (f.filename.clone(), B64.encode(&f.data)))
                            };

                            if let Some((filename, data)) = file_opt {
                                server_log!(Info, "User '{}' downloaded file id {}", username, id);
                                let response = ServerToClient::FileData {
                                    id,
                                    filename,
                                    data,
                                };
                                if let Ok(json) = serde_json::to_string(&response) {
                                    let _ = framed.send(json).await;
                                }
                            } else {
                                server_log!(Warn, "User '{}' requested non-existent file id {}", username, id);
                                let err_alert = ServerToClient::Error {
                                    message: format!("File with ID '{}' not found", id),
                                };
                                if let Ok(json) = serde_json::to_string(&err_alert) {
                                    let _ = framed.send(json).await;
                                }
                            }
                        }
                        ClientToServer::SetColor { color } => {
                            if let Some(ref c) = color {
                                server_log!(Info, "User '{}' set color to '{}'", username, c);
                                state.user_colors.lock().await.insert(username.clone(), c.clone());
                            } else {
                                server_log!(Info, "User '{}' reset their color", username);
                                state.user_colors.lock().await.remove(&username);
                            }

                            // Force list refresh to sync colors
                            let active_users = {
                                let u = state.users.lock().await;
                                u.iter().cloned().collect::<Vec<String>>()
                            };
                            let _ = state.tx.send(ServerToClient::UsersList { users: active_users });
                        }
                        ClientToServer::Handshake { .. } => {
                            // Already handshaked, ignore
                        }
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

// Add content getter helper for Broadcast in mod.rs locally since we need it in server
impl ServerToClient {
    fn content(self) -> String {
        match self {
            ServerToClient::Broadcast { content, .. } => content,
            ServerToClient::Notification { content, .. } => content,
            ServerToClient::SystemAlert { content, .. } => content,
            ServerToClient::Error { message } => message,
            _ => String::new(),
        }
    }
}
