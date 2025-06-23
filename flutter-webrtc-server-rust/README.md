# Flutter WebRTC Server - Rust Implementation

A Rust implementation of the WebRTC signaling server, providing the exact same features as the original Go version.

## Features

- ✅ WebRTC Signaling Server with WebSocket support
- ✅ Built-in TURN server for NAT traversal  
- ✅ HTTPS/WSS support with TLS certificates
- ✅ REST API for TURN credentials
- ✅ Static file serving for web assets
- ✅ Peer discovery and management
- ✅ Session management (offer/answer/candidate/bye)
- ✅ HMAC-SHA1 authentication for TURN
- ✅ Configurable via INI file

## Quick Start

### Prerequisites

- Rust 1.70+ 
- TLS certificates (can use mkcert for development)

### Setup

1. **Clone and build:**
```bash
cd flutter-webrtc-server-rust
cargo build --release
```

2. **Generate TLS certificates:**
```bash
# Install mkcert (macOS)
brew install mkcert

# Generate certificates
mkcert -key-file configs/certs/key.pem -cert-file configs/certs/cert.pem localhost 127.0.0.1 ::1 0.0.0.0
```

3. **Configure TURN server:**
Edit `configs/config.ini`:
```ini
[turn]
public_ip=YOUR_PUBLIC_IP_OR_DOMAIN
username=your_turn_username  
password=your_turn_password
```

4. **Run the server:**
```bash
cargo run --release
```

5. **Access the demo:**
Open `https://localhost:8086` in your browser.

## API Endpoints

- **WebSocket:** `wss://localhost:8086/ws`
- **TURN Credentials:** `GET /api/turn?service=turn&username=<username>`
- **Static Files:** `GET /*` (serves from `web/` directory)

## Configuration

All settings are in `configs/config.ini`:

```ini
[general]
domain=demo.cloudwebrtc.com
cert=configs/certs/cert.pem
key=configs/certs/key.pem  
bind=0.0.0.0
port=8086
html_root=web

[turn]
public_ip=<YOUR_PUBLIC_IP>
port=19302
realm=flutter-webrtc
username=<TURN_USERNAME>
password=<TURN_PASSWORD>
```

## WebSocket Protocol

The signaling protocol supports these message types:

- `new` - Register new peer
- `offer/answer/candidate` - WebRTC negotiation 
- `bye` - End session
- `leave` - Disconnect peer
- `keepalive` - Connection heartbeat

## Project Structure

```
src/
├── main.rs              # HTTP/WebSocket server
└── modules/
    ├── config.rs        # Configuration management
    ├── signaling.rs     # WebRTC signaling logic
    └── turn_server.rs   # TURN server implementation
```

## Comparison with Go Version

This Rust implementation provides 100% feature parity with the original Go server:

| Feature | Go Version | Rust Version |
|---------|------------|--------------|
| WebSocket Signaling | ✅ | ✅ |
| TURN Server | ✅ | ✅ |
| TLS/HTTPS Support | ✅ | ✅ |
| TURN Credentials API | ✅ | ✅ |
| Static File Serving | ✅ | ✅ |
| INI Configuration | ✅ | ✅ |
| HMAC Authentication | ✅ | ✅ |

## Development

- **Dependencies:** All managed via Cargo.toml
- **Logging:** Set `RUST_LOG=debug` for verbose output
- **Testing:** Use with [flutter-webrtc-demo](https://github.com/cloudwebrtc/flutter-webrtc-demo)

## License

Same as original project.