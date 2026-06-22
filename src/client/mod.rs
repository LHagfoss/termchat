pub mod display;
pub mod emoji;
pub mod input;
pub mod theme;

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use colored::Colorize;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LinesCodec};

use crate::protocol::{ClientToServer, ServerToClient};
use display::{
    clear_screen, draw_prompt, handle_incoming_message, print_help, print_info,
    print_welcome_banner, RawModeGuard,
};
use input::InputState;
use theme::THEME_NAMES;

fn get_downloads_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().join(".termchat").join("downloads"))
        .unwrap_or_else(|| PathBuf::from(".termchat/downloads"))
}

fn open_file(path: &PathBuf) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = path;
}

async fn show_help_overlay(
    colors: theme::ThemeColors,
    key_rx: &mut tokio::sync::mpsc::Receiver<Event>,
) {
    let mut stdout = std::io::stdout();
    if crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen, crossterm::cursor::Hide).is_ok() {
        clear_screen();
        print_help(colors);
        print!("\r\n   Press ESC or Enter to close...\r\n");
        let _ = stdout.flush();

        while let Some(evt) = key_rx.recv().await {
            if let Event::Key(key) = evt {
                if key.kind != event::KeyEventKind::Release {
                    if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                        break;
                    }
                }
            }
        }

        let _ = crossterm::execute!(stdout, crossterm::cursor::Show, crossterm::terminal::LeaveAlternateScreen);
    }
}

