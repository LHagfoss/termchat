use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub name: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub sidebar_collapsed: Option<bool>,
}

fn default_theme() -> String {
    "lagos".to_string()
}

pub fn load_or_create_config() -> Config {
    let proj_dirs = ProjectDirs::from("", "", "tc").expect("Could not find the home directory");

    let config_dir = proj_dirs.config_dir();
    let config_file = config_dir.join("config.toml");

    if config_file.exists() {
        let contents = fs::read_to_string(&config_file).expect("Failed to read config.toml");

        // Parse and ensure theme has default value if missing
        return toml::from_str(&contents).unwrap_or_else(|_| Config {
            name: "default-user".to_string(),
            theme: default_theme(),
            color: None,
            sidebar_collapsed: None,
        });
    }

    println!("   Welcome to tc! It looks like you don't have a profile yet.");
    print!("   What is your username? ");
    io::stdout().flush().unwrap();

    let mut name = String::new();
    io::stdin()
        .read_line(&mut name)
        .expect("Failed to read name");
    let name = name.trim().to_string();

    if name.is_empty() {
        eprintln!("Error: Name cannot be empty. Exiting.");
        std::process::exit(1);
    }

    let new_config = Config {
        name,
        theme: default_theme(),
        color: None,
        sidebar_collapsed: None,
    };

    fs::create_dir_all(config_dir).expect("Failed to create config directory");
    let toml_string = toml::to_string(&new_config).unwrap();
    fs::write(&config_file, toml_string).expect("Failed to write config.toml");

    println!("Profile saved to {:?}\n", config_file);

    new_config
}

pub fn update_name(new_name: String) -> Config {
    let proj_dirs = ProjectDirs::from("", "", "tc").expect("Could not find the home directory");
    let config_dir = proj_dirs.config_dir();
    let config_file = config_dir.join("config.toml");

    let mut config = load_or_create_config();
    config.name = new_name;

    fs::create_dir_all(config_dir).expect("Failed to create config directory");
    let toml_string = toml::to_string(&config).unwrap();
    fs::write(&config_file, toml_string).expect("Failed to write config.toml");

    config
}

pub fn update_theme(new_theme: String) -> Config {
    let proj_dirs = ProjectDirs::from("", "", "tc").expect("Could not find the home directory");
    let config_dir = proj_dirs.config_dir();
    let config_file = config_dir.join("config.toml");

    let mut config = load_or_create_config();
    config.theme = new_theme;

    fs::create_dir_all(config_dir).expect("Failed to create config directory");
    let toml_string = toml::to_string(&config).unwrap();
    fs::write(&config_file, toml_string).expect("Failed to write config.toml");

    config
}

pub fn update_color(new_color: Option<String>) -> Config {
    let proj_dirs = ProjectDirs::from("", "", "tc").expect("Could not find the home directory");
    let config_dir = proj_dirs.config_dir();
    let config_file = config_dir.join("config.toml");

    let mut config = load_or_create_config();
    config.color = new_color;

    fs::create_dir_all(config_dir).expect("Failed to create config directory");
    let toml_string = toml::to_string(&config).unwrap();
    fs::write(&config_file, toml_string).expect("Failed to write config.toml");

    config
}

pub fn update_sidebar_collapsed(new_val: bool) -> Config {
    let proj_dirs = ProjectDirs::from("", "", "tc").expect("Could not find the home directory");
    let config_dir = proj_dirs.config_dir();
    let config_file = config_dir.join("config.toml");

    let mut config = load_or_create_config();
    config.sidebar_collapsed = Some(new_val);

    fs::create_dir_all(config_dir).expect("Failed to create config directory");
    let toml_string = toml::to_string(&config).unwrap();
    fs::write(&config_file, toml_string).expect("Failed to write config.toml");

    config
}
