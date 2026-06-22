use crossterm::event::{self, KeyCode, KeyModifiers};

use super::theme::{COMMANDS, THEME_NAMES};
use super::emoji;

pub fn is_fuzzy_match(query: &str, target: &str) -> bool {
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

pub fn get_visible_prompt_and_cursor(
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

pub struct InputState {
    pub buffer: Vec<char>,
    pub cursor_index: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub temp_buffer: Vec<char>,
    pub show_help: bool,
    pub tab_matches: Vec<String>,
    pub tab_index: Option<usize>,
    pub pre_tab_buffer: Vec<char>,
    pub pre_tab_cursor: usize,
    pub tab_word_start: Option<usize>,
    pub theme_name: String,
    pub online_users: Vec<String>,
    pub debug: bool,
}

impl InputState {
    pub fn new(theme_name: String) -> Self {
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

    pub fn handle_key(&mut self, key_event: event::KeyEvent) -> Option<String> {
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
                if !key_event.modifiers.contains(KeyModifiers::CONTROL)
                    && !key_event.modifiers.contains(KeyModifiers::META)
                {
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
                                    if let Some(matched_theme) =
                                        THEME_NAMES.iter().find(|t| t.starts_with(query))
                                    {
                                        let suffix = &matched_theme[query.len()..];
                                        self.buffer.extend(suffix.chars());
                                        self.cursor_index = self.buffer.len();
                                    }
                                }
                            } else {
                                if let Some(matched_cmd) =
                                    COMMANDS.iter().find(|c| c.starts_with(&input_str))
                                {
                                    let suffix = &matched_cmd[input_str.len()..];
                                    self.buffer.extend(suffix.chars());
                                    self.cursor_index = self.buffer.len();
                                }
                            }
                        } else {
                            let before_cursor = &self.buffer[..self.cursor_index];
                            let word_start = before_cursor
                                .iter()
                                .rposition(|&c| c == ' ')
                                .map_or(0, |pos| pos + 1);
                            let word: String = before_cursor[word_start..].iter().collect();
                            if word.starts_with('@') {
                                let query = &word[1..].to_lowercase();
                                if !query.is_empty() {
                                    if let Some(matched_user) = self
                                        .online_users
                                        .iter()
                                        .find(|u| u.to_lowercase().starts_with(query))
                                    {
                                        let suffix = &matched_user[query.len()..];
                                        self.buffer.extend(suffix.chars());
                                        self.cursor_index = self.buffer.len();
                                    }
                                }
                            } else if word.starts_with(':') && !word.ends_with(':') {
                                let query = &word[1..];
                                if !query.is_empty() {
                                    if let Some(matched) = super::emoji::EMOJI_SHORTCODES
                                        .iter()
                                        .find(|sc| sc.starts_with(query))
                                    {
                                        let rest = &matched[query.len()..];
                                        self.buffer.extend(":".chars());
                                        self.buffer.extend(rest.chars());
                                        self.buffer.extend(":".chars());
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
                        let matches: Vec<String> = THEME_NAMES
                            .iter()
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
                        let word_start = before_cursor
                            .iter()
                            .rposition(|&c| c == ' ')
                            .map_or(0, |pos| pos + 1);
                        let word: String = before_cursor[word_start..].iter().collect();

                        if word.starts_with('@') {
                            let query = &word[1..];
                            self.pre_tab_buffer = self.buffer.clone();
                            self.pre_tab_cursor = current_cursor;
                            self.tab_word_start = Some(word_start);

                            let matches: Vec<String> = self
                                .online_users
                                .iter()
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
                                self.cursor_index =
                                    word_start + self.tab_matches[0].chars().count();
                            }
                        } else if word.starts_with('/') {
                            self.pre_tab_buffer = self.buffer.clone();
                            self.pre_tab_cursor = current_cursor;
                            self.tab_word_start = Some(word_start);

                            let matches: Vec<String> = COMMANDS
                                .iter()
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
                                self.cursor_index =
                                    word_start + self.tab_matches[0].chars().count();
                            }
                        } else if word.starts_with(':') {
                            let query = &word[1..]; // strip leading ':'
                            self.pre_tab_buffer = self.buffer.clone();
                            self.pre_tab_cursor = current_cursor;
                            self.tab_word_start = Some(word_start);

                            let matches: Vec<String> = emoji::EMOJI_SHORTCODES
                                .iter()
                                .filter(|sc| is_fuzzy_match(query, sc))
                                .map(|sc| format!(":{}:", sc))
                                .collect();

                            if !matches.is_empty() {
                                self.tab_matches = matches;
                                self.tab_index = Some(0);

                                let mut new_buf = self.pre_tab_buffer[..word_start].to_vec();
                                new_buf.extend(self.tab_matches[0].chars());
                                new_buf.extend(&self.pre_tab_buffer[current_cursor..]);
                                self.buffer = new_buf;
                                self.cursor_index =
                                    word_start + self.tab_matches[0].chars().count();
                            }
                        }
                    }
                } else if let (Some(idx), Some(word_start)) = (self.tab_index, self.tab_word_start)
                {
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
                        self.cursor_index =
                            word_start + self.tab_matches[next_idx].chars().count();
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