pub async fn run(
    ip: String,
    port: u16,
    name: String,
    token: String,
    theme_name: String,
    color: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", ip, port);
    println!("Connecting to {}...", addr);

    let stream = TcpStream::connect(&addr).await?;
    let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(16 * 1024 * 1024));

    let handshake = ClientToServer::Handshake {
        name: name.clone(),
        token: token.clone(),
        color,
    };
    let handshake_json = serde_json::to_string(&handshake)?;
    framed.send(handshake_json).await?;

    println!("Authenticating...");

    let server_name = match framed.next().await {
        Some(Ok(line)) => {
            let response: ServerToClient = serde_json::from_str(&line)?;
            match response {
                ServerToClient::Welcome { server_name } => server_name,
                ServerToClient::Error { message } => {
                    eprintln!(
                        "{} Connection rejected: {}",
                        "✖".red().bold(),
                        message.red()
                    );
                    return Ok(());
                }
                _ => {
                    eprintln!("{} Unexpected response from server", "✖".red().bold());
                    return Ok(());
                }
            }
        }
        Some(Err(e)) => {
            eprintln!(
                "{} Failed to read handshake response: {}",
                "✖".red().bold(),
                e
            );
            return Ok(());
        }
        None => {
            eprintln!("{} Connection closed by server", "✖".red().bold());
            return Ok(());
        }
    };

    let colors = theme::ThemeColors::get(&theme_name);
    print_welcome_banner(&server_name, &name, colors);

    let mut input_state = InputState::new(theme_name);
    let mut downloaded_files: HashMap<String, PathBuf> = HashMap::new();
    let mut pending_open: Option<String> = None;

    let (key_tx, mut key_rx) = tokio::sync::mpsc::channel::<Event>(100);
    std::thread::spawn(move || {
        loop {
            match event::read() {
                Ok(evt) => {
                    if key_tx.blocking_send(evt).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let (outbound_tx, mut outbound_rx) = tokio::sync::mpsc::channel::<String>(100);
    let _raw_guard = RawModeGuard::new();
    draw_prompt(&input_state)?;

    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    ping_interval.tick().await;

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                let ping = ClientToServer::Ping;
                if let Ok(json) = serde_json::to_string(&ping) {
                    if framed.send(json).await.is_err() {
                        let _ = terminal::disable_raw_mode();
                        eprintln!("\r\n{} Lost connection to server.", "✖".red().bold());
                        break;
                    }
                }
            }

            Some(json) = outbound_rx.recv() => {
                if framed.send(json).await.is_err() {
                    let _ = terminal::disable_raw_mode();
                    eprintln!("\r\n{} Lost connection to server.", "✖".red().bold());
                    break;
                }
            }

            Some(evt) = key_rx.recv() => {
                match evt {
                    Event::Key(key_event) => {
                        if key_event.kind == event::KeyEventKind::Release {
                            continue;
                        }

                        if key_event.code == KeyCode::Char('?') && input_state.buffer.is_empty() {
                            let current_colors = theme::ThemeColors::get(&input_state.theme_name);
                            show_help_overlay(current_colors, &mut key_rx).await;
                            let _ = draw_prompt(&input_state);
                            continue;
                        }

                        if let Some(cmd) = input_state.handle_key(key_event) {
                            if cmd == "/exit" {
                                let _ = terminal::disable_raw_mode();
                                println!("\r\nDisconnecting from chat...");
                                break;
                            } else if cmd == "/clear" {
                                clear_screen();
                                let _ = draw_prompt(&input_state);
                            } else if cmd == "/refresh" {
                                clear_screen();
                                let current_colors = theme::ThemeColors::get(&input_state.theme_name);
                                print_welcome_banner(&server_name, &name, current_colors);
                                let _ = draw_prompt(&input_state);
                            } else if cmd == "/help" {
                                let current_colors = theme::ThemeColors::get(&input_state.theme_name);
                                show_help_overlay(current_colors, &mut key_rx).await;
                                let _ = draw_prompt(&input_state);
                            } else if cmd == "/info" {
                                let info_colors = theme::ThemeColors::get(&input_state.theme_name);
                                print_info(&ip, port, &name, &server_name, &token, &input_state.theme_name, info_colors);
                                let _ = draw_prompt(&input_state);
                            } else if cmd == "/debug" {
                                input_state.debug = !input_state.debug;
                                let status = if input_state.debug { "enabled" } else { "disabled" };
                                let debug_msg = ServerToClient::SystemAlert {
                                    content: format!("Local client debugging {}", status),
                                    timestamp: chrono::Utc::now(),
                                };
                                handle_incoming_message(debug_msg, &mut input_state, &name);
                            } else if cmd.starts_with("/theme ") {
                                let target_theme = cmd[7..].trim().to_lowercase();
                                if THEME_NAMES.contains(&target_theme.as_str()) {
                                    input_state.theme_name = target_theme.clone();
                                    let _ = crate::config::update_theme(target_theme.clone());
                                    let info_msg = ServerToClient::SystemAlert {
                                        content: format!("Theme changed to '{}' successfully!", target_theme),
                                        timestamp: chrono::Utc::now(),
                                    };
                                    handle_incoming_message(info_msg, &mut input_state, &name);
                                } else {
                                    let error_msg = ServerToClient::Error {
                                        message: format!("Unknown theme '{}'. Options: blurple, matrix, cyberpunk, sunset", target_theme),
                                    };
                                    handle_incoming_message(error_msg, &mut input_state, &name);
                                }
                            } else if cmd.starts_with("/color ") || cmd == "/color" {
                                let target_color = if cmd == "/color" {
                                    "".to_string()
                                } else {
                                    cmd[7..].trim().to_string()
                                };

                                if target_color.is_empty() {
                                    let _ = crate::config::update_color(None);
                                    let set_msg = ClientToServer::SetColor { color: None };
                                    if let Ok(json) = serde_json::to_string(&set_msg) {
                                        let _ = outbound_tx.send(json).await;
                                    }
                                } else {
                                    let _ = crate::config::update_color(Some(target_color.clone()));
                                    let set_msg = ClientToServer::SetColor { color: Some(target_color.clone()) };
                                    if let Ok(json) = serde_json::to_string(&set_msg) {
                                        let _ = outbound_tx.send(json).await;
                                    }
                                }
                                let _ = draw_prompt(&input_state);
                            } else if cmd.starts_with("/ask ") || cmd == "/ask" {
                                let question = if cmd == "/ask" { "".to_string() } else { cmd[5..].trim().to_string() };
                                if question.is_empty() {
                                    let error_msg = ServerToClient::SystemAlert {
                                        content: "Usage: /ask <your question>".to_string(),
                                        timestamp: chrono::Utc::now(),
                                    };
                                    handle_incoming_message(error_msg, &mut input_state, &name);
                                } else {
                                    let chat_msg = ClientToServer::ChatMessage { content: cmd };
                                    if input_state.debug {
                                        let debug_alert = ServerToClient::SystemAlert {
                                            content: format!("[DEBUG] Sent: {:?}", chat_msg),
                                            timestamp: chrono::Utc::now(),
                                        };
                                        handle_incoming_message(debug_alert, &mut input_state, &name);
                                    }
                                    if let Ok(json) = serde_json::to_string(&chat_msg) {
                                        let _ = outbound_tx.send(json).await;
                                    }
                                }
                                let _ = draw_prompt(&input_state);
                            } else if cmd.starts_with("/send ") {
                                let path = cmd[6..].trim().to_string();
                                match std::fs::read(&path) {
                                    Ok(data) => {
                                        const MAX: usize = 10 * 1024 * 1024;
                                        if data.len() > MAX {
                                            let err = ServerToClient::Error {
                                                message: format!("File too large ({}MB > 10MB limit)", data.len() / 1024 / 1024),
                                            };
                                            handle_incoming_message(err, &mut input_state, &name);
                                        } else {
                                            let filename = std::path::Path::new(&path)
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("file")
                                                .to_string();
                                            let msg = ClientToServer::FileUpload {
                                                filename,
                                                data: B64.encode(&data),
                                            };
                                            if let Ok(json) = serde_json::to_string(&msg) {
                                                let _ = outbound_tx.send(json).await;
                                            }
                                            let info = ServerToClient::SystemAlert {
                                                content: format!("Uploading {}...", path),
                                                timestamp: chrono::Utc::now(),
                                            };
                                            handle_incoming_message(info, &mut input_state, &name);
                                        }
                                    }
                                    Err(e) => {
                                        let err = ServerToClient::Error {
                                            message: format!("Cannot read '{}': {}", path, e),
                                        };
                                        handle_incoming_message(err, &mut input_state, &name);
                                    }
                                }
                                let _ = draw_prompt(&input_state);
                            } else if cmd.starts_with("/download ") || cmd.starts_with("/open ") {
                                let (is_open, id_raw) = if cmd.starts_with("/open ") {
                                    (true, cmd[6..].trim().to_uppercase())
                                } else {
                                    (false, cmd[10..].trim().to_uppercase())
                                };

                                if id_raw.is_empty() {
                                    let err = ServerToClient::SystemAlert {
                                        content: "Usage: /download <id> or /open <id>".to_string(),
                                        timestamp: chrono::Utc::now(),
                                    };
                                    handle_incoming_message(err, &mut input_state, &name);
                                } else if is_open {
                                    if let Some(path) = downloaded_files.get(&id_raw) {
                                        open_file(path);
                                    } else {
                                        pending_open = Some(id_raw.clone());
                                        let msg = ClientToServer::FileRequest { id: id_raw };
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            let _ = outbound_tx.send(json).await;
                                        }
                                    }
                                } else {
                                    let msg = ClientToServer::FileRequest { id: id_raw };
                                    if let Ok(json) = serde_json::to_string(&msg) {
                                        let _ = outbound_tx.send(json).await;
                                    }
                                }
                                let _ = draw_prompt(&input_state);
                            } else {
                                let chat_msg = ClientToServer::ChatMessage { content: cmd };
                                if input_state.debug {
                                    let debug_alert = ServerToClient::SystemAlert {
                                        content: format!("[DEBUG] Sent: {:?}", chat_msg),
                                        timestamp: chrono::Utc::now(),
                                    };
                                    handle_incoming_message(debug_alert, &mut input_state, &name);
                                }
                                if let Ok(json) = serde_json::to_string(&chat_msg) {
                                    let _ = outbound_tx.send(json).await;
                                }
                                let _ = draw_prompt(&input_state);
                            }
                        } else {
                            let _ = draw_prompt(&input_state);
                        }
                    }
                    Event::Resize(_, _) => {
                        let _ = draw_prompt(&input_state);
                    }
                    _ => {}
                }
            }

            result = framed.next() => {
                match result {
                    Some(Ok(line)) => {
                        if let Ok(msg) = serde_json::from_str::<ServerToClient>(&line) {
                            if input_state.debug {
                                let debug_alert = ServerToClient::SystemAlert {
                                    content: format!("[DEBUG] Received: {:?}", msg),
                                    timestamp: chrono::Utc::now(),
                                };
                                handle_incoming_message(debug_alert, &mut input_state, &name);
                            }
                            match msg {
                                ServerToClient::Pong => {}
                                ServerToClient::UserTyping { .. } => {}
                                ServerToClient::UsersList { users } => {
                                    input_state.online_users = users;
                                    let _ = draw_prompt(&input_state);
                                }
                                ServerToClient::FileData { ref id, ref filename, ref data } => {
                                    match B64.decode(data) {
                                        Ok(raw) => {
                                            let dir = get_downloads_dir();
                                            let _ = std::fs::create_dir_all(&dir);
                                            let out_path = dir.join(filename);
                                            match std::fs::write(&out_path, &raw) {
                                                Ok(()) => {
                                                    let should_open = pending_open.as_deref() == Some(id.as_str());
                                                    if should_open {
                                                        pending_open = None;
                                                        open_file(&out_path);
                                                    }
                                                    downloaded_files.insert(id.clone(), out_path.clone());
                                                    let alert = ServerToClient::SystemAlert {
                                                        content: format!(
                                                            "Downloaded: {} → {}{}",
                                                            filename,
                                                            out_path.display(),
                                                            if should_open { " (opening...)" } else { "" }
                                                        ),
                                                        timestamp: chrono::Utc::now(),
                                                    };
                                                    handle_incoming_message(alert, &mut input_state, &name);
                                                }
                                                Err(e) => {
                                                    let err = ServerToClient::Error {
                                                        message: format!("Failed to save file: {}", e),
                                                    };
                                                    handle_incoming_message(err, &mut input_state, &name);
                                                }
                                            }
                                        }
                                        Err(_) => {
                                            let err = ServerToClient::Error {
                                                message: "Failed to decode received file data".to_string(),
                                            };
                                            handle_incoming_message(err, &mut input_state, &name);
                                        }
                                    }
                                }
                                _ => {
                                    handle_incoming_message(msg, &mut input_state, &name);
                                }
                            }
                        }
                    }
                    _ => {
                        let _ = terminal::disable_raw_mode();
                        eprintln!("\r\n{} Connection closed by server.", "✖".red().bold());
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
