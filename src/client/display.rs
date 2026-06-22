use colored::Colorize;
use crossterm::{cursor, execute, terminal};
use std::io::{self, Write};

use crate::protocol::ServerToClient;
use super::input::{get_visible_prompt_and_cursor, InputState};
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

pub fn format_username(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() > 10 {
        let truncated: String = chars.into_iter().take(7).collect();
        format!("{}...", truncated)
    } else {
        name.to_string()
    }
}

pub fn get_colored_name(name: &str) -> colored::ColoredString {
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

pub fn parse_terminal_color(color_str: &str) -> Option<colored::Color> {
    let clean = color_str.trim().to_lowercase();
    match clean.as_str() {
        "black" => Some(colored::Color::Black),
        "red" => Some(colored::Color::Red),
        "green" => Some(colored::Color::Green),
        "yellow" => Some(colored::Color::Yellow),
        "blue" => Some(colored::Color::Blue),
        "magenta" => Some(colored::Color::Magenta),
        "cyan" => Some(colored::Color::Cyan),
        "white" => Some(colored::Color::White),
        "brightblack" | "gray" | "grey" => Some(colored::Color::BrightBlack),
        "brightred" => Some(colored::Color::BrightRed),
        "brightgreen" => Some(colored::Color::BrightGreen),
        "brightyellow" => Some(colored::Color::BrightYellow),
        "brightblue" => Some(colored::Color::BrightBlue),
        "brightmagenta" => Some(colored::Color::BrightMagenta),
        "brightcyan" => Some(colored::Color::BrightCyan),
        "brightwhite" => Some(colored::Color::BrightWhite),
        other => {
            let hex = other.strip_prefix('#').unwrap_or(other);
            if hex.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    return Some(colored::Color::TrueColor { r, g, b });
                }
            }
            None
        }
    }
}

pub struct RawModeGuard;

