use crate::client::{Client, ClientState, ClientType};
use crate::packet::Packet;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};

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

/// Message sent from server to clients
#[derive(Debug, Clone)]
pub enum ServerMessage {
    Packet(Packet),
    Disconnect,
}

/// Main FSD Server
pub struct Server {
    config: ServerConfig,
    clients: Arc<RwLock<HashMap<SocketAddr, Client>>>,
    callsign_map: Arc<RwLock<HashMap<String, SocketAddr>>>,
    broadcast_tx: broadcast::Sender<(SocketAddr, ServerMessage)>,
}

impl Server {
    pub fn new(config: ServerConfig) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);
        
        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            callsign_map: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
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

        let (packet_tx, mut packet_rx) = mpsc::channel::<(SocketAddr, Packet)>(1000);

        // Spawn packet processor task
        let clients = self.clients.clone();
        let callsign_map = self.callsign_map.clone();
        let config = self.config.clone();
        let broadcast_tx = self.broadcast_tx.clone();
        
        tokio::spawn(async move {
            while let Some((addr, packet)) = packet_rx.recv().await {
                Self::process_packet(packet, addr, &clients, &callsign_map, &config, &broadcast_tx).await;
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
                if let Err(e) = Self::handle_client(stream, addr, packet_tx, broadcast_rx, clients).await {
                    log::error!("Client {} error: {}", addr, e);
                }
            });

            log::info!("Accepted connection from {}", addr);
        }
    }

    /// Handle individual client connection
    async fn handle_client(
        stream: TcpStream,
        addr: SocketAddr,
        packet_tx: mpsc::Sender<(SocketAddr, Packet)>,
        mut broadcast_rx: broadcast::Receiver<(SocketAddr, ServerMessage)>,
        clients: Arc<RwLock<HashMap<SocketAddr, Client>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        log::info!("Client connected from {}", addr);

        // Spawn task to handle outgoing messages
        let write_handle = tokio::spawn(async move {
            while let Ok((sender_addr, msg)) = broadcast_rx.recv().await {
                // Don't send messages back to the sender
                if sender_addr == addr {
                    continue;
                }
                
                match msg {
                    ServerMessage::Packet(packet) => {
                        let formatted = packet.format();
                        if let Err(e) = writer.write_all(formatted.as_bytes()).await {
                            log::error!("Failed to send packet to {}: {}", addr, e);
                            break;
                        }
                        if let Err(e) = writer.flush().await {
                            log::error!("Failed to flush to {}: {}", addr, e);
                            break;
                        }
                    }
                    ServerMessage::Disconnect => {
                        break;
                    }
                }
            }
        });

        // Handle incoming messages
        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            
            if bytes_read == 0 {
                log::info!("Client {} disconnected", addr);
                break;
            }

            match Packet::parse(&line) {
                Ok(packet) => {
                    log::debug!("Received packet from {}: {}", addr, packet);
                    
                    // Send packet to server for processing
                    if packet_tx.send((addr, packet)).await.is_err() {
                        log::error!("Failed to send packet to server");
                        break;
                    }
                }
                Err(e) => {
                    log::warn!("Failed to parse packet from {}: {}", addr, e);
                }
            }
        }

        // Clean up
        {
            let mut clients_map = clients.write().await;
            if let Some(client) = clients_map.get(&addr) {
                if let Some(callsign) = &client.callsign {
                    log::info!("Client {} ({}) disconnected", addr, callsign);
                }
            }
            clients_map.remove(&addr);
        }

        write_handle.abort();
        Ok(())
    }

    /// Process incoming packets
    async fn process_packet(
        packet: Packet,
        sender_addr: SocketAddr,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        config: &ServerConfig,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::debug!("Processing packet from {}: {}", sender_addr, packet);

        match packet.command.as_str() {
            "ID" => Self::handle_identification(packet, sender_addr, clients, callsign_map, config, broadcast_tx).await,
            "AA" | "AP" => Self::handle_login(packet, sender_addr, clients, callsign_map, broadcast_tx).await,
            "DA" | "DP" => Self::handle_logoff(packet, sender_addr, clients, callsign_map, broadcast_tx).await,
            "TM" => Self::handle_text_message(packet, sender_addr, clients, callsign_map, broadcast_tx).await,
            "CQ" => Self::handle_request(packet, sender_addr, clients, callsign_map, broadcast_tx).await,
            "CR" => Self::handle_response(packet, sender_addr, clients, callsign_map, broadcast_tx).await,
            "N" | "S" | "Y" => Self::handle_position_update(packet, sender_addr, clients, broadcast_tx).await,
            "FP" => Self::handle_flight_plan(packet, sender_addr, clients, broadcast_tx).await,
            _ => {
                log::debug!("Unhandled command: {}", packet.command);
            }
        }
    }

    /// Handle client identification (VATSIM)
    async fn handle_identification(
        packet: Packet,
        sender_addr: SocketAddr,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        _callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        _config: &ServerConfig,
        _broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::info!("Client identification from {}: {}", sender_addr, packet.source);
        
        // Update client info
        {
            let mut clients_map = clients.write().await;
            if let Some(client) = clients_map.get_mut(&sender_addr) {
                client.callsign = Some(packet.source.clone());
                client.state = ClientState::Identified;
            }
        }
        
        // TODO: Send server identification response
    }

    /// Handle login (AA for ATC, AP for pilot)
    async fn handle_login(
        packet: Packet,
        sender_addr: SocketAddr,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        _broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        let callsign = packet.source.clone();
        log::info!("Login attempt from {} ({})", sender_addr, callsign);

        // Extract client type from command
        let client_type = match packet.command.as_str() {
            "AA" => ClientType::Atc,
            "AP" => ClientType::Pilot,
            _ => return,
        };

        // Update client state
        {
            let mut clients_map = clients.write().await;
            if let Some(client) = clients_map.get_mut(&sender_addr) {
                client.callsign = Some(callsign.clone());
                client.client_type = Some(client_type);
                client.state = ClientState::Active;
            }
        }

        // Add to callsign map
        {
            let mut map = callsign_map.write().await;
            map.insert(callsign.clone(), sender_addr);
        }

        log::info!("Login successful for {}", callsign);
        
        // TODO: Send welcome message
    }

    /// Handle logoff
    async fn handle_logoff(
        packet: Packet,
        sender_addr: SocketAddr,
        _clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        _broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        let callsign = packet.source.clone();
        log::info!("Logoff from {} ({})", sender_addr, callsign);

        // Remove from callsign map
        let mut map = callsign_map.write().await;
        map.remove(&callsign);
    }

    /// Handle text message
    async fn handle_text_message(
        packet: Packet,
        sender_addr: SocketAddr,
        _clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        _callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::info!("Text message from {} to {}: {:?}", 
                   packet.source, packet.destination, packet.data);
        
        // Broadcast message to all clients
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
    }

    /// Handle position update
    async fn handle_position_update(
        packet: Packet,
        sender_addr: SocketAddr,
        _clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::debug!("Position update from {}: {}", sender_addr, packet.destination);
        
        // Broadcast position update to all clients
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
    }

    /// Handle flight plan
    async fn handle_flight_plan(
        packet: Packet,
        sender_addr: SocketAddr,
        _clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::info!("Flight plan from {}", packet.source);
        
        // Broadcast flight plan to all clients
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
    }

    /// Handle information request
    async fn handle_request(
        packet: Packet,
        sender_addr: SocketAddr,
        _clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        _callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        _broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::debug!("Request from {} ({}): {} -> {}", 
                   sender_addr, packet.source, packet.source, packet.destination);
        // TODO: Handle specific request types (CAPS, ATIS, RN, etc.)
        // For now, just log it
    }

    /// Handle information response
    async fn handle_response(
        packet: Packet,
        sender_addr: SocketAddr,
        _clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        _callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::debug!("Response from {} ({}): {} -> {}", 
                   sender_addr, packet.source, packet.source, packet.destination);
        
        // Broadcast response to all clients
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
    }
}
