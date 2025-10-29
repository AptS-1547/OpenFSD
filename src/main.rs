mod packet;
mod client;
mod server;
mod config;

use server::Server;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = if Path::new("config.toml").exists() {
        config::Config::from_file("config.toml")?
    } else {
        log::warn!("config.toml not found, using default configuration");
        config::Config::default()
    };

    // Initialize logger
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(&config.logging.level)
    ).init();

    log::info!("Starting OpenFSD Server...");

    // Create and run server
    let server_config = config.into();
    let server = Server::new(server_config);

    // Run the server
    server.run().await?;

    Ok(())
}
