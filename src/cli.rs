use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tc", version = "0.1.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Start {
        #[arg(short, long)]
        name: Option<String>,

        #[arg(short, long, default_value = "0.0.0.0")]
        ip: String,

        #[arg(short, long, default_value_t = 8080)]
        port: u16,

        #[arg(short, long)]
        debug: bool,
    },
    Join {
        #[arg(short, long)]
        ip: String,

        #[arg(short, long, default_value_t = 8080)]
        port: u16,

        #[arg(short, long)]
        name: Option<String>,
    },
    Profile {
        #[arg(short, long)]
        name: String,
    },
}
