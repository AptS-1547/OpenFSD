use crate::packet::Packet;
use crate::server::config::ServerMessage;
use std::net::SocketAddr;
use tokio::sync::broadcast;

/// Process message content for IVAO escaping (:: -> :)
/// IVAO uses :: as escape sequence for colons in message content
fn process_message_content(content: &str) -> String {
    content.replace("::", ":")
}

/// Handle text message
pub async fn handle_text_message(
    packet: Packet,
    sender_addr: SocketAddr,
    broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
) {
    log::info!(
        "Text message from {} to {}: {:?}",
        packet.source,
        packet.destination,
        packet.data
    );

    // Process message content for IVAO escaping
    let mut processed_packet = packet.clone();
    if !processed_packet.data.is_empty() {
        processed_packet.data = processed_packet.data
            .iter()
            .map(|content| process_message_content(content))
            .collect();
    }

    // Check for flight plan acknowledgment (VATSIM protocol)
    // Format: #TM(own callsign):FP:(flightplan callsign) GET
    if processed_packet.data.get(0) == Some(&"FP".to_string()) &&
       processed_packet.data.get(1).is_some() &&
       processed_packet.data.get(2) == Some(&"GET".to_string()) {

        let flightplan_callsign = &processed_packet.data[1];
        log::info!("Flight plan acknowledgment from {} for {}", processed_packet.source, flightplan_callsign);

        // Send server acknowledgment
        // #PCserver:(own callsign):CCP:BC:(flightplan callsign):0
        let ack_packet = Packet {
            packet_type: crate::packet::PacketType::Client,
            command: "PC".to_string(),
            source: "server".to_string(),
            destination: processed_packet.source.clone(),
            data: vec![
                "CCP".to_string(),
                "BC".to_string(),
                flightplan_callsign.clone(),
                "0".to_string(),
            ],
        };
        let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(ack_packet)));
        return;
    }

    // Broadcast message to all clients
    let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(processed_packet)));
}
