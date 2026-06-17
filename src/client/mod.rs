use colored::Colorize;
use futures::{SinkExt, StreamExt};
use std::io::{self, Write};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LinesCodec};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal,
};

use crate::protocol::{ClientToServer, ServerToClient};

const COMMANDS: &[&str] = &["/help", "/users", "/clear", "/refresh", "/info", "/exit", "/theme", "/debug", "/ask"];
const THEME_NAMES: &[&str] = &["blurple", "matrix", "cyberpunk", "sunset", "lagos", "mint", "lavender"];

#[derive(Clone, Copy, Debug)]
struct ThemeColors {
    pub title: (u8, u8, u8),
    pub prompt: (u8, u8, u8),
    pub accent: (u8, u8, u8),
}

impl ThemeColors {
    pub fn get(theme_name: &str) -> Self {
        match theme_name.to_lowercase().as_str() {
            "lagos" => Self {
                title: (236, 110, 93),   // Cozy Coral
                prompt: (236, 110, 93),  // Coral
                accent: (236, 110, 93),  // Coral
            },
            "mint" => Self {
                title: (140, 216, 167),  // Soft Mint
                prompt: (140, 216, 167), // Soft Mint
                accent: (140, 216, 167), // Soft Mint
            },
            "lavender" => Self {
                title: (193, 158, 214),  // Soft Lavender
                prompt: (193, 158, 214), // Soft Lavender
                accent: (193, 158, 214), // Soft Lavender
            },
            "matrix" => Self {
                title: (0, 255, 100),
                prompt: (0, 255, 100),
                accent: (100, 255, 150),
            },
            "cyberpunk" => Self {
                title: (255, 0, 127),
                prompt: (255, 0, 127),
                accent: (255, 200, 0),
            },
            "sunset" => Self {
                title: (255, 90, 90),
                prompt: (255, 180, 0),
                accent: (180, 80, 250),
            },
            // "blurple" (default)
            _ => Self {
                title: (114, 137, 218),  // Blurple!
                prompt: (114, 137, 218), // Blurple
                accent: (114, 137, 218), // Blurple
            },
        }
    }
}

fn format_username(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() > 10 {
        let truncated: String = chars.into_iter().take(7).collect();
        format!("{}...", truncated)
    } else {
        name.to_string()
    }
}

