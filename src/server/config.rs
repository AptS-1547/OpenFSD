use crate::packet::Packet;

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
