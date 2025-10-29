# OpenFSD

A complete implementation of the FSD (Flight Simulator Display) server protocol in Rust.

## Overview

OpenFSD is a Rust implementation of the FSD protocol used by flight simulation networks like IVAO and VATSIM. This server enables communication between flight simulator clients, allowing pilots and air traffic controllers to connect and interact in a shared virtual airspace.

## Features

- ✅ Complete FSD packet parser and formatter
- ✅ TCP server infrastructure
- ✅ Client connection management
- ✅ Support for authentication packets
- ✅ Position updates (pilots and ATC)
- ✅ Text messaging
- ✅ Information requests/responses
- ✅ Flight plan handling

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

The server can be configured by modifying the `ServerConfig` in `src/main.rs`:

```rust
let config = ServerConfig {
    address: "0.0.0.0".to_string(),
    port: 6809,
    server_name: "OpenFSD".to_string(),
    server_version: "0.1.0".to_string(),
    max_clients: 1000,
};
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
├── main.rs      # Main entry point
├── packet.rs    # FSD packet parser and formatter
├── client.rs    # Client connection handling
└── server.rs    # FSD server implementation
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
