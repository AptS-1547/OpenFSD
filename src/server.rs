use crate::client::{Client, ClientConnection, ClientState, ClientType};
use crate::packet::{Packet, PacketType};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use rand::Rng;

/// FSD Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub address: String,
    pub port: u16,
    pub server_name: String,
    pub server_version: String,
    pub max_clients: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            address: "0.0.0.0".to_string(),
            port: 6809,
            server_name: "OpenFSD".to_string(),
            server_version: "0.1.0".to_string(),
            max_clients: 1000,
        }
    }
}

/// Main FSD Server
pub struct Server {
    config: ServerConfig,
    clients: Arc<RwLock<HashMap<SocketAddr, Client>>>,
    callsign_map: Arc<RwLock<HashMap<String, SocketAddr>>>,
}

impl Server {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            callsign_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the FSD server
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("{}:{}", self.config.address, self.config.port);
        let listener = TcpListener::bind(&addr).await?;
        
        log::info!("FSD Server {} v{} listening on {}", 
                   self.config.server_name, 
                   self.config.server_version,
                   addr);

        let (tx, mut rx) = mpsc::channel::<Packet>(1000);

        // Spawn packet processor task
        let clients = self.clients.clone();
        let callsign_map = self.callsign_map.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            while let Some(packet) = rx.recv().await {
                Self::process_packet(packet, &clients, &callsign_map, &config).await;
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
            let tx = tx.clone();
            tokio::spawn(async move {
                let conn = ClientConnection::new(stream, addr, tx);
                if let Err(e) = conn.handle().await {
                    log::error!("Client {} error: {}", addr, e);
                }
            });

            log::info!("Accepted connection from {}", addr);
        }
    }

    /// Process incoming packets
    async fn process_packet(
        packet: Packet,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        config: &ServerConfig,
    ) {
        log::debug!("Processing packet: {}", packet);

        match packet.command.as_str() {
            "ID" => Self::handle_identification(packet, clients, callsign_map, config).await,
            "AA" | "AP" => Self::handle_login(packet, clients, callsign_map).await,
            "DA" | "DP" => Self::handle_logoff(packet, clients, callsign_map).await,
            "TM" => Self::handle_text_message(packet, clients, callsign_map).await,
            "CQ" => Self::handle_request(packet, clients, callsign_map).await,
            "CR" => Self::handle_response(packet, clients, callsign_map).await,
            _ => {
                log::debug!("Unhandled command: {}", packet.command);
            }
        }
    }

    /// Handle client identification (VATSIM)
    async fn handle_identification(
        packet: Packet,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        config: &ServerConfig,
    ) {
        log::info!("Client identification: {}", packet);
        
        // Send server identification first if this is initial contact
        // For now, we'll just mark the client as identified
        // TODO: Implement proper VATSIM authentication
    }

    /// Handle login (AA for ATC, AP for pilot)
    async fn handle_login(
        packet: Packet,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
    ) {
        let callsign = packet.source.clone();
        log::info!("Login attempt from {}", callsign);

        // Extract client type from command
        let client_type = match packet.command.as_str() {
            "AA" => ClientType::Atc,
            "AP" => ClientType::Pilot,
            _ => return,
        };

        // TODO: Validate credentials
        // For now, accept all logins

        // Send welcome message
        log::info!("Login successful for {}", callsign);
    }

    /// Handle logoff
    async fn handle_logoff(
        packet: Packet,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
    ) {
        let callsign = packet.source.clone();
        log::info!("Logoff from {}", callsign);

        // Remove from callsign map
        let mut map = callsign_map.write().await;
        map.remove(&callsign);
    }

    /// Handle text message
    async fn handle_text_message(
        packet: Packet,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
    ) {
        log::info!("Text message from {} to {}: {:?}", 
                   packet.source, packet.destination, packet.data);
        
        // TODO: Route message to destination
    }

    /// Handle information request
    async fn handle_request(
        packet: Packet,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
    ) {
        log::debug!("Request: {} -> {}", packet.source, packet.destination);
        // TODO: Handle specific request types (CAPS, ATIS, RN, etc.)
    }

    /// Handle information response
    async fn handle_response(
        packet: Packet,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
    ) {
        log::debug!("Response: {} -> {}", packet.source, packet.destination);
        // TODO: Route response to requester
    }

    /// Generate random hex token for server identification
    fn generate_token() -> String {
        let mut rng = rand::thread_rng();
        (0..22)
            .map(|_| format!("{:X}", rng.gen_range(0..16)))
            .collect()
    }
}
