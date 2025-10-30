use crate::client::{Client, ClientType};
use crate::packet::Packet;
use crate::server::config::ServerMessage;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Handle information request
pub async fn handle_request(
    packet: Packet,
    sender_addr: SocketAddr,
    clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
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
            // Handle ATIS requests
            handle_atis_request(packet, sender_addr, clients, broadcast_tx).await;
        }
        "RN" => {
            // Handle real name request
            handle_real_name_request(packet, sender_addr, clients, broadcast_tx).await;
        }
        "INF" => {
            // Handle system information request
            handle_inf_request(packet, sender_addr, clients, broadcast_tx).await;
        }
        "ACC" => {
            // Handle aircraft configuration request (VATSIM only)
            handle_acc_request(packet, sender_addr, clients, broadcast_tx).await;
        }
        _ => {
            // Forward other requests
            let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
        }
    }
}

/// Handle real name request
pub async fn handle_real_name_request(
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
pub async fn handle_metar_request(
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

/// Handle ATIS request
/// Returns the requested callsign's voice server URL and ATIS message
pub async fn handle_atis_request(
    packet: Packet,
    sender_addr: SocketAddr,
    clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
    broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
) {
    log::info!("ATIS request from {} to {}", packet.source, packet.destination);

    // For now, send a sample ATIS response
    // In a real implementation, this would be stored per-client or fetched from database

    // Sample ATIS messages
    let atis_lines = vec![
        "London Heathrow ATIS Information Alpha",
        "Runway 27L in use for landing",
        "Runway 27R in use for departure",
        "Wind 270 at 8 knots",
        "Visibility 10km",
        "Cloud scattered at 4000ft",
        "Temperature 15 Celsius",
        "QNH 1013",
        "Advise on first contact you have information Alpha",
    ];

    // Send voice server URL
    let voice_response = Packet {
        packet_type: crate::packet::PacketType::Request,
        command: "CR".to_string(),
        source: packet.destination.clone(),
        destination: packet.source.clone(),
        data: vec![
            "ATIS".to_string(),
            "V".to_string(),
            "voice.vatsim.net/uk".to_string(),
        ],
    };
    let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(voice_response)));

    // Send ATIS text lines
    for line in &atis_lines {
        let text_response = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "CR".to_string(),
            source: packet.destination.clone(),
            destination: packet.source.clone(),
            data: vec![
                "ATIS".to_string(),
                "T".to_string(),
                line.to_string(),
            ],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(text_response)));
    }

    // Send end marker with line count
    let end_response = Packet {
        packet_type: crate::packet::PacketType::Request,
        command: "CR".to_string(),
        source: packet.destination.clone(),
        destination: packet.source.clone(),
        data: vec![
            "ATIS".to_string(),
            "E".to_string(),
            (atis_lines.len() + 2).to_string(), // +2 for voice and end lines
        ],
    };
    let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(end_response)));
}

/// Handle system information request (INF)
/// Response format: #TM(callsign):DATA:(client string) PID=(CID) ((Real name ICAO)) IP=(IP address) SYS_UID=(uid) FSVER=(sim) LT=(lat) LO=(lon) AL=(alt)
pub async fn handle_inf_request(
    packet: Packet,
    sender_addr: SocketAddr,
    clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
    broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
) {
    log::info!("System information request from {} to {}", packet.source, packet.destination);

    // Find the target client
    let target_callsign = &packet.destination;
    let clients_map = clients.read().await;

    let mut found_client = None;
    for (addr, client) in clients_map.iter() {
        if let Some(callsign) = &client.callsign {
            if callsign == target_callsign {
                found_client = Some((addr, client));
                break;
            }
        }
    }

    if let Some((client_addr, client)) = found_client {
        let client_string = client.client_string.clone().unwrap_or_default();
        let real_name = client.real_name.clone().unwrap_or_default();
        let network_id = client.network_id.clone().unwrap_or_default();

        // Generate sample system information
        // In a real implementation, this would be collected from the client
        let inf_response = format!(
            "{} PID=({}) (({})) IP=({}) SYS_UID=-123456789 FSVER={} LT=51.5 LO=-0.1 AL=35000",
            client_string,
            network_id,
            real_name,
            client_addr.ip(),
            client.client_type.as_ref().map(|t| match t {
                ClientType::Atc => "",
                _ => "Prepar3dV3",
            }).unwrap_or("")
        );

        let response = Packet {
            packet_type: crate::packet::PacketType::Client,
            command: "TM".to_string(),
            source: target_callsign.clone(),
            destination: "DATA".to_string(),
            data: vec![inf_response],
        };

        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(response)));
    } else {
        log::warn!("System information request for unknown client: {}", target_callsign);
    }
}

/// Handle information response
pub async fn handle_response(
    packet: Packet,
    sender_addr: SocketAddr,
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

/// Handle aircraft configuration request (ACC) - VATSIM only
/// Returns current configuration of aircraft in JSON format
pub async fn handle_acc_request(
    packet: Packet,
    sender_addr: SocketAddr,
    clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
    broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
) {
    log::info!("Aircraft configuration request from {} to {}", packet.source, packet.destination);

    // Find the target client
    let target_callsign = &packet.destination;
    let clients_map = clients.read().await;

    let mut found_client = None;
    for (_addr, client) in clients_map.iter() {
        if let Some(callsign) = &client.callsign {
            if callsign == target_callsign {
                found_client = Some(client);
                break;
            }
        }
    }

    if let Some(client) = found_client {
        // Generate sample aircraft configuration data in JSON format
        // In a real implementation, this would be collected from the client
        let acc_response = r#"{
    "config": {
        "is_full_data": true,
        "lights": {
            "strobe_on": false,
            "landing_on": false,
            "taxi_on": true,
            "beacon_on": true,
            "nav_on": true,
            "logo_on": false
        },
        "engines": {
            "1": {
                "on": true
            },
            "2": {
                "on": true
            }
        },
        "gear_down": false,
        "flaps_pct": 0,
        "spoilers_out": false,
        "on_ground": true
    }
}"#;

        // Note: ACC responses are prefixed with $CQ, not $CR as expected
        let response = Packet {
            packet_type: crate::packet::PacketType::Request,
            command: "CQ".to_string(),
            source: target_callsign.clone(),
            destination: packet.source.clone(),
            data: vec!["ACC".to_string(), acc_response.to_string()],
        };

        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(response)));
    } else {
        log::warn!("ACC request for unknown client: {}", target_callsign);
    }
}
