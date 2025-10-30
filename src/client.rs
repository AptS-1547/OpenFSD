use crate::packet::Packet;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

/// Client connection state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientState {
    /// Just connected, waiting for identification
    Connected,
    /// Identified but not logged in
    Identified,
    /// Logged in and active
    Active,
    /// Disconnected
    Disconnected,
}

/// Client type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientType {
    Pilot,
    Atc,
    Observer,
}

/// Represents a connected client
#[derive(Debug)]
pub struct Client {
    pub callsign: Option<String>,
    pub addr: SocketAddr,
    pub state: ClientState,
    pub client_type: Option<ClientType>,
    pub real_name: Option<String>,
    pub network_id: Option<String>,
    pub rating: Option<i32>,
    pub client_string: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<i32>,
}

impl Client {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            callsign: None,
            addr,
            state: ClientState::Connected,
            client_type: None,
            real_name: None,
            network_id: None,
            rating: None,
            client_string: None,
            latitude: None,
            longitude: None,
            altitude: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.state == ClientState::Active
    }

    pub fn callsign(&self) -> Option<&str> {
        self.callsign.as_deref()
    }
}

/// Client connection handler
pub struct ClientConnection {
    stream: TcpStream,
    addr: SocketAddr,
    tx: mpsc::Sender<Packet>,
}

impl ClientConnection {
    pub fn new(stream: TcpStream, addr: SocketAddr, tx: mpsc::Sender<Packet>) -> Self {
        Self { stream, addr, tx }
    }

    /// Handle the client connection
    pub async fn handle(self) -> Result<(), Box<dyn std::error::Error>> {
        let (reader, mut writer) = self.stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        log::info!("Client connected from {}", self.addr);

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            
            if bytes_read == 0 {
                log::info!("Client {} disconnected", self.addr);
                break;
            }

            match Packet::parse(&line) {
                Ok(packet) => {
                    log::debug!("Received packet from {}: {}", self.addr, packet);
                    
                    // Send packet to server for processing
                    if self.tx.send(packet).await.is_err() {
                        log::error!("Failed to send packet to server");
                        break;
                    }
                }
                Err(e) => {
                    log::warn!("Failed to parse packet from {}: {}", self.addr, e);
                }
            }
        }

        Ok(())
    }

    /// Send a packet to the client
    pub async fn send_packet(&mut self, packet: &Packet) -> Result<(), Box<dyn std::error::Error>> {
        let formatted = packet.format();
        self.stream.write_all(formatted.as_bytes()).await?;
        self.stream.flush().await?;
        Ok(())
    }
}