fn get_colored_name(name: &str) -> colored::ColoredString {
    let mut hash = 0u32;
    for c in name.chars() {
        hash = hash.wrapping_add(c as u32).wrapping_mul(31);
    }
    let colors = [
        colored::Color::Red,
        colored::Color::Green,
        colored::Color::Yellow,
        colored::Color::Blue,
        colored::Color::Magenta,
        colored::Color::Cyan,
        colored::Color::BrightRed,
        colored::Color::BrightGreen,
        colored::Color::BrightYellow,
        colored::Color::BrightBlue,
        colored::Color::BrightMagenta,
        colored::Color::BrightCyan,
    ];
    let color = colors[(hash as usize) % colors.len()];
    name.color(color).bold()
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Self {
        let _ = terminal::enable_raw_mode();
        RawModeGuard
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn is_fuzzy_match(query: &str, target: &str) -> bool {
    let query_lower = query.to_lowercase();
    let target_lower = target.to_lowercase();
    let mut target_chars = target_lower.chars();
    for q_char in query_lower.chars() {
        match target_chars.position(|t_char| t_char == q_char) {
            Some(_) => {}
            None => return false,
        }
    }
    true
}

struct InputState {
    buffer: Vec<char>,
    cursor_index: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    temp_buffer: Vec<char>,
    show_help: bool,
    tab_matches: Vec<String>,
    tab_index: Option<usize>,
    pre_tab_buffer: Vec<char>,
    pre_tab_cursor: usize,
    tab_word_start: Option<usize>,
    theme_name: String,
    online_users: Vec<String>,
    debug: bool,
}

impl InputState {
    fn new(theme_name: String) -> Self {
        Self {
            buffer: Vec::new(),
            cursor_index: 0,
            history: Vec::new(),
            history_index: None,
            temp_buffer: Vec::new(),
            show_help: false,
            tab_matches: Vec::new(),
            tab_index: None,
            pre_tab_buffer: Vec::new(),
            pre_tab_cursor: 0,
            tab_word_start: None,
            theme_name,
            online_users: Vec::new(),
            debug: false,
        }
    }

    fn handle_key(&mut self, key_event: event::KeyEvent) -> Option<String> {
        if key_event.kind == event::KeyEventKind::Release {
            return None;
        }

        if key_event.code != KeyCode::Tab {
            self.tab_index = None;
            self.tab_matches.clear();
            self.pre_tab_buffer.clear();
            self.pre_tab_cursor = 0;
            self.tab_word_start = None;
        }

        match key_event.code {
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                Some("/exit".to_string())
            }
            KeyCode::Char('l') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                Some("/clear".to_string())
            }
            KeyCode::Char(c) => {
                self.show_help = false;
                if !key_event.modifiers.contains(KeyModifiers::CONTROL) && !key_event.modifiers.contains(KeyModifiers::META) {
                    self.buffer.insert(self.cursor_index, c);
                    self.cursor_index += 1;
                }
                None
            }
            KeyCode::Backspace => {
                self.show_help = false;
                if self.cursor_index > 0 {
                    self.buffer.remove(self.cursor_index - 1);
                    self.cursor_index -= 1;
                }
                None
            }
            KeyCode::Delete => {
                self.show_help = false;
                if self.cursor_index < self.buffer.len() {
                    self.buffer.remove(self.cursor_index);
                }
                None
            }
            KeyCode::Left => {
                if self.cursor_index > 0 {
                    self.cursor_index -= 1;
                }
                None
            }
            KeyCode::Right => {
                if self.cursor_index < self.buffer.len() {
                    self.cursor_index += 1;
                } else {
                    let input_str: String = self.buffer.iter().collect();
                    if !input_str.is_empty() {
                        if input_str.starts_with('/') {
                            if input_str.starts_with("/theme ") {
                                let query = &input_str[7..];
                                if !query.is_empty() {
                                    if let Some(matched_theme) = THEME_NAMES.iter().find(|t| t.starts_with(query)) {
                                        let suffix = &matched_theme[query.len()..];
                                        self.buffer.extend(suffix.chars());
                                        self.cursor_index = self.buffer.len();
                                    }
                                }
                            } else {
                                if let Some(matched_cmd) = COMMANDS.iter().find(|c| c.starts_with(&input_str)) {
                                    let suffix = &matched_cmd[input_str.len()..];
                                    self.buffer.extend(suffix.chars());
                                    self.cursor_index = self.buffer.len();
                                }
                            }
                        } else {
                            let before_cursor = &self.buffer[..self.cursor_index];
                            let word_start = before_cursor.iter().rposition(|&c| c == ' ').map_or(0, |pos| pos + 1);
                            let word: String = before_cursor[word_start..].iter().collect();
                            if word.starts_with('@') {
                                let query = &word[1..].to_lowercase();
                                if !query.is_empty() {
                                    if let Some(matched_user) = self.online_users.iter().find(|u| u.to_lowercase().starts_with(query)) {
                                        let suffix = &matched_user[query.len()..];
                                        self.buffer.extend(suffix.chars());
                                        self.cursor_index = self.buffer.len();
                                    }
                                }
                            }
                        }
                    }
                }
                None
            }
            KeyCode::Home => {
                self.cursor_index = 0;
                None
            }
            KeyCode::End => {
                self.cursor_index = self.buffer.len();
                None
            }
            KeyCode::Up => {
                self.show_help = false;
                if !self.history.is_empty() {
                    if self.history_index.is_none() {
                        self.temp_buffer = self.buffer.clone();
                        let new_idx = self.history.len() - 1;
                        self.history_index = Some(new_idx);
                        self.buffer = self.history[new_idx].chars().collect();
                        self.cursor_index = self.buffer.len();
                    } else if let Some(idx) = self.history_index {
                        if idx > 0 {
                            let new_idx = idx - 1;
                            self.history_index = Some(new_idx);
                            self.buffer = self.history[new_idx].chars().collect();
                            self.cursor_index = self.buffer.len();
                        }
                    }
                }
                None
            }
            KeyCode::Down => {
                self.show_help = false;
                if let Some(idx) = self.history_index {
                    if idx + 1 < self.history.len() {
                        let new_idx = idx + 1;
                        self.history_index = Some(new_idx);
                        self.buffer = self.history[new_idx].chars().collect();
                        self.cursor_index = self.buffer.len();
                    } else {
                        self.history_index = None;
                        self.buffer = self.temp_buffer.clone();
                        self.cursor_index = self.buffer.len();
                    }
                }
                None
            }
            KeyCode::Tab => {
                let input_str: String = self.buffer.iter().collect();
                let current_cursor = self.cursor_index;
                
                if self.tab_matches.is_empty() {
                    if input_str.starts_with("/theme ") {
                        self.pre_tab_buffer = self.buffer.clone();
                        self.pre_tab_cursor = current_cursor;
                        self.tab_word_start = Some(7);
                        let query = &input_str[7..];
                        let matches: Vec<String> = THEME_NAMES.iter()
                            .filter(|t| is_fuzzy_match(query, t))
                            .map(|t| format!("/theme {}", t))
                            .collect();
                        if !matches.is_empty() {
                            self.tab_matches = matches;
                            self.tab_index = Some(0);
                            self.buffer = self.tab_matches[0].chars().collect();
                            self.cursor_index = self.buffer.len();
                        }
                    } else {
                        let before_cursor = &self.buffer[..current_cursor];
                        let word_start = before_cursor.iter().rposition(|&c| c == ' ').map_or(0, |pos| pos + 1);
                        let word: String = before_cursor[word_start..].iter().collect();

                        if word.starts_with('@') {
                            let query = &word[1..];
                            self.pre_tab_buffer = self.buffer.clone();
                            self.pre_tab_cursor = current_cursor;
                            self.tab_word_start = Some(word_start);

                            let matches: Vec<String> = self.online_users.iter()
                                .filter(|u| is_fuzzy_match(query, u))
                                .map(|u| format!("@{}", u))
                                .collect();

                            if !matches.is_empty() {
                                self.tab_matches = matches;
                                self.tab_index = Some(0);

                                let mut new_buf = self.pre_tab_buffer[..word_start].to_vec();
                                new_buf.extend(self.tab_matches[0].chars());
                                new_buf.extend(&self.pre_tab_buffer[current_cursor..]);
                                self.buffer = new_buf;
                                self.cursor_index = word_start + self.tab_matches[0].chars().count();
                            }
                        } else if word.starts_with('/') {
                            self.pre_tab_buffer = self.buffer.clone();
                            self.pre_tab_cursor = current_cursor;
                            self.tab_word_start = Some(word_start);

                            let matches: Vec<String> = COMMANDS.iter()
                                .filter(|cmd| is_fuzzy_match(&word, cmd))
                                .map(|s| s.to_string())
                                .collect();

                            if !matches.is_empty() {
                                self.tab_matches = matches;
                                self.tab_index = Some(0);

                                let mut new_buf = self.pre_tab_buffer[..word_start].to_vec();
                                new_buf.extend(self.tab_matches[0].chars());
                                new_buf.extend(&self.pre_tab_buffer[current_cursor..]);
                                self.buffer = new_buf;
                                self.cursor_index = word_start + self.tab_matches[0].chars().count();
                            }
                        }
                    }
                } else if let (Some(idx), Some(word_start)) = (self.tab_index, self.tab_word_start) {
                    let next_idx = (idx + 1) % self.tab_matches.len();
                    self.tab_index = Some(next_idx);

                    let orig_cursor = self.pre_tab_cursor;
                    if input_str.starts_with("/theme ") {
                        self.buffer = self.tab_matches[next_idx].chars().collect();
                        self.cursor_index = self.buffer.len();
                    } else {
                        let mut new_buf = self.pre_tab_buffer[..word_start].to_vec();
                        new_buf.extend(self.tab_matches[next_idx].chars());
                        new_buf.extend(&self.pre_tab_buffer[orig_cursor..]);
                        self.buffer = new_buf;
                        self.cursor_index = word_start + self.tab_matches[next_idx].chars().count();
                    }
                }
                None
            }
            KeyCode::Esc => {
                self.show_help = false;
                self.tab_index = None;
                self.tab_matches.clear();
                self.pre_tab_buffer.clear();
                self.pre_tab_cursor = 0;
                self.tab_word_start = None;
                None
            }
            KeyCode::Enter => {
                self.show_help = false;
                let content: String = self.buffer.iter().collect();
                self.buffer.clear();
                self.cursor_index = 0;
                self.history_index = None;
                if !content.trim().is_empty() {
                    if self.history.last() != Some(&content) {
                        self.history.push(content.clone());
                    }
                    Some(content)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn get_visible_prompt_and_cursor(
    buffer: &[char],
    cursor_index: usize,
    width: usize,
) -> (String, usize) {
    let max_len = width.saturating_sub(5);
    if max_len == 0 {
        return (String::new(), 0);
    }
    if buffer.len() <= max_len {
        (buffer.iter().collect(), cursor_index)
    } else {
        let half = max_len / 2;
        let start = cursor_index.saturating_sub(half);
        let end = (start + max_len).min(buffer.len());
        let start = if end == buffer.len() {
            buffer.len().saturating_sub(max_len)
        } else {
            start
        };
        let visible_str: String = buffer[start..end].iter().collect();
        let visible_cursor = cursor_index - start;
        (visible_str, visible_cursor)
    }
}

fn print_welcome_banner(server_name: &str, username: &str, colors: ThemeColors) {
    println!();
    println!("{}", r"████████╗███████╗██████╗ ███╗   ███╗ ██████╗██╗  ██╗ █████╗ ████████╗".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
    println!("{}", r"╚══██╔══╝██╔════╝██╔══██╗████╗ ████║██╔════╝██║  ██║██╔══██╗╚══██╔══╝".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
    println!("{}", r"   ██║   █████╗  ██████╔╝██╔████╔██║██║     ███████║███████║   ██║   ".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
    println!("{}", r"   ██║   ██╔══╝  ██╔══██╗██║╚██╔╝██║██║     ██╔══██║██╔══██║   ██║   ".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
    println!("{}", r"   ██║   ███████╗██║  ██║██║ ╚═╝ ██║╚██████╗██║  ██║██║  ██║   ██║   ".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
    println!("{}", r"   ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝ ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝   ╚═╝   ".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
    println!();
    println!(
        "  {} Connected to {} as {}",
        "✓".truecolor(colors.accent.0, colors.accent.1, colors.accent.2).bold(),
        server_name.bold().truecolor(colors.accent.0, colors.accent.1, colors.accent.2),
        username.bold().truecolor(colors.title.0, colors.title.1, colors.title.2)
    );
    println!(
        "  {} Press {} for shortcuts, or type {} to exit.",
        "ℹ".truecolor(colors.title.0, colors.title.1, colors.title.2).bold(),
        "?".truecolor(colors.accent.0, colors.accent.1, colors.accent.2).bold(),
        "/exit".truecolor(colors.accent.0, colors.accent.1, colors.accent.2).bold()
    );
    println!();
}

fn print_help(colors: ThemeColors) {
    print!("{}\r\n", "  ── TermChat Shortcuts & Commands ──".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "/help", "Show this help menu");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "/users", "List all online users");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "/clear", "Clear screen");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "/refresh", "Clear screen & show welcome banner");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "/info", "Show connection info");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "/debug", "Toggle local debug mode");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "/ask <query>", "Ask local Ollama AI");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "/exit", "Exit the chat client");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "/theme <name>", "Change color theme");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "Ctrl+C", "Exit the chat client");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "Ctrl+L", "Clear screen");
    print!("   {}  {:<12} {} \r\n", "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), "Up/Down", "Navigate message history");
    print!("{}\r\n", "  ───────────────────────────────────".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
}

fn print_info(ip: &str, port: u16, name: &str, server_name: &str, token: &str, theme: &str, colors: ThemeColors) {
    print!("{}\r\n", "  ── Connection Info ──".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
    print!("   {:<10} {}\r\n", "User:".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), name);
    print!("   {:<10} {}\r\n", "Server:".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), server_name);
    print!("   {:<10} {}:{}\r\n", "Address:".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), ip, port);
    print!("   {:<10} {}\r\n", "Token:".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), token);
    print!("   {:<10} {}\r\n", "Theme:".truecolor(colors.accent.0, colors.accent.1, colors.accent.2), theme);
    print!("{}\r\n", "  ─────────────────────".truecolor(colors.title.0, colors.title.1, colors.title.2).bold());
}

