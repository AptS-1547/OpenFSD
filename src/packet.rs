use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PacketError {
    #[error("Invalid packet format: {0}")]
    InvalidFormat(String),
    #[error("Missing field: {0}")]
    MissingField(String),
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// FSD packet types based on command prefix
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketType {
    /// $ prefix - Requests and responses
    Request,
    /// # prefix - Adding/removing clients, text messages
    Client,
    /// % prefix - ATC update
    AtcUpdate,
    /// @ prefix - Aircraft update
    PilotUpdate,
    /// ! prefix - IVAO specific
    IvaoSpecific,
    /// & prefix - IVAO specific
    IvaoData,
    /// - prefix - IVAO specific
    IvaoOther,
}

/// FSD packet representation
#[derive(Debug, Clone)]
pub struct Packet {
    pub packet_type: PacketType,
    pub command: String,
    pub destination: String,
    pub source: String,
    pub data: Vec<String>,
}

impl Packet {
    /// Parse a raw FSD packet string
    pub fn parse(raw: &str) -> Result<Self, PacketError> {
        let raw = raw.trim_end_matches("\r\n").trim();
        
        if raw.is_empty() {
            return Err(PacketError::InvalidFormat("Empty packet".to_string()));
        }

        // Determine packet type from prefix
        let first_char = raw.chars().next().unwrap();
        let packet_type = match first_char {
            '$' => PacketType::Request,
            '#' => PacketType::Client,
            '%' => PacketType::AtcUpdate,
            '@' => PacketType::PilotUpdate,
            '!' => PacketType::IvaoSpecific,
            '&' => PacketType::IvaoData,
            '-' => PacketType::IvaoOther,
            _ => return Err(PacketError::InvalidFormat(format!("Unknown prefix: {}", first_char))),
        };

        // Remove the prefix
        let without_prefix = &raw[1..];
        
        // Find the first colon to separate (command+identifier) from the rest
        let first_colon = without_prefix.find(':')
            .ok_or_else(|| PacketError::InvalidFormat("No colon found".to_string()))?;
        
        let command_ident = &without_prefix[..first_colon];
        let rest = &without_prefix[first_colon + 1..];
        
        // Extract command and first identifier
        let (command, first_ident) = Self::split_command_source(command_ident);
        
        // Split remaining parts by colons
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        
        if parts.is_empty() {
            return Err(PacketError::InvalidFormat("Not enough fields".to_string()));
        }

        let second_ident = parts[0].to_string();
        
        // Determine which is source and which is destination based on command
        // For server identification (DI), format is: command+destination:source
        // For most others (ID, TM, AA, AP, etc.), format is: command+source:destination
        // For position updates (@), format is: command+destination:other_data
        let (source, destination) = if command == "DI" {
            // Server identification: destination comes first
            (second_ident, first_ident)
        } else if packet_type == PacketType::PilotUpdate || packet_type == PacketType::AtcUpdate {
            // Position updates: first identifier is the destination (subject of update)
            (String::new(), first_ident)  // Source is implicit (the sender)
        } else {
            // Default case (ID, TM, AA, AP, etc.): source comes first
            (first_ident, second_ident)
        };
        
        let data = if parts.len() > 1 {
            parts[1].split(':').map(|s| s.to_string()).collect()
        } else {
            Vec::new()
        };

        Ok(Packet {
            packet_type,
            command,
            destination,
            source,
            data,
        })
    }

    /// Split command and identifier from combined string
    /// Commands are typically 1-2 characters (DI, ID, TM, AA, AP, N, S, Y, etc.)
    /// Returns (command, identifier) where identifier could be source or destination depending on context
    fn split_command_source(s: &str) -> (String, String) {
        // Try to identify command by known patterns
        if s.len() >= 2 {
            let first_two = &s[..2];
            // Known 2-character commands
            if matches!(first_two, "DI" | "ID" | "TM" | "AA" | "AP" | "DA" | "DP" | "CQ" | "CR" | "FP" | "NV") {
                return (first_two.to_string(), s[2..].to_string());
            }
        }
        
        // Single character commands (for position updates, etc.)
        if !s.is_empty() {
            let first_char = &s[..1];
            if matches!(first_char, "N" | "S" | "Y" | "C" | "R") {
                return (first_char.to_string(), s[1..].to_string());
            }
        }
        
        // Default: assume 2-character command
        if s.len() >= 2 {
            (s[..2].to_string(), s[2..].to_string())
        } else {
            (s.to_string(), String::new())
        }
    }

    /// Format the packet back to FSD protocol string
    pub fn format(&self) -> String {
        let prefix = match self.packet_type {
            PacketType::Request => '$',
            PacketType::Client => '#',
            PacketType::AtcUpdate => '%',
            PacketType::PilotUpdate => '@',
            PacketType::IvaoSpecific => '!',
            PacketType::IvaoData => '&',
            PacketType::IvaoOther => '-',
        };

        // Handle different formats based on command type
        let mut result = if self.command == "DI" {
            // Server identification: command+destination:source
            format!("{}{}{}:{}",prefix, self.command, self.destination, self.source)
        } else if self.packet_type == PacketType::PilotUpdate || self.packet_type == PacketType::AtcUpdate {
            // Position updates: command+destination:data (no separate source field)
            format!("{}{}{}",prefix, self.command, self.destination)
        } else {
            // Default: command+source:destination
            format!("{}{}{}:{}",prefix, self.command, self.source, self.destination)
        };
        
        if !self.data.is_empty() {
            result.push(':');
            result.push_str(&self.data.join(":"));
        }
        
        result.push_str("\r\n");
        result
    }
}

impl fmt::Display for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format().trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_server_identification() {
        let raw = "$DISERVER:CLIENT:VATSIM FSD V3.13:ABCD1234567890ABCD1234\r\n";
        let packet = Packet::parse(raw).unwrap();
        
        assert_eq!(packet.packet_type, PacketType::Request);
        assert_eq!(packet.command, "DI");
        assert_eq!(packet.destination, "SERVER");
        assert_eq!(packet.source, "CLIENT");
        assert_eq!(packet.data.len(), 2);
        assert_eq!(packet.data[0], "VATSIM FSD V3.13");
    }

    #[test]
    fn test_parse_client_identification() {
        let raw = "$IDUAX123:SERVER:69d7:EuroScope 3.2:3:2:1234567:987654321\r\n";
        let packet = Packet::parse(raw).unwrap();
        
        assert_eq!(packet.command, "ID");
        assert_eq!(packet.source, "UAX123");
        assert_eq!(packet.data[0], "69d7");
    }

    #[test]
    fn test_parse_text_message() {
        let raw = "#TMUAX123:BAW456:Hello there\r\n";
        let packet = Packet::parse(raw).unwrap();
        
        assert_eq!(packet.packet_type, PacketType::Client);
        assert_eq!(packet.command, "TM");
        assert_eq!(packet.source, "UAX123");
        assert_eq!(packet.destination, "BAW456");
        assert_eq!(packet.data[0], "Hello there");
    }

    #[test]
    fn test_parse_position_update() {
        let raw = "@NUAX123:1200:1:45.5:-73.5:35000:450:123456789:50\r\n";
        let packet = Packet::parse(raw).unwrap();
        
        assert_eq!(packet.packet_type, PacketType::PilotUpdate);
        assert_eq!(packet.command, "N");
        assert_eq!(packet.destination, "UAX123");
    }

    #[test]
    fn test_format_packet() {
        let packet = Packet {
            packet_type: PacketType::Request,
            command: "DI".to_string(),
            destination: "SERVER".to_string(),
            source: "CLIENT".to_string(),
            data: vec!["VATSIM FSD V3.13".to_string(), "TOKEN123".to_string()],
        };
        
        let formatted = packet.format();
        assert!(formatted.starts_with("$DISERVER:CLIENT:"));
        assert!(formatted.ends_with("\r\n"));
    }
}
