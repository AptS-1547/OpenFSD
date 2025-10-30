# OpenFSD

A complete implementation of the FSD (Flight Simulator Display) server protocol in Rust.

## Overview

OpenFSD is a Rust implementation of the FSD protocol used by flight simulation networks like IVAO and VATSIM. This server enables communication between flight simulator clients, allowing pilots and air traffic controllers to connect and interact in a shared virtual airspace.

## Features

- ✅ Complete FSD packet parser and formatter with support for all major packet types
- ✅ High-performance async TCP server using Tokio
- ✅ Client connection management with callsign mapping
- ✅ Support for authentication packets (client ID, pilot/ATC login)
- ✅ Position updates (pilots and ATC)
- ✅ Text messaging with broadcast support
- ✅ Information requests/responses
- ✅ Flight plan handling and broadcasting
- ✅ TOML-based configuration
- ✅ Structured logging with configurable levels
- ✅ Example client demonstrating protocol usage

## Building

### Prerequisites

- Rust 1.70 or later
- Cargo

### Build Instructions

```bash
cargo build --release
```

### Running Tests

```bash
cargo test
```

## Usage

### Starting the Server

```bash
cargo run --release
```

The server will start listening on `0.0.0.0:6809` by default (standard FSD port).

### Configuration

The server can be configured using a `config.toml` file in the project root. If the file doesn't exist, the server will use default settings.

Example `config.toml`:

```toml
[server]
address = "0.0.0.0"
port = 6809
name = "OpenFSD"
version = "0.1.0"
max_clients = 1000

[logging]
level = "info"
```

### Running the Example Client

An example client is provided to demonstrate basic FSD communication:

```bash
cargo run --example simple_client
```

This will connect to a running server on `localhost:6809` and send sample packets including:
- Client identification
- Pilot login
- Position update
- Text message
- Logoff

## Architecture

The server uses a broadcast-based architecture:

1. **Client Connections**: Each client connection runs in its own Tokio task
2. **Packet Processing**: Incoming packets are sent to a central processing queue
3. **Broadcasting**: Processed packets are broadcast to relevant clients via channels
4. **Non-blocking**: All I/O operations are asynchronous using Tokio

### Packet Flow

```
Client → TCP Stream → Parser → Packet Queue → Processor → Broadcast Channel → Other Clients
```

## Protocol Documentation

The FSD protocol implementation is based on the documentation available at:
https://github.com/AptS-1547/fsd-doc

### Packet Format

FSD packets follow the general format:
```
(prefix)(command)(identifier):(field):(data)
```

Where:
- `prefix`: Single character indicating packet type (`$`, `#`, `%`, `@`, etc.)
- `command`: 1-2 character command code
- `identifier`, `field`, `data`: Vary based on packet type

## Project Structure

```
src/
├── main.rs      # Main entry point and configuration loading
├── packet.rs    # FSD packet parser and formatter
├── client.rs    # Client data structures
├── server.rs    # FSD server implementation with broadcast logic
└── config.rs    # Configuration file handling
examples/
└── simple_client.rs  # Example FSD client
config.toml      # Server configuration (optional)
```

## Development

### Code Style

This project uses standard Rust formatting:

```bash
cargo fmt
```

### Linting

```bash
cargo clippy
```

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues.

## Acknowledgments

- FSD protocol documentation: https://github.com/AptS-1547/fsd-doc
- IVAO and VATSIM for their flight simulation networks
