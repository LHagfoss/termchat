use crate::protocol::{ClientToServer, ServerToClient};
use super::input::InputState;
use super::theme::ThemeColors;

pub fn format_file_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chrono::Local;
use crossterm::event::{self, Event, KeyCode, KeyModifiers, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags};
use crossterm::terminal;
use futures::{SinkExt, StreamExt};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, BorderType},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::codec::{Framed, LinesCodec};

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

fn parse_terminal_color_to_ratatui(color_str: &str) -> Option<Color> {
    let clean = color_str.trim().to_lowercase();
    match clean.as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "brightblack" | "gray" | "grey" => Some(Color::DarkGray),
        "brightred" => Some(Color::LightRed),
        "brightgreen" => Some(Color::LightGreen),
        "brightyellow" => Some(Color::LightYellow),
        "brightblue" => Some(Color::LightBlue),
        "brightmagenta" => Some(Color::LightMagenta),
        "brightcyan" => Some(Color::LightCyan),
        "brightwhite" => Some(Color::White),
        other => {
            let hex = other.strip_prefix('#').unwrap_or(other);
            if hex.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    return Some(Color::Rgb(r, g, b));
                }
            }
            None
        }
    }
}

// Convert a theme RGB tuple to a Ratatui Color
fn to_tui_color(rgb: (u8, u8, u8)) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

// Tokenize message content to highlight mentions of online users or @all
fn parse_content_spans(
    content: &str,
    own_name: &str,
    online_users: &[String],
    accent_color: Color,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let own_lower = own_name.to_lowercase();

    let words: Vec<&str> = content.split(' ').collect();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }

        if word.starts_with('@') && word.len() > 1 {
            let mut username_part = String::new();
            let mut punctuation_part = String::new();
            let mut in_punc = false;
            for c in word.chars().skip(1) {
                if in_punc {
                    punctuation_part.push(c);
                } else if c.is_alphanumeric() || c == '_' || c == '-' {
                    username_part.push(c);
                } else {
                    in_punc = true;
                    punctuation_part.push(c);
                }
            }

            let username_lower = username_part.to_lowercase();
            let is_online = online_users.iter().any(|u| u.to_lowercase() == username_lower) || username_lower == "all";

            if is_online {
                if username_lower == own_lower {
                    spans.push(Span::styled(
                        format!("@{}", username_part),
                        Style::default().fg(accent_color).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ));
                } else {
                    spans.push(Span::styled(
                        format!("@{}", username_part),
                        Style::default().fg(accent_color).add_modifier(Modifier::BOLD),
                    ));
                }
                if !punctuation_part.is_empty() {
                    spans.push(Span::raw(punctuation_part));
                }
                continue;
            }
        }

        spans.push(Span::raw(word.to_string()));
    }
    spans
}

