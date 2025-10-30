mod config;
mod connection;
mod handlers;
mod processor;

pub use config::{ServerConfig, ServerMessage};

use crate::client::Client;
use crate::packet::Packet;
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};

/// Main FSD Server
pub struct Server {
    config: ServerConfig,
    clients: Arc<RwLock<HashMap<SocketAddr, Client>>>,
    callsign_map: Arc<RwLock<HashMap<String, SocketAddr>>>,
    broadcast_tx: broadcast::Sender<(SocketAddr, ServerMessage)>,
    db: Arc<DatabaseConnection>,
}

impl Server {
    pub fn new(config: ServerConfig, db: DatabaseConnection) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);

        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            callsign_map: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
            db: Arc::new(db),
        }
    }

    /// Start the FSD server
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("{}:{}", self.config.address, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        log::info!(
            "FSD Server {} v{} listening on {}",
            self.config.server_name,
            self.config.server_version,
            addr
        );

        let (packet_tx, mut packet_rx) = mpsc::channel::<(SocketAddr, Packet)>(1000);

        // Spawn packet processor task
        let clients = self.clients.clone();
        let callsign_map = self.callsign_map.clone();
        let config = self.config.clone();
        let broadcast_tx = self.broadcast_tx.clone();
        let db = self.db.clone();

        tokio::spawn(async move {
            while let Some((addr, packet)) = packet_rx.recv().await {
                processor::process_packet(
                    packet,
                    addr,
                    &clients,
                    &callsign_map,
                    &config,
                    &broadcast_tx,
                    &db,
                )
                .await;
            }
        });

        // Spawn heartbeat task
        let broadcast_tx_heartbeat = self.broadcast_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let heartbeat = Packet {
                    packet_type: crate::packet::PacketType::Client,
                    command: "DL".to_string(),
                    source: "SERVER".to_string(),
                    destination: "*".to_string(),
                    data: vec!["0".to_string(), "0".to_string()],
                };
                // Use a dummy address for server-originated broadcasts
                let _ = broadcast_tx_heartbeat.send((
                    "0.0.0.0:0".parse().unwrap(),
                    ServerMessage::Packet(heartbeat),
                ));
            }
        });

        // Accept connections
        loop {
            let (stream, addr) = listener.accept().await?;

            // Check max clients
            {
                let clients = self.clients.read().await;
                if clients.len() >= self.config.max_clients {
                    log::warn!("Max clients reached, rejecting connection from {}", addr);
                    continue;
                }
            }

            // Add new client
            {
                let mut clients = self.clients.write().await;
                clients.insert(addr, Client::new(addr));
            }

            // Spawn client handler
            let packet_tx = packet_tx.clone();
            let broadcast_rx = self.broadcast_tx.subscribe();
            let clients = self.clients.clone();

            tokio::spawn(async move {
                if let Err(e) =
                    connection::handle_client(stream, addr, packet_tx, broadcast_rx, clients).await
                {
                    log::error!("Client {} error: {}", addr, e);
                }
            });

            log::info!("Accepted connection from {}", addr);
        }
    }
}
