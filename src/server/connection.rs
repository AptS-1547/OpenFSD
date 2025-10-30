use crate::client::Client;
use crate::packet::Packet;
use crate::server::config::ServerMessage;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, RwLock};

/// Generate a random 22-character hexadecimal token for server identification
pub fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..22)
        .map(|_| format!("{:x}", rng.gen_range(0..16)))
        .collect()
}

/// Send a text message to a client
pub async fn send_text_message(
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

/// Handle individual client connection
pub async fn handle_client(
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
            generate_token(),
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
