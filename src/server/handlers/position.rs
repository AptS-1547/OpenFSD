use crate::packet::Packet;
use crate::server::config::ServerMessage;
use std::net::SocketAddr;
use tokio::sync::broadcast;

/// Handle position update
pub async fn handle_position_update(
    packet: Packet,
    sender_addr: SocketAddr,
    broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
) {
    log::debug!(
        "Position update from {}: {}",
        sender_addr,
        packet.destination
    );

    // Check for emergency squawk code (7500) - immediate disconnect
    // Position update format for pilots: @(mode):(callsign):(squawk):(rating):(lat):(lon):(alt):(groundspeed):(num1):(num2)
    if packet.packet_type == crate::packet::PacketType::PilotUpdate {
        if let Some(squawk) = packet.data.get(1) {
            if squawk == "7500" {
                log::warn!(
                    "Squawk 7500 (hijacking) detected from {} - immediate disconnect",
                    packet.source
                );

                // Send disconnect message
                let _ = broadcast_tx.send((sender_addr, ServerMessage::Disconnect));
                return;
            }
        }
    }

    // Broadcast position update to all clients
    let _ = broadcast_tx.send((sender_addr, ServerMessage::Packet(packet)));
}
