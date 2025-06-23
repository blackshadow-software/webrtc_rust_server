use anyhow::Result;
use log::{error, info, warn};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

pub struct TurnServer {
    config: crate::modules::config::TurnConfig,
    signaler: Arc<crate::modules::signaling::Signaler>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TurnServer {
    pub fn new(
        config: crate::modules::config::TurnConfig,
        signaler: Arc<crate::modules::signaling::Signaler>,
    ) -> Self {
        Self {
            config,
            signaler,
            server_handle: None,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.config.public_ip.contains("YOUR PUBLIC IP") {
            warn!("TURN server public IP not configured, skipping TURN server startup");
            return Ok(());
        }

        let bind_addr: SocketAddr = format!("0.0.0.0:{}", self.config.port).parse()?;
        
        info!("Starting TURN server on {}", bind_addr);

        // Create UDP socket for TURN server
        let socket = Arc::new(UdpSocket::bind(bind_addr).await?);
        info!("TURN server UDP socket bound to {}", bind_addr);

        // Create simple TURN relay server
        let turn_relay = SimpleTurnRelay::new(socket, self.signaler.clone(), self.config.clone());
        
        // Start server in background task
        let handle = tokio::spawn(async move {
            info!("TURN server started and listening for connections");
            if let Err(e) = turn_relay.run().await {
                error!("TURN server error: {}", e);
            }
        });

        self.server_handle = Some(handle);
        info!("TURN server successfully started on {}", bind_addr);
        
        Ok(())
    }

    pub async fn close(&mut self) -> Result<()> {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
            info!("TURN server stopped");
        }
        Ok(())
    }
}

struct SimpleTurnRelay {
    socket: Arc<UdpSocket>,
    signaler: Arc<crate::modules::signaling::Signaler>,
    config: crate::modules::config::TurnConfig,
    allocations: Arc<Mutex<HashMap<SocketAddr, TurnAllocation>>>,
}

#[derive(Clone)]
struct TurnAllocation {
    client_addr: SocketAddr,
    relay_addr: SocketAddr,
    username: String,
}

impl SimpleTurnRelay {
    fn new(
        socket: Arc<UdpSocket>,
        signaler: Arc<crate::modules::signaling::Signaler>,
        config: crate::modules::config::TurnConfig,
    ) -> Self {
        Self {
            socket,
            signaler,
            config,
            allocations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn run(self) -> Result<()> {
        let mut buffer = [0u8; 65536];
        
        loop {
            match self.socket.recv_from(&mut buffer).await {
                Ok((len, addr)) => {
                    let data = &buffer[..len];
                    
                    // Check if this is a STUN/TURN message
                    if len >= 20 && self.is_stun_message(data) {
                        if let Err(e) = self.handle_stun_message(data, addr).await {
                            warn!("Error handling STUN message from {}: {}", addr, e);
                        }
                    } else {
                        // Handle data relay
                        if let Err(e) = self.handle_data_relay(data, addr).await {
                            warn!("Error handling data relay from {}: {}", addr, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Error receiving UDP packet: {}", e);
                    break;
                }
            }
        }
        
        Ok(())
    }

    fn is_stun_message(&self, data: &[u8]) -> bool {
        if data.len() < 20 {
            return false;
        }
        
        // STUN message type is in first 2 bytes
        // STUN messages start with 0x00 or 0x01 in first byte
        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        
        // Check for common STUN/TURN message types
        matches!(msg_type & 0xFF00, 0x0000 | 0x0100)
    }

    async fn handle_stun_message(&self, data: &[u8], addr: SocketAddr) -> Result<()> {
        info!("Received STUN/TURN message from {} ({} bytes)", addr, data.len());
        
        // For now, we'll implement basic STUN binding response
        // This is a simplified implementation - a full TURN server would need
        // proper STUN message parsing and TURN protocol implementation
        
        let response = self.create_binding_response(addr)?;
        
        if let Err(e) = self.socket.send_to(&response, addr).await {
            warn!("Failed to send STUN response to {}: {}", addr, e);
        } else {
            info!("Sent STUN binding response to {}", addr);
        }
        
        Ok(())
    }

    async fn handle_data_relay(&self, data: &[u8], addr: SocketAddr) -> Result<()> {
        // Simple data relay logic
        let allocations = self.allocations.lock().await;
        
        if let Some(allocation) = allocations.get(&addr) {
            info!("Relaying {} bytes from {} to {}", data.len(), addr, allocation.relay_addr);
            // In a real implementation, we would relay to the target
        }
        
        Ok(())
    }

    fn create_binding_response(&self, client_addr: SocketAddr) -> Result<Vec<u8>> {
        // Create a basic STUN Binding Success Response
        // This is a simplified implementation
        let mut response = Vec::new();
        
        // STUN header: Message Type (Binding Success Response = 0x0101)
        response.extend_from_slice(&0x0101u16.to_be_bytes());
        
        // Message Length (will be updated)
        let length_pos = response.len();
        response.extend_from_slice(&0u16.to_be_bytes());
        
        // Magic Cookie
        response.extend_from_slice(&0x2112A442u32.to_be_bytes());
        
        // Transaction ID (12 bytes) - simplified random
        response.extend_from_slice(&[0u8; 12]);
        
        // XOR-MAPPED-ADDRESS attribute
        response.extend_from_slice(&0x0020u16.to_be_bytes()); // Attribute type
        response.extend_from_slice(&0x0008u16.to_be_bytes()); // Attribute length
        response.push(0x00); // Reserved
        response.push(0x01); // Family (IPv4)
        
        // Port XOR'd with magic cookie
        let port = client_addr.port() ^ 0x2112;
        response.extend_from_slice(&port.to_be_bytes());
        
        // IP XOR'd with magic cookie
        if let SocketAddr::V4(addr_v4) = client_addr {
            let ip_bytes = addr_v4.ip().octets();
            let magic_bytes = 0x2112A442u32.to_be_bytes();
            for (i, &byte) in ip_bytes.iter().enumerate() {
                response.push(byte ^ magic_bytes[i]);
            }
        }
        
        // Update message length
        let attr_length = response.len() - 20; // Exclude header
        response[length_pos..length_pos + 2].copy_from_slice(&(attr_length as u16).to_be_bytes());
        
        Ok(response)
    }
}