// Format a ServerToClient message into one or more Lines (handles multiline content and splits by \n)
fn format_message_to_lines(
    msg: &ServerToClient,
    own_name: &str,
    online_users: &[String],
    theme_colors: ThemeColors,
) -> Vec<Line<'static>> {
    let accent_color = to_tui_color(theme_colors.accent);

    match msg {
        ServerToClient::Broadcast {
            sender,
            content,
            timestamp,
            sender_color,
        } => {
            let local_time = timestamp.with_timezone(&Local).format("%H:%M").to_string();
            let display_name = if sender.chars().count() > 10 {
                let truncated: String = sender.chars().take(7).collect();
                format!("{}...", truncated)
            } else {
                sender.to_string()
            };

            let sender_style = Style::default().add_modifier(Modifier::BOLD);
            let sender_color_resolved = if let Some(color_str) = sender_color {
                parse_terminal_color_to_ratatui(color_str).unwrap_or_else(|| get_hash_color(&display_name))
            } else {
                get_hash_color(&display_name)
            };

            let mut lines = Vec::new();
            let emoji_rendered = super::emoji::render_emojis(content);
            let raw_lines: Vec<&str> = emoji_rendered.split('\n').collect();

            for (idx, raw_line) in raw_lines.iter().enumerate() {
                if idx == 0 {
                    let mut spans = vec![
                        Span::styled(format!("{} ", local_time), Style::default().fg(Color::DarkGray)),
                        Span::styled(format!("{}: ", display_name), sender_style.fg(sender_color_resolved)),
                    ];
                    spans.extend(parse_content_spans(raw_line, own_name, online_users, accent_color));
                    lines.push(Line::from(spans));
                } else {
                    let indent = 6 + display_name.chars().count() + 2;
                    let mut spans = vec![
                        Span::raw(" ".repeat(indent)),
                    ];
                    spans.extend(parse_content_spans(raw_line, own_name, online_users, accent_color));
                    lines.push(Line::from(spans));
                }
            }
            lines
        }
        ServerToClient::Notification { targets, content, timestamp } => {
            let is_target = targets.iter().any(|t| t.eq_ignore_ascii_case(own_name));
            if !is_target {
                return vec![];
            }
            let local_time = timestamp.with_timezone(&Local).format("%H:%M").to_string();
            let emoji_rendered = super::emoji::render_emojis(content);
            let raw_lines: Vec<&str> = emoji_rendered.split('\n').collect();
            let mut lines = Vec::new();

            for (idx, raw_line) in raw_lines.iter().enumerate() {
                let mut spans = Vec::new();
                if idx == 0 {
                    spans.push(Span::styled(format!("{} ", local_time), Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled("⚡ 📢 ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
                    spans.push(Span::styled(raw_line.to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
                } else {
                    let indent = 6 + 4; // local_time + notification icons
                    spans.push(Span::raw(" ".repeat(indent)));
                    spans.push(Span::styled(raw_line.to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
                }
                lines.push(Line::from(spans));
            }
            lines
        }
        ServerToClient::SystemAlert { content, timestamp } => {
            let local_time = timestamp.with_timezone(&Local).format("%H:%M").to_string();
            let content_rendered = super::emoji::render_emojis(content);
            let raw_lines: Vec<&str> = content_rendered.split('\n').collect();
            let mut lines = Vec::new();

            for (idx, raw_line) in raw_lines.iter().enumerate() {
                let mut spans = Vec::new();
                if idx == 0 {
                    spans.push(Span::styled(format!("{} ", local_time), Style::default().fg(Color::DarkGray)));
                    if raw_line.starts_with("Usage:") || raw_line.starts_with("[Usage]") {
                        let text = raw_line.strip_prefix("[Usage]").unwrap_or(raw_line).strip_prefix("Usage:").unwrap_or(raw_line).trim();
                        spans.push(Span::styled("⚠ Usage: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
                        spans.push(Span::raw(text.to_string()));
                    } else if raw_line.starts_with("Error:") || raw_line.starts_with("[Error]") {
                        let text = raw_line.strip_prefix("[Error]").unwrap_or(raw_line).strip_prefix("Error:").unwrap_or(raw_line).trim();
                        spans.push(Span::styled("✖ Error: ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)));
                        spans.push(Span::styled(text.to_string(), Style::default().fg(Color::Red)));
                    } else if raw_line.starts_with("Info:") || raw_line.starts_with("[Info]") {
                        let text = raw_line.strip_prefix("[Info]").unwrap_or(raw_line).strip_prefix("Info:").unwrap_or(raw_line).trim();
                        spans.push(Span::styled("ℹ Info: ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)));
                        spans.push(Span::raw(text.to_string()));
                    } else if raw_line.starts_with("Success:") || raw_line.starts_with("[Success]") || raw_line.contains("successfully") {
                        let text = raw_line.strip_prefix("[Success]").unwrap_or(raw_line).strip_prefix("Success:").unwrap_or(raw_line).trim();
                        spans.push(Span::styled("✔ ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
                        spans.push(Span::styled(text.to_string(), Style::default().fg(Color::Green)));
                    } else if raw_line.starts_with("Downloaded") {
                        spans.push(Span::styled("✔ ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
                        spans.push(Span::styled(raw_line.to_string(), Style::default().fg(Color::Green)));
                    } else {
                        spans.push(Span::styled("✦ ", Style::default().fg(accent_color).add_modifier(Modifier::BOLD)));
                        spans.push(Span::styled(raw_line.to_string(), Style::default().fg(Color::DarkGray)));
                    }
                } else {
                    let indent = 6 + 2; // local_time + 2 spaces
                    spans.push(Span::raw(" ".repeat(indent)));
                    spans.push(Span::styled(raw_line.to_string(), Style::default().fg(Color::DarkGray)));
                }
                lines.push(Line::from(spans));
            }
            lines
        }
        ServerToClient::Error { message } => {
            let raw_lines: Vec<&str> = message.split('\n').collect();
            let mut lines = Vec::new();
            for (idx, raw_line) in raw_lines.iter().enumerate() {
                let mut spans = Vec::new();
                if idx == 0 {
                    spans.push(Span::styled("✖ Error: ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)));
                } else {
                    spans.push(Span::raw("         ")); // align under "✖ Error: "
                }
                spans.push(Span::styled(raw_line.to_string(), Style::default().fg(Color::Red)));
                lines.push(Line::from(spans));
            }
            lines
        }
        ServerToClient::FileAvailable { id, filename, size_bytes, sender, timestamp } => {
            let local_time = timestamp.with_timezone(&Local).format("%H:%M").to_string();
            let display_name = if sender.chars().count() > 10 {
                let truncated: String = sender.chars().take(7).collect();
                format!("{}...", truncated)
            } else {
                sender.to_string()
            };
            let sender_color = get_hash_color(&display_name);
            let size_str = format_file_size(*size_bytes);
            let filename_emoji = super::emoji::render_emojis(filename);

            vec![Line::from(vec![
                Span::styled(format!("{} ", local_time), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}: ", display_name), Style::default().fg(sender_color).add_modifier(Modifier::BOLD)),
                Span::styled("📎 ", Style::default().fg(accent_color)),
                Span::styled(filename_emoji, Style::default().fg(accent_color).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" ({}) ", size_str), Style::default().fg(Color::DarkGray)),
                Span::raw("— type "),
                Span::styled(format!("/download {}", id), Style::default().fg(accent_color).add_modifier(Modifier::BOLD)),
            ])]
        }
        _ => vec![],
    }
}

// Wrap a Line containing styled Spans into multiple Lines matching max_width
fn wrap_line(line: Line<'static>, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 {
        return vec![line];
    }
    let mut wrapped = Vec::new();
    let mut current_spans = Vec::new();
    let mut current_width = 0;

    for span in line.spans {
        let text = span.content;
        let style = span.style;
        
        let words = text.split_inclusive(' ');
        for word in words {
            let word_width = word.chars().count();
            if current_width + word_width > max_width {
                if !current_spans.is_empty() {
                    wrapped.push(Line::from(current_spans));
                    current_spans = Vec::new();
                    current_width = 0;
                }
                
                if word_width > max_width {
                    let chars: Vec<char> = word.chars().collect();
                    for chunk in chars.chunks(max_width) {
                        let chunk_str: String = chunk.iter().collect();
                        wrapped.push(Line::from(vec![Span::styled(chunk_str, style)]));
                    }
                } else {
                    current_spans.push(Span::styled(word.to_string(), style));
                    current_width = word_width;
                }
            } else {
                current_spans.push(Span::styled(word.to_string(), style));
                current_width += word_width;
            }
        }
    }

    if !current_spans.is_empty() {
        wrapped.push(Line::from(current_spans));
    }

    if wrapped.is_empty() {
        wrapped.push(Line::from(vec![]));
    }

    wrapped
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

    // Initialize raw mode and Alternate Screen
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    crossterm::execute!(stdout, terminal::EnterAlternateScreen, event::EnableMouseCapture)?;
    let _ = crossterm::execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Establish socket connection
    let connect_res = TcpStream::connect(&addr).await;
    let stream = match connect_res {
        Ok(s) => s,
        Err(e) => {
            // Restore terminal before returning error
            let _ = terminal::disable_raw_mode();
            let mut out = io::stdout();
            let _ = crossterm::execute!(out, terminal::LeaveAlternateScreen, event::DisableMouseCapture);
            return Err(e.into());
        }
    };

    let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(16 * 1024 * 1024));

    // Send handshake
    let handshake = ClientToServer::Handshake {
        name: name.clone(),
        token: token.clone(),
        color,
    };
    let handshake_json = serde_json::to_string(&handshake)?;
    framed.send(handshake_json).await?;

    let server_name = match framed.next().await {
        Some(Ok(line)) => {
            let response: ServerToClient = serde_json::from_str(&line)?;
            match response {
                ServerToClient::Welcome { server_name } => server_name,
                ServerToClient::Error { message } => {
                    let _ = terminal::disable_raw_mode();
                    let mut out = io::stdout();
                    let _ = crossterm::execute!(out, terminal::LeaveAlternateScreen, event::DisableMouseCapture);
                    eprintln!("Connection rejected: {}", message);
                    return Ok(());
                }
                _ => {
                    let _ = terminal::disable_raw_mode();
                    let mut out = io::stdout();
                    let _ = crossterm::execute!(out, terminal::LeaveAlternateScreen, event::DisableMouseCapture);
                    eprintln!("Unexpected response from server");
                    return Ok(());
                }
            }
        }
        Some(Err(e)) => {
            let _ = terminal::disable_raw_mode();
            let mut out = io::stdout();
            let _ = crossterm::execute!(out, terminal::LeaveAlternateScreen, event::DisableMouseCapture);
            return Err(e.into());
        }
        None => {
            let _ = terminal::disable_raw_mode();
            let mut out = io::stdout();
            let _ = crossterm::execute!(out, terminal::LeaveAlternateScreen, event::DisableMouseCapture);
            eprintln!("Connection closed by server during handshake");
            return Ok(());
        }
    };

    let mut input_state = InputState::new(theme_name);
    let mut downloaded_files: HashMap<String, PathBuf> = HashMap::new();
    let mut pending_open: Option<String> = None;

    // Start Crossterm event reading thread
    let (event_tx, mut event_rx) = mpsc::channel::<Event>(100);
    std::thread::spawn(move || {
        loop {
            match event::read() {
                Ok(evt) => {
                    if event_tx.blocking_send(evt).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(100);
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    ping_interval.tick().await;

    let mut tick_interval = tokio::time::interval(Duration::from_millis(150));

    // Store variables to sync scrolling with layout calculations
    let mut total_wrapped_lines = 0;
    let mut chat_height_cache = 10;
    let start_time = Instant::now();

    loop {
        // Render UI frame
        let current_theme = ThemeColors::get(&input_state.theme_name);
        let name_clone = name.clone();
        let server_name_clone = server_name.clone();
        let addr_clone = addr.clone();

        // Filter out expired typing users
        let now = Instant::now();
        input_state.typing_users.retain(|_, last_seen| now.duration_since(*last_seen) < Duration::from_secs(4));

        terminal.draw(|f| {
            let (lines_rendered, chat_height) = draw_ui(
                f,
                &input_state,
                current_theme,
                &name_clone,
                &server_name_clone,
                &addr_clone,
                start_time,
            );
            total_wrapped_lines = lines_rendered;
            chat_height_cache = chat_height;
        })?;

        // Clamp scroll_offset in case screen resized or messages deleted
        let max_scroll = total_wrapped_lines.saturating_sub(chat_height_cache);
        input_state.scroll_offset = input_state.scroll_offset.min(max_scroll);

        tokio::select! {
            _ = tick_interval.tick() => {
                // Redraw frame for dot animations
            }

            _ = ping_interval.tick() => {
                let ping = ClientToServer::Ping;
                if let Ok(json) = serde_json::to_string(&ping) {
                    if framed.send(json).await.is_err() {
                        break;
                    }
                }
            }

            _ = tokio::signal::ctrl_c() => {
                // Intercept SIGINT and break out to clean up alternate screen gracefully!
                break;
            }

            Some(json) = outbound_rx.recv() => {
                if framed.send(json).await.is_err() {
                    break;
                }
            }

            Some(evt) = event_rx.recv() => {
                match evt {
                    Event::Key(key_event) => {
                        if key_event.kind == event::KeyEventKind::Release {
                            continue;
                        }

                        // Intercept Ctrl+C to exit client gracefully
                        if key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                            break;
                        }

                        // Intercept F2 or Ctrl+B to toggle sidebar collapse
                        if key_event.code == KeyCode::F(2) || 
                           (key_event.code == KeyCode::Char('b') && key_event.modifiers.contains(KeyModifiers::CONTROL)) {
                            input_state.sidebar_collapsed = !input_state.sidebar_collapsed;
                            let _ = crate::config::update_sidebar_collapsed(input_state.sidebar_collapsed);
                            continue;
                        }

                        // Intercept scrolling keys (PageUp, PageDown, Shift+Up, Shift+Down)
                        if key_event.code == KeyCode::PageUp || 
                           (key_event.code == KeyCode::Up && key_event.modifiers.contains(KeyModifiers::SHIFT)) {
                            input_state.auto_scroll = false;
                            let scroll_amount = if key_event.code == KeyCode::PageUp {
                                (chat_height_cache / 2).max(1)
                            } else {
                                1
                            };
                            let max_scroll = total_wrapped_lines.saturating_sub(chat_height_cache);
                            input_state.scroll_offset = (input_state.scroll_offset + scroll_amount).min(max_scroll);
                            continue;
                        }
                        if key_event.code == KeyCode::PageDown || 
                           (key_event.code == KeyCode::Down && key_event.modifiers.contains(KeyModifiers::SHIFT)) {
                            let scroll_amount = if key_event.code == KeyCode::PageDown {
                                (chat_height_cache / 2).max(1)
                            } else {
                                1
                            };
                            input_state.scroll_offset = input_state.scroll_offset.saturating_sub(scroll_amount);
                            if input_state.scroll_offset == 0 {
                                input_state.auto_scroll = true;
                            }
                            continue;
                        }

                        // Help overlay toggle key ('?' when buffer is empty)
                        if key_event.code == KeyCode::Char('?') && input_state.buffer.is_empty() {
                            input_state.show_help = !input_state.show_help;
                            continue;
                        }

                        // Close help overlay with Esc or Enter if shown
                        if input_state.show_help {
                            if key_event.code == KeyCode::Esc || key_event.code == KeyCode::Enter {
                                input_state.show_help = false;
                            }
                            continue;
                        }

                        // Typing state notification: send Typing(true) on keypress if not already typing
                        let send_typing = !input_state.buffer.is_empty() && key_event.code != KeyCode::Enter;
                        let typing_msg = ClientToServer::Typing { is_typing: send_typing };
                        if let Ok(json) = serde_json::to_string(&typing_msg) {
                            let _ = outbound_tx.send(json).await;
                        }

                        if let Some(cmd) = input_state.handle_key(key_event) {
                            if cmd == "/exit" {
                                break;
                            } else if cmd == "/clear" {
                                input_state.messages.clear();
                                input_state.scroll_offset = 0;
                                input_state.auto_scroll = true;
                            } else if cmd == "/refresh" {
                                input_state.messages.clear();
                                input_state.scroll_offset = 0;
                                input_state.auto_scroll = true;
                            } else if cmd == "/help" {
                                input_state.show_help = true;
                            } else if cmd == "/info" {
                                let alert = ServerToClient::SystemAlert {
                                    content: format!("Connected to {} at {} as {}", server_name, addr, name),
                                    timestamp: chrono::Utc::now(),
                                };
                                input_state.messages.push(alert);
                            } else if cmd == "/debug" {
                                input_state.debug = !input_state.debug;
                                let status = if input_state.debug { "enabled" } else { "disabled" };
                                let debug_msg = ServerToClient::SystemAlert {
                                    content: format!("Local client debugging {}", status),
                                    timestamp: chrono::Utc::now(),
                                };
                                input_state.messages.push(debug_msg);
                            } else if cmd.starts_with("/theme ") {
                                let target_theme = cmd[7..].trim().to_lowercase();
                                if super::theme::THEME_NAMES.contains(&target_theme.as_str()) {
                                    input_state.theme_name = target_theme.clone();
                                    let _ = crate::config::update_theme(target_theme.clone());
                                    let info_msg = ServerToClient::SystemAlert {
                                        content: format!("Theme changed to '{}' successfully!", target_theme),
                                        timestamp: chrono::Utc::now(),
                                    };
                                    input_state.messages.push(info_msg);
                                } else {
                                    let error_msg = ServerToClient::Error {
                                        message: format!("Unknown theme '{}'. Options: blurple, matrix, cyberpunk, sunset, lagos, mint, lavender", target_theme),
                                    };
                                    input_state.messages.push(error_msg);
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
                            } else if cmd.starts_with("/ask ") || cmd == "/ask" {
                                let question = if cmd == "/ask" { "".to_string() } else { cmd[5..].trim().to_string() };
                                if question.is_empty() {
                                    let error_msg = ServerToClient::SystemAlert {
                                        content: "Usage: /ask <your question>".to_string(),
                                        timestamp: chrono::Utc::now(),
                                    };
                                    input_state.messages.push(error_msg);
                                } else {
                                    let chat_msg = ClientToServer::ChatMessage { content: cmd };
                                    if input_state.debug {
                                        let debug_alert = ServerToClient::SystemAlert {
                                            content: format!("[DEBUG] Sent: {:?}", chat_msg),
                                            timestamp: chrono::Utc::now(),
                                        };
                                        input_state.messages.push(debug_alert);
                                    }
                                    if let Ok(json) = serde_json::to_string(&chat_msg) {
                                        let _ = outbound_tx.send(json).await;
                                    }
                                }
                            } else if cmd.starts_with("/send ") {
                                let path = cmd[6..].trim().to_string();
                                match std::fs::read(&path) {
                                    Ok(data) => {
                                        const MAX: usize = 10 * 1024 * 1024;
                                        if data.len() > MAX {
                                            let err = ServerToClient::Error {
                                                message: format!("File too large ({}MB > 10MB limit)", data.len() / 1024 / 1024),
                                            };
                                            input_state.messages.push(err);
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
                                            input_state.messages.push(info);
                                        }
                                    }
                                    Err(e) => {
                                        let err = ServerToClient::Error {
                                            message: format!("Cannot read '{}': {}", path, e),
                                        };
                                        input_state.messages.push(err);
                                    }
                                }
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
                                    input_state.messages.push(err);
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
                            } else {
                                let chat_msg = ClientToServer::ChatMessage { content: cmd };
                                if input_state.debug {
                                    let debug_alert = ServerToClient::SystemAlert {
                                        content: format!("[DEBUG] Sent: {:?}", chat_msg),
                                        timestamp: chrono::Utc::now(),
                                    };
                                    input_state.messages.push(debug_alert);
                                }
                                if let Ok(json) = serde_json::to_string(&chat_msg) {
                                    let _ = outbound_tx.send(json).await;
                                }
                            }
                        }
                    }
                    Event::Mouse(mouse_event) => {
                        if mouse_event.kind == event::MouseEventKind::ScrollUp {
                            input_state.auto_scroll = false;
                            let max_scroll = total_wrapped_lines.saturating_sub(chat_height_cache);
                            input_state.scroll_offset = (input_state.scroll_offset + 1).min(max_scroll);
                        } else if mouse_event.kind == event::MouseEventKind::ScrollDown {
                            input_state.scroll_offset = input_state.scroll_offset.saturating_sub(1);
                            if input_state.scroll_offset == 0 {
                                input_state.auto_scroll = true;
                            }
                        }
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
                                input_state.messages.push(debug_alert);
                            }
                            match msg {
                                ServerToClient::Pong => {}
                                ServerToClient::UserTyping { sender, is_typing } => {
                                    if is_typing {
                                        input_state.typing_users.insert(sender, Instant::now());
                                    } else {
                                        input_state.typing_users.remove(&sender);
                                    }
                                }
                                ServerToClient::UsersList { users } => {
                                    input_state.online_users = users;
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
                                                    input_state.messages.push(alert);
                                                }
                                                Err(e) => {
                                                    let err = ServerToClient::Error {
                                                        message: format!("Failed to save file: {}", e),
                                                    };
                                                    input_state.messages.push(err);
                                                }
                                            }
                                        }
                                        Err(_) => {
                                            let err = ServerToClient::Error {
                                                message: "Failed to decode received file data".to_string(),
                                            };
                                            input_state.messages.push(err);
                                        }
                                    }
                                }
                                _ => {
                                    input_state.messages.push(msg);
                                }
                            }
                        }
                    }
                    _ => {
                        break;
                    }
                }
            }
        }
    }

    terminal::disable_raw_mode()?;
    let _ = crossterm::execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    crossterm::execute!(
        terminal.backend_mut(),
        terminal::LeaveAlternateScreen,
        event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    println!("Disconnected from TermChat TUI.");
    Ok(())
}

fn draw_ui(
    f: &mut Frame,
    state: &InputState,
    theme: ThemeColors,
    name: &str,
    server_name: &str,
    addr: &str,
    start_time: Instant,
) -> (usize, usize) {
    let accent_color = to_tui_color(theme.accent);
    let title_color = to_tui_color(theme.title);
    let prompt_color = to_tui_color(theme.prompt);

    // Calculate height of input box dynamically based on newlines in buffer
    let input_lines = state.buffer.iter().filter(|&&c| c == '\n').count() + 1;
    let input_box_height = (input_lines + 2).min(6) as u16;

    // Layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(5),    // Main panel (Chat + Users)
            Constraint::Length(input_box_height), // Input box
            Constraint::Length(1), // Footer (help/actions)
        ])
        .split(f.area());

    // 1. Header Area
    let version = env!("CARGO_PKG_VERSION");
    let header_text = format!(
        "  Server: {} ({})  |  User: {}  |  Theme: {}  |  Press '?' for Help",
        server_name, addr, name, state.theme_name
    );
    let header = Paragraph::new(Line::from(vec![
        Span::styled(header_text, Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(title_color))
            .title(Line::from(vec![
                Span::styled(" TermChat", Style::default().fg(title_color).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" v{} ", version), Style::default().fg(Color::DarkGray)),
            ])),
    );
    f.render_widget(header, chunks[0]);

    // 2. Main Content (Chat History + Online Users sidebar)
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if state.sidebar_collapsed {
            vec![Constraint::Min(10), Constraint::Length(0)]
        } else {
            vec![Constraint::Min(10), Constraint::Length(25)]
        })
        .split(chunks[1]);

    let chat_area = main_layout[0];

    // Split chat area into content + typing indicator row (no borders on chat)
    let chat_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(chat_area);
    let chat_content_area = chat_split[0];
    let typing_indicator_area = chat_split[1];

    // Horizontal padding inside chat content
    let chat_inner_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2), // Left padding
            Constraint::Min(1),    // Text area
            Constraint::Length(2), // Right padding
        ])
        .split(chat_content_area);

    let text_area = chat_inner_layout[1];
    let chat_width = text_area.width as usize;
    let chat_height = text_area.height as usize;

    // Format & Wrap all lines
    let mut all_wrapped_lines = Vec::new();
    for msg in &state.messages {
        let lines = format_message_to_lines(msg, name, &state.online_users, theme);
        for line in lines {
            let wrapped = wrap_line(line, chat_width);
            all_wrapped_lines.extend(wrapped);
        }
    }

    let total_lines = all_wrapped_lines.len();
    let max_scroll = total_lines.saturating_sub(chat_height);
    let current_scroll = state.scroll_offset.min(max_scroll);

    let start_idx = total_lines.saturating_sub(chat_height + current_scroll);
    let end_idx = total_lines.saturating_sub(current_scroll);
    let visible_lines = all_wrapped_lines[start_idx..end_idx].to_vec();

    // Render chat lines (no borders on chat room)
    let chat_para = Paragraph::new(visible_lines);
    f.render_widget(chat_para, text_area);

    // Draw Scrollbar for Chat
    if total_lines > chat_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("░"))
            .thumb_symbol("█")
            .style(Style::default().fg(accent_color));

        let mut scrollbar_state = ScrollbarState::new(max_scroll + 1).position(max_scroll - current_scroll);
        f.render_stateful_widget(scrollbar, chat_content_area, &mut scrollbar_state);
    }

    // Typing indicator row (below chat content, above input)
    let typing_users: Vec<&String> = state.typing_users.keys().filter(|&u| u.as_str() != name).collect();
    if !typing_users.is_empty() {
        let dots = match (start_time.elapsed().as_millis() / 150) % 4 {
            0 => "",
            1 => ".",
            2 => "..",
            _ => "...",
        };
        let indicator_text = match typing_users.len() {
            1 => format!("  {} is typing{}", typing_users[0], dots),
            2 => format!("  {}, {} are typing{}", typing_users[0], typing_users[1], dots),
            _ => format!("  {} users are typing{}", typing_users.len(), dots),
        };
        let typing_para = Paragraph::new(Line::from(
            Span::styled(indicator_text, Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC))
        ));
        f.render_widget(typing_para, typing_indicator_area);
    }

    // Right Panel: Online Users (rendered only if not collapsed)
    if !state.sidebar_collapsed {
        let mut sidebar_items = Vec::new();
        let mut sorted_users = state.online_users.clone();
        sorted_users.sort();

        for user in sorted_users {
            let user_color = get_hash_color(&user);
            let mut display_str = user.clone();
            
            if user == name {
                display_str = format!("{} (me)", display_str);
            }

            let mut style = Style::default().fg(user_color);
            if user == name {
                style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }
            sidebar_items.push(ListItem::new(Line::from(vec![
                Span::styled("• ", Style::default().fg(Color::DarkGray)),
                Span::styled(display_str, style),
            ])));
        }

        let users_list = List::new(sidebar_items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(title_color))
                .title(format!(" Online ({}) ", state.online_users.len())),
        );
        f.render_widget(users_list, main_layout[1]);
    }

    // 3. Input Box
    let input_str: String = state.buffer.iter().collect();
    let has_newlines = state.buffer.contains(&'\n');

    // Autocomplete/suggestion suffix logic (disabled on multiline inputs to avoid glitches)
    let mut suggestion_suffix = String::new();
    if !has_newlines && !input_str.is_empty() && state.cursor_index == state.buffer.len() {
        if input_str.starts_with('/') {
            if input_str.starts_with("/theme ") {
                let query = &input_str[7..];
                if !query.is_empty() {
                    if let Some(matched_theme) =
                        super::theme::THEME_NAMES.iter().find(|t| t.starts_with(query))
                    {
                        suggestion_suffix = matched_theme[query.len()..].to_string();
                    }
                }
            } else if let Some(matched_cmd) =
                super::theme::COMMANDS.iter().find(|c| c.starts_with(&input_str))
            {
                suggestion_suffix = matched_cmd[input_str.len()..].to_string();
            }
        } else {
            let before_cursor = &state.buffer[..state.cursor_index];
            let word_start = before_cursor
                .iter()
                .rposition(|&c| c == ' ')
                .map_or(0, |pos| pos + 1);
            let word: String = before_cursor[word_start..].iter().collect();
            if word.starts_with('@') {
                let query = &word[1..].to_lowercase();
                if !query.is_empty() {
                    if let Some(matched_user) = state
                        .online_users
                        .iter()
                        .find(|u| u.to_lowercase().starts_with(query))
                    {
                        suggestion_suffix = matched_user[query.len()..].to_string();
                    }
                }
            }
        }
    }

    let mut input_lines: Vec<Line<'static>> = input_str
        .split('\n')
        .map(|s| Line::from(s.to_string()))
        .collect();

    if input_lines.is_empty() {
        input_lines.push(Line::from(""));
    }

    if !suggestion_suffix.is_empty() {
        let last_idx = input_lines.len() - 1;
        let mut spans = input_lines[last_idx].spans.clone();
        spans.push(Span::styled(suggestion_suffix, Style::default().fg(Color::DarkGray)));
        input_lines[last_idx] = Line::from(spans);
    }

    let input_para = Paragraph::new(input_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(prompt_color))
            .title(Span::styled(" Chat Input ", Style::default().fg(prompt_color).add_modifier(Modifier::BOLD))),
    );
    f.render_widget(input_para, chunks[2]);

    // Calculate cursor row and col based on newlines in buffer
    let mut cursor_line = 0;
    let mut cursor_col = 0;
    for (i, &c) in state.buffer.iter().enumerate() {
        if i == state.cursor_index {
            break;
        }
        if c == '\n' {
            cursor_line += 1;
            cursor_col = 0;
        } else {
            cursor_col += 1;
        }
    }

    // Position cursor: starts exactly after the left border (chunks[2].x + 1)
    f.set_cursor_position((
        chunks[2].x + 1 + cursor_col as u16,
        chunks[2].y + 1 + cursor_line as u16,
    ));

    // 4. Footer Area (Quick Actions / Help metadata)
    let footer_text = vec![
        Span::styled(" Esc", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Span::styled(" / ", Style::default().fg(Color::DarkGray)),
        Span::styled("?", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Span::styled(" Help  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Shift+Enter", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Span::styled(" Newline  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled("F2", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Span::styled(" / ", Style::default().fg(Color::DarkGray)),
        Span::styled("Ctrl+B", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" Toggle Sidebar ({})  |  ", if state.sidebar_collapsed { "Collapsed" } else { "Expanded" }), Style::default().fg(Color::DarkGray)),
        Span::styled("PgUp/PgDn", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Span::styled(" Scroll  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Ctrl+C", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Span::styled(" Exit", Style::default().fg(Color::DarkGray)),
    ];
    let footer = Paragraph::new(Line::from(footer_text)).alignment(ratatui::layout::Alignment::Center);
    f.render_widget(footer, chunks[3]);

    // 5. Render Help Overlay Popup if toggled
    if state.show_help {
        draw_help_popup(f, accent_color, title_color);
    }

    (total_lines, chat_height)
}

fn draw_help_popup(f: &mut Frame, accent_color: Color, title_color: Color) {
    let size = f.area();
    let popup_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent_color))
        .title(Span::styled(" TermChat Shortcuts & Commands ", Style::default().fg(title_color).add_modifier(Modifier::BOLD)));

    let mut help_spans = Vec::new();
    for &(cmd, desc) in &[
        ("/help", "Show this help menu"),
        ("/users", "List all online users"),
        ("/send <path>", "Share a file"),
        ("/download <id>", "Download a shared file"),
        ("/open <id>", "Download & open a file natively"),
        ("/clear", "Clear chat history locally"),
        ("/info", "Show connection info"),
        ("/debug", "Toggle local debug mode"),
        ("/ask <query>", "Ask local Ollama AI"),
        ("/exit", "Exit the chat client"),
        ("/color <val>", "Change name color (e.g. red, #ff9900)"),
        ("/theme <name>", "Change color theme"),
        ("Ctrl+C", "Exit the chat client gracefully"),
        ("Shift+Up/Down", "Scroll through chat history (1 line)"),
        ("PageUp/PageDown", "Scroll through chat history (half page)"),
        ("Up/Down", "Navigate command history"),
        ("Shift+Enter", "Insert newline in input (multiline text)"),
        ("F2 / Ctrl+U", "Toggle online users list sidebar"),
        ("Tab", "Autocomplete theme/user/commands/emojis"),
    ] {
        help_spans.push(Line::from(vec![
            Span::styled(format!("• {:<16}", cmd), Style::default().fg(accent_color).add_modifier(Modifier::BOLD)),
            Span::raw(format!(" {}", desc)),
        ]));
    }

    let help_para = Paragraph::new(help_spans).block(popup_block);

    // Center popup in layout
    let popup_area = centered_rect(65, 65, size);
    f.render_widget(Clear, popup_area); // Clear background under popup
    f.render_widget(help_para, popup_area);
}

// Helper to center a rectangular popup inside another rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
