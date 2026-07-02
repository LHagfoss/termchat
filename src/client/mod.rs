pub mod emoji;
pub mod input;
pub mod theme;
pub mod tui;

pub async fn run(
    ip: String,
    port: u16,
    name: String,
    token: String,
    theme_name: String,
    color: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    tui::run(ip, port, name, token, theme_name, color).await
}
