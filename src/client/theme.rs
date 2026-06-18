pub const COMMANDS: &[&str] = &[
    "/help", "/users", "/clear", "/refresh", "/info", "/exit", "/theme", "/debug", "/ask",
    "/send", "/download", "/open",
];
pub const THEME_NAMES: &[&str] = &[
    "blurple",
    "matrix",
    "cyberpunk",
    "sunset",
    "lagos",
    "mint",
    "lavender",
];

#[derive(Clone, Copy, Debug)]
pub struct ThemeColors {
    pub title: (u8, u8, u8),
    pub prompt: (u8, u8, u8),
    pub accent: (u8, u8, u8),
}

impl ThemeColors {
    pub fn get(theme_name: &str) -> Self {
        match theme_name.to_lowercase().as_str() {
            "lagos" => Self {
                title: (236, 110, 93),
                prompt: (236, 110, 93),
                accent: (236, 110, 93),
            },
            "mint" => Self {
                title: (140, 216, 167),
                prompt: (140, 216, 167),
                accent: (140, 216, 167),
            },
            "lavender" => Self {
                title: (193, 158, 214),
                prompt: (193, 158, 214),
                accent: (193, 158, 214),
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
            _ => Self {
                title: (114, 137, 218),
                prompt: (114, 137, 218),
                accent: (114, 137, 218),
            },
        }
    }
}
