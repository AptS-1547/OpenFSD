mod packet;
mod client;
mod server;

use server::{Server, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting OpenFSD Server...");

    // Create server with default config
    let config = ServerConfig::default();
    let server = Server::new(config);

    // Run the server
    server.run().await?;

    Ok(())
}