fn clear_screen() {
    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    );
}

fn highlight_mentions(content: &str, own_username: &str, online_users: &[String], colors: ThemeColors) -> (String, bool) {
    let mut words = Vec::new();
    let mut has_self_mention = false;
    let own_lower = own_username.to_lowercase();

    for token in content.split(' ') {
        let chars: Vec<char> = token.chars().collect();
        if let Some(at_idx) = chars.iter().position(|&c| c == '@') {
            if at_idx + 1 < chars.len() && chars[at_idx + 1].is_alphanumeric() {
                let leading_ok = chars[..at_idx].iter().all(|&c| !c.is_alphanumeric() && c != '@');
                if leading_ok {
                    let leading_part: String = chars[..at_idx].iter().collect();
                    let mut username_chars = Vec::new();
                    let mut punctuation_chars = Vec::new();
                    let mut in_punctuation = false;

                    for &c in &chars[at_idx + 1..] {
                        if in_punctuation {
                            punctuation_chars.push(c);
                        } else if c.is_alphanumeric() || c == '_' || c == '-' {
                            username_chars.push(c);
                        } else {
                            in_punctuation = true;
                            punctuation_chars.push(c);
                        }
                    }

                    let username_part: String = username_chars.into_iter().collect();
                    let punctuation_part: String = punctuation_chars.into_iter().collect();
                    let username_part_lower = username_part.to_lowercase();

                    let is_online = online_users.iter().any(|u| u.to_lowercase() == username_part_lower);

                    if is_online {
                        if username_part_lower == own_lower {
                            has_self_mention = true;
                            let formatted_mention = format!("@{}", username_part)
                                .truecolor(colors.accent.0, colors.accent.1, colors.accent.2)
                                .bold()
                                .underline()
                                .to_string();
                            words.push(format!("{}{}{}", leading_part, formatted_mention, punctuation_part));
                            continue;
                        } else {
                            let formatted_mention = format!("@{}", username_part)
                                .truecolor(colors.accent.0, colors.accent.1, colors.accent.2)
                                .bold()
                                .to_string();
                            words.push(format!("{}{}{}", leading_part, formatted_mention, punctuation_part));
                            continue;
                        }
                    }
                }
            }
        }
        words.push(token.to_string());
    }

    (words.join(" "), has_self_mention)
}

