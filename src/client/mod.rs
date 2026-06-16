use colored::Colorize;
use futures::{SinkExt, StreamExt};
use lagos_logger::{Level::*, logger};
use std::io::{self, Write};
use tokio::net::TcpStream;
use tokio::signal;
use tokio_util::codec::{Framed, FramedRead, LinesCodec};

use crate::protocol::{ClientToServer, ServerToClient};

fn format_username(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() > 10 {
        let truncated: String = chars.into_iter().take(7).collect();
        format!("{}...", truncated)
    } else {
        name.to_string()
    }
}

pub async fn run(
    ip: String,
    port: u16,
    name: String,
    token: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", ip, port);
    logger!(Info, "Attempting to connect to {}...", addr);

    let stream = TcpStream::connect(&addr).await?;
    let mut framed = Framed::new(stream, LinesCodec::new());

    let handshake = ClientToServer::Handshake { name, token };
    let handshake_json = serde_json::to_string(&handshake)?;
    framed.send(handshake_json).await?;

    logger!(Info, "Handshake sent! Authenticating...");

    match framed.next().await {
        Some(Ok(line)) => {
            let response: ServerToClient = serde_json::from_str(&line)?;
            match response {
                ServerToClient::Welcome { server_name } => {
                    logger!(Info, "Successfully joined server '{}'", server_name.bold());
                }
                ServerToClient::Error { message } => {
                    logger!(Error, "Connection rejected: {}", message);
                    return Ok(());
                }
                _ => {
                    logger!(Error, "Unexpected server response during handshake");
                    return Ok(());
                }
            }
        }
        Some(Err(e)) => {
            logger!(Error, "Failed to read from server: {}", e);
            return Ok(());
        }
        None => {
            logger!(
                Error,
                "Connection closed by server before authentication completed."
            );
            return Ok(());
        }
    }

    let mut stdin = FramedRead::new(tokio::io::stdin(), LinesCodec::new());

    print!("{} ", ">".bright_black().bold());
    io::stdout().flush().unwrap();

    loop {
        tokio::select! {
                    Some(Ok(line)) = stdin.next() => {
                        print!("\x1B[1A\x1B[2K\r");
                        io::stdout().flush().unwrap();

                        let chat_msg = ClientToServer::ChatMessage { content: line };
                        if let Ok(json) = serde_json::to_string(&chat_msg) {
                            if framed.send(json).await.is_err() {
                                logger!(Error, "Lost connection to server.");
                                break;
                            }
                        }

                        print!("{} ", ">".bright_black().bold());
                        io::stdout().flush().unwrap();
                    }

        result = framed.next() => {
                        match result {
                            Some(Ok(line)) => {
                                if let Ok(msg) = serde_json::from_str::<ServerToClient>(&line) {
                                    print!("\r\x1B[2K");
                                    io::stdout().flush().unwrap();

                                    match msg {
                                        ServerToClient::Broadcast { sender, content, timestamp } => {
                                            let local_time = timestamp.with_timezone(&chrono::Local).format("%H:%M");
                                            let display_name = format_username(&sender);

                                            logger!(Info, "{} {}: {}",
                                                local_time.to_string().dimmed(),
                                                display_name.blue().bold(),
                                                content
                                            );
                                        }
                                        ServerToClient::SystemAlert { content, .. } => {
                                            logger!(Info, "{}", content.yellow());
                                        }
                                        _ => {}
                                    }

                                    print!("{} ", ">".bright_black().bold());
                                    io::stdout().flush().unwrap();
                                }
                            }
                            _ => {
                                print!("\r\x1B[2K");
                                logger!(Error, "Connection closed by server.");
                                std::process::exit(0);
                            }
                        }
                    }

                    _ = signal::ctrl_c() => {
                        print!("\r\x1B[2K");
                        logger!(Info, "Disconnecting from chat...");
                        std::process::exit(0);
                    }
                }
    }

    Ok(())
}
