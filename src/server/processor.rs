use crate::client::Client;
use crate::packet::Packet;
use crate::server::config::{ServerConfig, ServerMessage};
use crate::server::handlers;
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Process incoming packets and route to appropriate handlers
pub async fn process_packet(
    packet: Packet,
    sender_addr: SocketAddr,
    clients: &Arc<RwLock<HashMap<SocketAddr, Client>>>,
    callsign_map: &Arc<RwLock<HashMap<String, SocketAddr>>>,
    config: &ServerConfig,
    broadcast_tx: &broadcast::Sender<(SocketAddr, ServerMessage)>,
    db: &Arc<DatabaseConnection>,
) {
    log::debug!("Processing packet from {}: {}", sender_addr, packet);

    match packet.command.as_str() {
        "ID" => {
            handlers::handle_identification(
                packet,
                sender_addr,
                clients,
                callsign_map,
                config,
                broadcast_tx,
                db,
            )
            .await
        }
        "AA" | "AP" => {
            handlers::handle_login(packet, sender_addr, clients, callsign_map, broadcast_tx, db).await
        }
        "DA" | "DP" => {
            handlers::handle_logoff(packet, sender_addr, clients, callsign_map, broadcast_tx).await
        }
        "TM" => {
            handlers::handle_text_message(packet, sender_addr, broadcast_tx).await
        }
        "CQ" => {
            handlers::handle_request(packet, sender_addr, clients, broadcast_tx).await
        }
        "CR" => {
            handlers::handle_response(packet, sender_addr, broadcast_tx).await
        }
        "AX" => {
            handlers::handle_metar_request(packet, sender_addr, broadcast_tx).await
        }
        "N" | "S" | "Y" => {
            handlers::handle_position_update(packet, sender_addr, broadcast_tx).await
        }
        "FP" => handlers::handle_flight_plan(packet, sender_addr, broadcast_tx).await,
        _ => {
            log::debug!("Unhandled command: {}", packet.command);
        }
    }
}