fn print_message(message: &ServerToClient, own_username: &str, online_users: &[String], colors: ThemeColors) {
    match message {
        ServerToClient::Broadcast { sender, content, timestamp } => {
            let local_time = timestamp.with_timezone(&chrono::Local).format("%H:%M");
            let display_name = format_username(sender);
            let colored_name = get_colored_name(&display_name);
            let (highlighted_content, has_self_mention) = highlight_mentions(content, own_username, online_users, colors);
            let normalized = highlighted_content.replace("\r\n", "\n").replace("\n", "\r\n");
            print!(" {} {}: {}\r\n", local_time.to_string().dimmed(), colored_name, normalized);
            if has_self_mention {
                print!("\x07"); // trigger terminal beep
                let _ = io::stdout().flush();
            }
        }
        ServerToClient::SystemAlert { content, .. } => {
            let normalized = content.replace("\r\n", "\n").replace("\n", "\r\n");
            print!(" {} {}\r\n", "✦".truecolor(colors.accent.0, colors.accent.1, colors.accent.2).bold(), normalized.dimmed());
        }
        ServerToClient::Error { message } => {
            let normalized = message.replace("\r\n", "\n").replace("\n", "\r\n");
            print!(" {} {}\r\n", "✖".red().bold(), normalized.red());
        }
        _ => {}
    }
}

