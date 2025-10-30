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

    /// Generate a random 22-character hexadecimal token for server identification
    fn generate_token() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..22)
            .map(|_| format!("{:x}", rng.gen_range(0..16)))
            .collect()
    }

    /// Send a text message to a client
    async fn send_text_message(
        writer: &mut tokio::net::tcp::OwnedWriteHalf,
        from: &str,
        to: &str,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let packet = Packet {
            packet_type: crate::packet::PacketType::Client,
            command: "TM".to_string(),
            source: from.to_string(),
            destination: to.to_string(),
            data: vec![message.to_string()],
        };
        let formatted = packet.format();
        writer.write_all(formatted.as_bytes()).await?;
        writer.flush().await?;
        Ok(())
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

        tokio::spawn(async move {
            while let Some((addr, packet)) = packet_rx.recv().await {
                Self::process_packet(
                    packet,
                    addr,
                    &clients,
                    &callsign_map,
                    &config,
                    &broadcast_tx,
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
                    Self::handle_client(stream, addr, packet_tx, broadcast_rx, clients).await
                {
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

        // Send server identification (VATSIM protocol)
        let server_ident = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "DI".to_string(),
            destination: "SERVER".to_string(),
            source: "CLIENT".to_string(),
            data: vec![
                "VATSIM FSD V3.13".to_string(),
                Self::generate_token(),
            ],
        };
        let formatted = server_ident.format();
        if let Err(e) = writer.write_all(formatted.as_bytes()).await {
            log::error!("Failed to send server identification to {}: {}", addr, e);
            return Err(e.into());
        }
        writer.flush().await?;

        // Spawn task to handle outgoing messages
        let write_handle = tokio::spawn(async move {
            while let Ok((sender_addr, msg)) = broadcast_rx.recv().await {
                // Don't send messages back to the sender (except for server-originated messages)
                let is_server_message = sender_addr.port() == 0;
                if !is_server_message && sender_addr == addr {
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
            "ID" => {
                Self::handle_identification(
                    packet,
                    sender_addr,
                    clients,
                    callsign_map,
                    config,
                    broadcast_tx,
                )
                .await
            }
            "AA" | "AP" => {
                Self::handle_login(packet, sender_addr, clients, callsign_map, broadcast_tx).await
            }
            "DA" | "DP" => {
                Self::handle_logoff(packet, sender_addr, clients, callsign_map, broadcast_tx).await
            }
            "TM" => {
                Self::handle_text_message(packet, sender_addr, clients, callsign_map, broadcast_tx)
                    .await
            }
            "CQ" => {
                Self::handle_request(packet, sender_addr, clients, callsign_map, broadcast_tx).await
            }
            "CR" => {
                Self::handle_response(packet, sender_addr, clients, callsign_map, broadcast_tx)
                    .await
            }
            "AX" => {
                Self::handle_metar_request(packet, sender_addr, broadcast_tx).await
            }
            "N" | "S" | "Y" => {
                Self::handle_position_update(packet, sender_addr, clients, broadcast_tx).await
            }
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
        log::info!(
            "Client identification from {}: {}",
            sender_addr,
            packet.source
        );

        // Parse client ID packet
        // $ID(callsign):SERVER:(client id):(client string):3:2:(network ID):(num)
        let client_id = packet.data.get(0).cloned();
        let client_string = packet.data.get(1).cloned();
        let network_id = packet.data.get(4).cloned();

        // Update client info
        {
            let mut clients_map = clients.write().await;
            if let Some(client) = clients_map.get_mut(&sender_addr) {
                client.callsign = Some(packet.source.clone());
                client.client_string = client_string.clone();
                client.network_id = network_id;
                client.state = ClientState::Identified;
            }
        }

        log::info!(
            "Client {} identified with client software: {:?}",
            packet.source,
            client_string
        );
    }

    /// Handle login (AA for ATC, AP for pilot)
    async fn handle_login(
        packet: Packet,
        sender_addr: SocketAddr,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        let callsign = packet.source.clone();
        log::info!("Login attempt from {} ({})", sender_addr, callsign);

        // Extract client type from command and parse login data
        let client_type = match packet.command.as_str() {
            "AA" => ClientType::Atc,
            "AP" => ClientType::Pilot,
            _ => return,
        };

        // Parse login data
        let (real_name, network_id, rating) = match packet.command.as_str() {
            "AA" => {
                // #AA(callsign):SERVER:(full name):(network ID):(password):(rating):(protocol version)
                let real_name = packet.data.get(0).cloned();
                let network_id = packet.data.get(1).cloned();
                let rating = packet.data.get(3).and_then(|s| s.parse().ok());
                (real_name, network_id, rating)
            }
            "AP" => {
                // #AP(callsign):SERVER:(network ID):(password):(rating):(protocol version):(num2):(full name ICAO)
                let network_id = packet.data.get(0).cloned();
                let rating = packet.data.get(2).and_then(|s| s.parse().ok());
                let real_name = packet.data.get(5).cloned();
                (real_name, network_id, rating)
            }
            _ => (None, None, None),
        };

        // Update client state
        {
            let mut clients_map = clients.write().await;
            if let Some(client) = clients_map.get_mut(&sender_addr) {
                client.callsign = Some(callsign.clone());
                client.client_type = Some(client_type);
                client.state = ClientState::Active;
                client.real_name = real_name;
                client.network_id = network_id;
                client.rating = rating;
            }
        }

        // Add to callsign map
        {
            let mut map = callsign_map.write().await;
            map.insert(callsign.clone(), sender_addr);
        }

        log::info!("Login successful for {}", callsign);

        // Send welcome messages (VATSIM style)
        let welcome_messages = vec![
            "By using your VATSIM assigned identification number on this server you",
            "hereby agree to the terms of the VATSIM Code of Regulations and the",
            "VATSIM User Agreement and the VATSIM Code of Conduct which may be viewed",
            "at http://www.vatsim.net/network/docs/",
            "All logins are tracked and identification numbers are recorded.",
            "Users must enter their real full first names and surnames when logging",
            "onto any of the VATSIM.net servers.",
        ];

        for msg in welcome_messages {
            let welcome_packet = Packet {
                packet_type: crate::packet::PacketType::Client,
                command: "TM".to_string(),
                source: "server".to_string(),
                destination: callsign.clone(),
                data: vec![msg.to_string()],
            };
            let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(welcome_packet)));
        }

        // Request client capabilities
        let caps_request = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "CQ".to_string(),
            source: "SERVER".to_string(),
            destination: callsign.clone(),
            data: vec!["CAPS".to_string()],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(caps_request)));

        // Broadcast client addition to all other clients
        let add_client_packet = Packet {
            packet_type: crate::packet::PacketType::Client,
            command: packet.command.clone(),
            source: callsign.clone(),
            destination: "SERVER".to_string(),
            data: packet.data.clone(),
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(add_client_packet)));
    }

    /// Handle logoff
    async fn handle_logoff(
        packet: Packet,
        sender_addr: SocketAddr,
        _clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        let callsign = packet.source.clone();
        log::info!("Logoff from {} ({})", sender_addr, callsign);

        // Remove from callsign map
        {
            let mut map = callsign_map.write().await;
            map.remove(&callsign);
        }

        // Broadcast client removal to all other clients
        let remove_packet = Packet {
            packet_type: crate::packet::PacketType::Client,
            command: packet.command.clone(),
            source: callsign,
            destination: packet.destination.clone(),
            data: packet.data.clone(),
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(remove_packet)));
    }

    /// Handle text message
    async fn handle_text_message(
        packet: Packet,
        sender_addr: SocketAddr,
        _clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        _callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::info!(
            "Text message from {} to {}: {:?}",
            packet.source,
            packet.destination,
            packet.data
        );

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
        log::debug!(
            "Position update from {}: {}",
            sender_addr,
            packet.destination
        );

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
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet.clone())));

        // Send flight plan acknowledgment (VATSIM protocol)
        // #PC(server):(callsign):CCP:BC:(flightplan callsign):0
        let ack_packet = Packet {
            packet_type: crate::packet::PacketType::Client,
            command: "PC".to_string(),
            source: "server".to_string(),
            destination: packet.source.clone(),
            data: vec![
                "CCP".to_string(),
                "BC".to_string(),
                packet.source.clone(),
                "0".to_string(),
            ],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(ack_packet)));
    }

    /// Handle information request
    async fn handle_request(
        packet: Packet,
        sender_addr: SocketAddr,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        _callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::debug!(
            "Request from {} ({}): {} -> {}",
            sender_addr,
            packet.source,
            packet.source,
            packet.destination
        );

        if packet.data.is_empty() {
            return;
        }

        let request_type = &packet.data[0];
        match request_type.as_str() {
            "CAPS" => {
                // Just forward CAPS requests to the destination
                let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
            }
            "ATIS" => {
                // Forward ATIS requests to the destination
                let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
            }
            "RN" => {
                // Handle real name request
                Self::handle_real_name_request(packet, sender_addr, clients, broadcast_tx).await;
            }
            _ => {
                // Forward other requests
                let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
            }
        }
    }

    /// Handle real name request
    async fn handle_real_name_request(
        packet: Packet,
        sender_addr: SocketAddr,
        clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        let clients_map = clients.read().await;
        if let Some(client) = clients_map.get(&sender_addr) {
            if let Some(callsign) = &client.callsign {
                let real_name = client.real_name.clone().unwrap_or_default();
                let rating = client.rating.unwrap_or(0);
                let client_type = client.client_type.clone();

                let response_data = match client_type {
                    Some(ClientType::Atc) => {
                        // ATC: $CR(requestee):(requester):RN:(real name):(ATC sector file):(rating)
                        vec![
                            "RN".to_string(),
                            real_name,
                            String::new(), // ATC sector file (empty for now)
                            rating.to_string(),
                        ]
                    }
                    Some(ClientType::Pilot) => {
                        // Pilot: $CR(requestee):(requester):RN:(real name ICAO)::(rating)
                        vec![
                            "RN".to_string(),
                            real_name,
                            String::new(), // Empty field
                            rating.to_string(),
                        ]
                    }
                    _ => return,
                };

                let response = Packet {
                    packet_type: crate::packet::PacketType::Request,
                    command: "CR".to_string(),
                    source: callsign.clone(),
                    destination: packet.source.clone(),
                    data: response_data,
                };

                let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(response)));
            }
        }
    }

    /// Handle METAR request
    async fn handle_metar_request(
        packet: Packet,
        sender_addr: SocketAddr,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        // Extract ICAO code from packet data
        // $AX(callsign):SERVER:METAR:(ICAO airport code)
        if packet.data.len() < 2 {
            log::warn!("Invalid METAR request format from {}", sender_addr);
            return;
        }

        let icao = &packet.data[1];
        log::info!("METAR request for {} from {}", icao, packet.source);

        // For now, send a dummy METAR response
        // In a real implementation, you would fetch actual METAR data
        let metar_data = format!(
            "{} 121200Z AUTO 09008KT 9999 FEW040 BKN100 15/08 Q1013 NOSIG",
            icao
        );

        let response = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "AR".to_string(),
            source: "server".to_string(),
            destination: packet.source.clone(),
            data: vec!["METAR".to_string(), metar_data],
        };

        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(response)));
    }

    /// Handle information response
    async fn handle_response(
        packet: Packet,
        sender_addr: SocketAddr,
        _clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
        _callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
        broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    ) {
        log::debug!(
            "Response from {} ({}): {} -> {}",
            sender_addr,
            packet.source,
            packet.source,
            packet.destination
        );

        // Broadcast response to all clients
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
    }
}
