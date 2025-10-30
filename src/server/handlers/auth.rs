use crate::auth;
use crate::client::{Client, ClientState, ClientType};
use crate::packet::Packet;
use crate::server::config::{ServerConfig, ServerMessage};
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Handle client identification (VATSIM)
pub async fn handle_identification(
    packet: Packet,
    sender_addr: SocketAddr,
    clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
    _callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
    _config: &ServerConfig,
    broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    db: &Arc<DatabaseConnection>,
) {
    log::info!(
        "Client identification from {}: {}",
        sender_addr,
        packet.source
    );

    // Parse client ID packet
    // $ID(callsign):SERVER:(client id):(client string):3:2:(network ID):(num)
    let client_id_str = packet.data.get(0).cloned().unwrap_or_default();
    let client_string = packet.data.get(1).cloned();
    let network_id = packet.data.get(4).cloned();

    // Validate client ID against whitelist
    match auth::validate_client_id(db, &client_id_str).await {
        Ok(()) => {
            log::info!("Client ID {} is whitelisted", client_id_str);
        }
        Err(e) => {
            log::warn!("Client ID validation failed: {}", e);
            // Send error message and disconnect
            let error_packet = Packet {
                packet_type: crate::packet::PacketType::Request,
                command: "ER".to_string(),
                source: "server".to_string(),
                destination: packet.source.clone(),
                data: vec![
                    "016".to_string(),
                    String::new(),
                    "Unauthorized client software".to_string(),
                ],
            };
            let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(error_packet)));
            return;
        }
    }

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
pub async fn handle_login(
    packet: Packet,
    sender_addr: SocketAddr,
    clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
    callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
    broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    db: &Arc<DatabaseConnection>,
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
    let (real_name, network_id, password, _rating) = match packet.command.as_str() {
        "AA" => {
            // #AA(callsign):SERVER:(full name):(network ID):(password):(rating):(protocol version)
            let real_name = packet.data.get(0).cloned();
            let network_id = packet.data.get(1).cloned();
            let password = packet.data.get(2).cloned();
            let rating: Option<i32> = packet.data.get(3).and_then(|s| s.parse().ok());
            (real_name, network_id, password, rating)
        }
        "AP" => {
            // #AP(callsign):SERVER:(network ID):(password):(rating):(protocol version):(num2):(full name ICAO)
            let network_id = packet.data.get(0).cloned();
            let password = packet.data.get(1).cloned();
            let rating: Option<i32> = packet.data.get(2).and_then(|s| s.parse().ok());
            let real_name = packet.data.get(5).cloned();
            (real_name, network_id, password, rating)
        }
        _ => (None, None, None, None),
    };

    // Validate credentials
    let network_id_str = match network_id.clone() {
        Some(id) => id,
        None => {
            log::warn!("Missing network ID for login");
            return;
        }
    };

    let password_str = match password {
        Some(pwd) => pwd,
        None => {
            log::warn!("Missing password for login");
            return;
        }
    };

    // Authenticate user
    let user = match auth::validate_login(db, &network_id_str, &password_str).await {
        Ok(user) => {
            log::info!("User {} authenticated successfully", network_id_str);
            user
        }
        Err(e) => {
            log::warn!("Authentication failed for {}: {}", network_id_str, e);
            // Send error message
            let error_packet = Packet {
                packet_type: crate::packet::PacketType::Request,
                command: "ER".to_string(),
                source: "server".to_string(),
                destination: callsign.clone(),
                data: vec![
                    "003".to_string(),
                    String::new(),
                    "Invalid credentials".to_string(),
                ],
            };
            let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(error_packet)));
            return;
        }
    };

    // Use rating from database
    let atc_rating = user.atc_rating;
    let pilot_rating = user.pilot_rating;
    let db_real_name = user.real_name.clone();

    // Update client state
    {
        let mut clients_map = clients.write().await;
        if let Some(client) = clients_map.get_mut(&sender_addr) {
            client.callsign = Some(callsign.clone());
            client.client_type = Some(client_type.clone());
            client.state = ClientState::Active;
            client.real_name = Some(db_real_name.clone());
            client.network_id = Some(network_id_str.clone());
            client.rating = Some(match client_type {
                ClientType::Atc => atc_rating,
                ClientType::Pilot => pilot_rating,
                _ => 1,
            });
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

    // Complete VATSIM login sequence for ATC
    if client_type == ClientType::Atc {
        // Request client capabilities
        let caps_request = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "CQ".to_string(),
            source: "SERVER".to_string(),
            destination: callsign.clone(),
            data: vec!["CAPS".to_string()],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(caps_request)));

        // Send additional ATC capability requests
        let atc_info_request = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "CR".to_string(),
            source: "SERVER".to_string(),
            destination: callsign.clone(),
            data: vec!["CAPS:ATCINFO=1:SECPOS=1:MODELDESC=1:ONGOINGCOORD=1".to_string()],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(atc_info_request)));

        // Send IP information
        let ip_request = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "CR".to_string(),
            source: "SERVER".to_string(),
            destination: callsign.clone(),
            data: vec!["IP".to_string(), sender_addr.ip().to_string()],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(ip_request)));
    }

    // Complete VATSIM login sequence for Pilots
    if client_type == ClientType::Pilot {
        // Request client capabilities
        let caps_request = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "CQ".to_string(),
            source: "SERVER".to_string(),
            destination: callsign.clone(),
            data: vec!["CAPS".to_string()],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(caps_request)));

        // Send IP information
        let ip_request = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "CR".to_string(),
            source: "SERVER".to_string(),
            destination: callsign.clone(),
            data: vec!["IP".to_string(), sender_addr.ip().to_string()],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(ip_request)));

        // Send no flight plan warning (if applicable)
        let no_fp_warning = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "ER".to_string(),
            source: "server".to_string(),
            destination: callsign.clone(),
            data: vec![
                "008".to_string(),
                callsign.clone(),
                "No flightplan".to_string(),
            ],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(no_fp_warning)));
    }

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
pub async fn handle_logoff(
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