fn draw_prompt(state: &InputState) -> Result<(), io::Error> {
    let colors = ThemeColors::get(&state.theme_name);
    let mut stdout = io::stdout();
    let (width, _) = terminal::size().unwrap_or((80, 24));
    let width = width as usize;

    execute!(stdout, cursor::MoveToColumn(0), terminal::Clear(terminal::ClearType::CurrentLine))?;

    let prompt_sym = "> ".truecolor(colors.prompt.0, colors.prompt.1, colors.prompt.2).bold();
    let (visible_buffer, visible_cursor) = get_visible_prompt_and_cursor(&state.buffer, state.cursor_index, width);
    print!("{}{}", prompt_sym, visible_buffer);

    let mut suggestion_suffix = String::new();
    let input_str: String = state.buffer.iter().collect();
    if !input_str.is_empty() && state.cursor_index == state.buffer.len() {
        if input_str.starts_with('/') {
            if input_str.starts_with("/theme ") {
                let query = &input_str[7..];
                if !query.is_empty() {
                    if let Some(matched_theme) = THEME_NAMES.iter().find(|t| t.starts_with(query)) {
                        suggestion_suffix = matched_theme[query.len()..].to_string();
                    }
                }
            } else {
                if let Some(matched_cmd) = COMMANDS.iter().find(|c| c.starts_with(&input_str)) {
                    suggestion_suffix = matched_cmd[input_str.len()..].to_string();
                }
            }
        } else {
            let before_cursor = &state.buffer[..state.cursor_index];
            let word_start = before_cursor.iter().rposition(|&c| c == ' ').map_or(0, |pos| pos + 1);
            let word: String = before_cursor[word_start..].iter().collect();
            if word.starts_with('@') {
                let query = &word[1..].to_lowercase();
                if !query.is_empty() {
                    if let Some(matched_user) = state.online_users.iter().find(|u| u.to_lowercase().starts_with(query)) {
                        suggestion_suffix = matched_user[query.len()..].to_string();
                    }
                }
            }
        }
    }

    if !suggestion_suffix.is_empty() {
        print!("{}", suggestion_suffix.dimmed());
    }

    execute!(stdout, cursor::MoveToColumn((2 + visible_cursor) as u16))?;
    stdout.flush()?;

    Ok(())
}

