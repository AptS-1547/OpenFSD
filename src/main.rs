mod auth;
mod client;
mod config;
mod db;
mod packet;
mod server;

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
        env_logger::Env::default().default_filter_or(&config.logging.level),
    )
    .init();

    log::info!("Starting OpenFSD Server...");

    // Initialize database
    log::info!("Initializing database...");
    let db = db::init(&config.database.url).await?;
    log::info!("Database initialized successfully");

    // Create and run server
    let server_config = config.into();
    let server = Server::new(server_config, db);

    // Run the server
    server.run().await?;

    Ok(())
}
