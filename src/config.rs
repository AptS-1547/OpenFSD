use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub address: String,
    pub port: u16,
    pub name: String,
    pub version: String,
    pub max_clients: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    pub level: String,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Create default configuration
    pub fn default() -> Self {
        Self {
            server: ServerConfig {
                address: "0.0.0.0".to_string(),
                port: 6809,
                name: "OpenFSD".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                max_clients: 1000,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
        }
    }
}

impl From<Config> for crate::server::ServerConfig {
    fn from(config: Config) -> Self {
        Self {
            address: config.server.address,
            port: config.server.port,
            server_name: config.server.name,
            server_version: config.server.version,
            max_clients: config.server.max_clients,
        }
    }
}
