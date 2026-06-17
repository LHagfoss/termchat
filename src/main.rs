mod cli;
mod client;
mod config;
mod protocol;
mod server;

use clap::Parser;
use colored::Colorize;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = cli::Cli::parse();
    let user_config = config::load_or_create_config();

    match cli.command {
        cli::Commands::Start { name, ip, port } => {
            let server_name = match name {
                Some(n) => n,
                None => {
                    print!(
                        "       {} Enter server name: ",
                        "Input".bright_white().bold()
                    );
                    io::stdout().flush().unwrap();

                    let mut input = String::new();
                    io::stdin().read_line(&mut input).unwrap();
                    let trimmed = input.trim();

                    if trimmed.is_empty() {
                        "default-server".to_string()
                    } else {
                        trimmed.to_string()
                    }
                }
            };

            server::run(server_name, ip, port).await?;
        }
        cli::Commands::Join { ip, port, name } => {
            print!(
                "       {} Enter server token: ",
                "Input".bright_white().bold()
            );
            io::stdout().flush().unwrap();

            let mut token = String::new();
            io::stdin().read_line(&mut token).unwrap();
            let token = token.trim().to_string();

            let final_username = name.unwrap_or(user_config.name);

            client::run(ip, port, final_username, token, user_config.theme).await?;
        }
        cli::Commands::Profile { name } => {
            let updated = config::update_name(name.clone());
            println!(
                "{} Profile updated! Your username is now '{}'",
                "✓".green().bold(),
                updated.name
            );
        }
    }
    Ok(())
}