impl RawModeGuard {
    pub fn new() -> Self {
        let _ = terminal::enable_raw_mode();
        RawModeGuard
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub fn highlight_mentions(
    content: &str,
    own_username: &str,
    online_users: &[String],
    colors: ThemeColors,
) -> (String, bool) {
    let mut words = Vec::new();
    let mut has_self_mention = false;
    let own_lower = own_username.to_lowercase();

    for token in content.split(' ') {
        let chars: Vec<char> = token.chars().collect();
        if let Some(at_idx) = chars.iter().position(|&c| c == '@') {
            if at_idx + 1 < chars.len() && chars[at_idx + 1].is_alphanumeric() {
                let leading_ok = chars[..at_idx]
                    .iter()
                    .all(|&c| !c.is_alphanumeric() && c != '@');
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

                    let is_online = online_users
                        .iter()
                        .any(|u| u.to_lowercase() == username_part_lower);

                    if is_online {
                        if username_part_lower == own_lower {
                            has_self_mention = true;
                            let formatted_mention = format!("@{}", username_part)
                                .truecolor(colors.accent.0, colors.accent.1, colors.accent.2)
                                .bold()
                                .underline()
                                .to_string();
                            words.push(format!(
                                "{}{}{}",
                                leading_part, formatted_mention, punctuation_part
                            ));
                            continue;
                        } else {
                            let formatted_mention = format!("@{}", username_part)
                                .truecolor(colors.accent.0, colors.accent.1, colors.accent.2)
                                .bold()
                                .to_string();
                            words.push(format!(
                                "{}{}{}",
                                leading_part, formatted_mention, punctuation_part
                            ));
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

pub fn print_welcome_banner(server_name: &str, username: &str, colors: ThemeColors) {
    println!();
    println!(
        "{}",
        r"████████╗███████╗██████╗ ███╗   ███╗ ██████╗██╗  ██╗ █████╗ ████████╗"
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
    println!(
        "{}",
        r"╚══██╔══╝██╔════╝██╔══██╗████╗ ████║██╔════╝██║  ██║██╔══██╗╚══██╔══╝"
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
    println!(
        "{}",
        r"   ██║   █████╗  ██████╔╝██╔████╔██║██║     ███████║███████║   ██║   "
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
    println!(
        "{}",
        r"   ██║   ██╔══╝  ██╔══██╗██║╚██╔╝██║██║     ██╔══██║██╔══██║   ██║   "
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
    println!(
        "{}",
        r"   ██║   ███████╗██║  ██║██║ ╚═╝ ██║╚██████╗██║  ██║██║  ██║   ██║   "
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
    println!(
        "{}",
        r"   ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝ ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝   ╚═╝   "
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
    println!();
    println!(
        "  {} Connected to {} as {}",
        "✓"
            .truecolor(colors.accent.0, colors.accent.1, colors.accent.2)
            .bold(),
        server_name
            .bold()
            .truecolor(colors.accent.0, colors.accent.1, colors.accent.2),
        username
            .bold()
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
    );
    println!(
        "  {} Press {} for shortcuts, or type {} to exit.",
        "ℹ"
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold(),
        "?".truecolor(colors.accent.0, colors.accent.1, colors.accent.2)
            .bold(),
        "/exit"
            .truecolor(colors.accent.0, colors.accent.1, colors.accent.2)
            .bold()
    );
    println!();
}

pub fn print_help(colors: ThemeColors) {
    print!(
        "{}\r\n",
        "  ── TermChat Shortcuts & Commands ──"
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
    for (cmd, desc) in [
        ("/help", "Show this help menu"),
        ("/users", "List all online users"),
        ("/send <path>", "Share a file"),
        ("/download <id>", "Download a shared file"),
        ("/open <id>", "Download & open a file natively"),
        ("/clear", "Clear screen"),
        ("/refresh", "Clear screen & show welcome banner"),
        ("/info", "Show connection info"),
        ("/debug", "Toggle local debug mode"),
        ("/ask <query>", "Ask local Ollama AI"),
        ("/exit", "Exit the chat client"),
        ("/color <val>", "Change name color (e.g., red, #ff9900)"),
        ("/theme <name>", "Change color theme"),
        ("Ctrl+C", "Exit the chat client"),
        ("Ctrl+L", "Clear screen"),
        ("Up/Down", "Navigate message history"),
    ] {
        print!(
            "   {}  {:<12} {} \r\n",
            "•".truecolor(colors.accent.0, colors.accent.1, colors.accent.2),
            cmd,
            desc
        );
    }
    print!(
        "{}\r\n",
        "  ───────────────────────────────────"
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
}

pub fn print_info(
    ip: &str,
    port: u16,
    name: &str,
    server_name: &str,
    token: &str,
    theme: &str,
    colors: ThemeColors,
) {
    print!(
        "{}\r\n",
        "  ── Connection Info ──"
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
    for (label, value) in [
        ("User:", name.to_string()),
        ("Server:", server_name.to_string()),
        ("Address:", format!("{}:{}", ip, port)),
        ("Token:", token.to_string()),
        ("Theme:", theme.to_string()),
    ] {
        print!(
            "   {:<10} {}\r\n",
            label.truecolor(colors.accent.0, colors.accent.1, colors.accent.2),
            value
        );
    }
    print!(
        "{}\r\n",
        "  ─────────────────────"
            .truecolor(colors.title.0, colors.title.1, colors.title.2)
            .bold()
    );
}

pub fn clear_screen() {
    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    );
}

pub fn print_message(
    message: &ServerToClient,
    own_username: &str,
    online_users: &[String],
    colors: ThemeColors,
) {
    match message {
        ServerToClient::Broadcast {
            sender,
            content,
            timestamp,
            sender_color,
        } => {
            let local_time = timestamp.with_timezone(&chrono::Local).format("%H:%M");
            let display_name = format_username(sender);
            let colored_name = if let Some(color_str) = sender_color {
                if let Some(parsed_color) = parse_terminal_color(color_str) {
                    display_name.color(parsed_color).bold()
                } else {
                    get_colored_name(&display_name)
                }
            } else {
                get_colored_name(&display_name)
            };
            let (highlighted_content, has_self_mention) =
                highlight_mentions(content, own_username, online_users, colors);
            let emoji_rendered = super::emoji::render_emojis(&highlighted_content);
            let normalized = emoji_rendered
                .replace("\r\n", "\n")
                .replace('\n', "\r\n");
            print!(
                " {} {}: {}\r\n",
                local_time.to_string().dimmed(),
                colored_name,
                normalized
            );
            if has_self_mention {
                print!("\x07");
                let _ = io::stdout().flush();
            }
        }
        ServerToClient::Notification { targets, content, timestamp } => {
            // Only display if the current user is mentioned
            let is_target = targets.iter()
                .any(|t| t.eq_ignore_ascii_case(own_username));
            if !is_target {
                return;
            }
            let local_time = timestamp.with_timezone(&chrono::Local).format("%H:%M");
            let emoji_rendered = super::emoji::render_emojis(content);
            print!(
                "\r\n {} {} 📢 {}\r\n",
                local_time.to_string().dimmed(),
                "⚡".yellow().bold(),
                emoji_rendered.yellow().bold()
            );
            print!("\x07");
            let _ = io::stdout().flush();
        }
        ServerToClient::SystemAlert { content, .. } => {
            let normalized = content.replace("\r\n", "\n").replace('\n', "\r\n");
            if normalized.starts_with("Usage:") || normalized.starts_with("[Usage]") {
                let text = normalized.strip_prefix("[Usage]").unwrap_or(&normalized).strip_prefix("Usage:").unwrap_or(&normalized).trim();
                print!(
                    " {} {}: {}\r\n",
                    "⚠".yellow().bold(),
                    "Usage".yellow().bold(),
                    text
                );
            } else if normalized.starts_with("Error:") || normalized.starts_with("[Error]") {
                let text = normalized.strip_prefix("[Error]").unwrap_or(&normalized).strip_prefix("Error:").unwrap_or(&normalized).trim();
                print!(
                    " {} {}: {}\r\n",
                    "✖".red().bold(),
                    "Error".red().bold(),
                    text.red()
                );
            } else if normalized.starts_with("Info:") || normalized.starts_with("[Info]") {
                let text = normalized.strip_prefix("[Info]").unwrap_or(&normalized).strip_prefix("Info:").unwrap_or(&normalized).trim();
                print!(
                    " {} {}: {}\r\n",
                    "ℹ".blue().bold(),
                    "Info".blue().bold(),
                    text
                );
            } else if normalized.starts_with("Success:") || normalized.starts_with("[Success]") || normalized.contains("successfully") {
                let text = normalized.strip_prefix("[Success]").unwrap_or(&normalized).strip_prefix("Success:").unwrap_or(&normalized).trim();
                print!(
                    " {} {}\r\n",
                    "✔".green().bold(),
                    text.green()
                );
            } else if normalized.starts_with("Downloaded") || normalized.starts_with("[Downloaded]") || normalized.contains("Downloaded") {
                print!(
                    " {} {}\r\n",
                    "✔".green().bold(),
                    normalized.green()
                );
            } else {
                print!(
                    " {} {}\r\n",
                    "✦"
                        .truecolor(colors.accent.0, colors.accent.1, colors.accent.2)
                        .bold(),
                    normalized.dimmed()
                );
            }
        }
        ServerToClient::Error { message } => {
            let normalized = message.replace("\r\n", "\n").replace('\n', "\r\n");
            print!(
                " {} {}: {}\r\n",
                "✖".red().bold(),
                "Error".red().bold(),
                normalized.red()
            );
        }
        ServerToClient::FileAvailable { id, filename, size_bytes, sender, timestamp } => {
            let emoji_filename = super::emoji::render_emojis(filename);
            let local_time = timestamp.with_timezone(&chrono::Local).format("%H:%M");
            let display_name = format_username(sender);
            let colored_name = get_colored_name(&display_name);
            let size_str = format_file_size(*size_bytes);
            print!(
                " {} {}: {} {} {} — /download {}\r\n",
                local_time.to_string().dimmed(),
                colored_name,
                "📎".truecolor(colors.accent.0, colors.accent.1, colors.accent.2),
                emoji_filename.truecolor(colors.accent.0, colors.accent.1, colors.accent.2).bold(),
                size_str.dimmed(),
                id.truecolor(colors.accent.0, colors.accent.1, colors.accent.2).bold(),
            );
        }
        _ => {}
    }
}

pub fn draw_prompt(state: &InputState) -> Result<(), io::Error> {
    use super::theme::ThemeColors;
    let colors = ThemeColors::get(&state.theme_name);
    let mut stdout = io::stdout();
    let (width, _) = terminal::size().unwrap_or((80, 24));
    let width = width as usize;

    execute!(
        stdout,
        cursor::MoveToColumn(0),
        terminal::Clear(terminal::ClearType::CurrentLine)
    )?;

    let prompt_sym = "> "
        .truecolor(colors.prompt.0, colors.prompt.1, colors.prompt.2)
        .bold();
    let (visible_buffer, visible_cursor) =
        get_visible_prompt_and_cursor(&state.buffer, state.cursor_index, width);
    print!("{}{}", prompt_sym, visible_buffer);

    let mut suggestion_suffix = String::new();
    let input_str: String = state.buffer.iter().collect();
    if !input_str.is_empty() && state.cursor_index == state.buffer.len() {
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

    if !suggestion_suffix.is_empty() {
        print!("{}", suggestion_suffix.dimmed());
    }

    execute!(stdout, cursor::MoveToColumn((2 + visible_cursor) as u16))?;
    stdout.flush()?;

    Ok(())
}

pub fn handle_incoming_message(
    msg: ServerToClient,
    input_state: &mut InputState,
    username: &str,
) {
    let colors = ThemeColors::get(&input_state.theme_name);
    let mut stdout = io::stdout();

    let _ = execute!(
        stdout,
        cursor::MoveToColumn(0),
        terminal::Clear(terminal::ClearType::CurrentLine)
    );

    print_message(&msg, username, &input_state.online_users, colors);

    let _ = draw_prompt(input_state);
}