fn handle_incoming_message(
    msg: ServerToClient,
    input_state: &mut InputState,
    username: &str,
) {
    let colors = ThemeColors::get(&input_state.theme_name);
    let mut stdout = io::stdout();
    
    // Clear the current input line
    let _ = execute!(stdout, cursor::MoveToColumn(0), terminal::Clear(terminal::ClearType::CurrentLine));

    print_message(&msg, username, &input_state.online_users, colors);

    let _ = draw_prompt(input_state);
}

pub async fn run(
    ip: String,
    port: u16,
    name: String,
    token: String,
    theme_name: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", ip, port);
    println!("Connecting to {}...", addr);

    let stream = TcpStream::connect(&addr).await?;
    let mut framed = Framed::new(stream, LinesCodec::new());

    let handshake = ClientToServer::Handshake {
        name: name.clone(),
        token: token.clone(),
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
                    eprintln!("{} Connection rejected: {}", "✖".red().bold(), message.red());
                    return Ok(());
                }
                _ => {
                    eprintln!("{} Unexpected response from server", "✖".red().bold());
                    return Ok(());
                }
            }
        }
        Some(Err(e)) => {
            eprintln!("{} Failed to read handshake response: {}", "✖".red().bold(), e);
            return Ok(());
        }
        None => {
            eprintln!("{} Connection closed by server", "✖".red().bold());
            return Ok(());
        }
    };

    let colors = ThemeColors::get(&theme_name);
    print_welcome_banner(&server_name, &name, colors);

    let mut input_state = InputState::new(theme_name);

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
                            clear_screen();
                            let current_colors = ThemeColors::get(&input_state.theme_name);
                            print_help(current_colors);
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
                                let current_colors = ThemeColors::get(&input_state.theme_name);
                                print_welcome_banner(&server_name, &name, current_colors);
                                let _ = draw_prompt(&input_state);
                            } else if cmd == "/help" {
                                clear_screen();
                                let current_colors = ThemeColors::get(&input_state.theme_name);
                                print_help(current_colors);
                                let _ = draw_prompt(&input_state);
                            } else if cmd == "/info" {
                                let info_colors = ThemeColors::get(&input_state.theme_name);
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
                            } else if cmd.starts_with("/ask ") || cmd == "/ask" {
                                let question = if cmd == "/ask" {
                                    "".to_string()
                                } else {
                                    cmd[5..].trim().to_string()
                                };

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
                                ServerToClient::Pong => {
                                    // Ignore heartbeat responses
                                }
                                ServerToClient::UserTyping { .. } => {
                                    // Ignore typing indicators
                                }
                                ServerToClient::UsersList { users } => {
                                    input_state.online_users = users;
                                    let _ = draw_prompt(&input_state);
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
