use crate::packet::Packet;
use crate::server::config::ServerMessage;
use std::net::SocketAddr;
use tokio::sync::broadcast;

/// Handle flight plan
pub async fn handle_flight_plan(
    packet: Packet,
    sender_addr: SocketAddr,
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
