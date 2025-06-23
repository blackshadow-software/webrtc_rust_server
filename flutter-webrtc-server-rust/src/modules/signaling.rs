use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use chrono::{Duration, Utc};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::sync::Arc;
use tokio::sync::mpsc;

const SHARED_KEY: &str = "flutter-webrtc-turn-server-shared-key";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCredentials {
    pub username: String,
    pub password: String,
    pub ttl: i64,
    pub uris: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
    pub user_agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Negotiation {
    pub from: String,
    pub to: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Byebye {
    pub session_id: String,
    pub from: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingError {
    pub request: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Method {
    #[serde(rename = "new")]
    New(PeerInfo),
    #[serde(rename = "bye")]
    Bye(Byebye),
    #[serde(rename = "offer")]
    Offer(serde_json::Value),
    #[serde(rename = "answer")]
    Answer(serde_json::Value),
    #[serde(rename = "candidate")]
    Candidate(serde_json::Value),
    #[serde(rename = "leave")]
    Leave(String),
    #[serde(rename = "keepalive")]
    Keepalive,
    #[serde(rename = "peers")]
    Peers(Vec<PeerInfo>),
    #[serde(rename = "error")]
    Error(SignalingError),
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub info: PeerInfo,
    pub sender: mpsc::UnboundedSender<Method>,
}

#[derive(Debug, Clone)]
pub struct ExpiredCredential {
    pub credential: TurnCredentials,
    pub expires_at: chrono::DateTime<Utc>,
}

pub struct Signaler {
    pub peers: Arc<DashMap<String, Peer>>,
    pub turn_credentials: Arc<DashMap<String, ExpiredCredential>>,
    pub turn_config: crate::modules::config::TurnConfig,
}

impl Signaler {
    pub fn new(turn_config: crate::modules::config::TurnConfig) -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            turn_credentials: Arc::new(DashMap::new()),
            turn_config,
        }
    }

    pub fn generate_turn_credentials(&self, username: &str) -> Result<TurnCredentials> {
        let timestamp = Utc::now().timestamp();
        let turn_username = format!("{}:{}", timestamp, username);
        
        let mut mac = Hmac::<Sha1>::new_from_slice(SHARED_KEY.as_bytes())?;
        mac.update(turn_username.as_bytes());
        let turn_password = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, mac.finalize().into_bytes());

        let ttl = 86400; // 24 hours
        let host = format!("{}:{}", self.turn_config.public_ip, self.turn_config.port);
        
        let credentials = TurnCredentials {
            username: turn_username.clone(),
            password: turn_password,
            ttl,
            uris: vec![format!("turn:{}?transport=udp", host)],
        };

        // Store credentials with expiration
        let expires_at = Utc::now() + Duration::seconds(ttl);
        self.turn_credentials.insert(
            turn_username,
            ExpiredCredential {
                credential: credentials.clone(),
                expires_at,
            },
        );

        Ok(credentials)
    }

    pub fn validate_turn_credentials(&self, username: &str) -> Option<String> {
        if let Some(entry) = self.turn_credentials.get(username) {
            if entry.expires_at > Utc::now() {
                return Some(entry.credential.password.clone());
            } else {
                // Remove expired credentials
                self.turn_credentials.remove(username);
            }
        }
        None
    }

    pub fn notify_peers_update(&self) {
        let peer_infos: Vec<PeerInfo> = self.peers.iter().map(|entry| entry.value().info.clone()).collect();
        let message = Method::Peers(peer_infos);

        for peer in self.peers.iter() {
            if let Err(e) = peer.value().sender.send(message.clone()) {
                error!("Failed to send peers update to {}: {}", peer.key(), e);
            }
        }
    }

    pub async fn handle_websocket(&self, socket: WebSocket) {
        let (mut sender, mut receiver) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Method>();
        
        let peer_id = Arc::new(tokio::sync::Mutex::new(None::<String>));
        let peer_id_clone = peer_id.clone();
        let peers_clone = self.peers.clone();

        // Spawn task to handle outgoing messages
        let send_task = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let msg_json = match serde_json::to_string(&message) {
                    Ok(json) => json,
                    Err(e) => {
                        error!("Failed to serialize message: {}", e);
                        continue;
                    }
                };

                if sender.send(Message::Text(msg_json)).await.is_err() {
                    break;
                }
            }
        });

        // Handle incoming messages
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Err(e) = self.handle_message(text, &tx, &peer_id_clone).await {
                        error!("Error handling message: {}", e);
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        // Cleanup on disconnect
        if let Some(id) = peer_id.lock().await.as_ref() {
            info!("Removing peer: {}", id);
            peers_clone.remove(id);
            self.notify_peers_update();
        }

        send_task.abort();
    }

    async fn handle_message(
        &self,
        text: String,
        sender: &mpsc::UnboundedSender<Method>,
        peer_id: &Arc<tokio::sync::Mutex<Option<String>>>,
    ) -> Result<()> {
        debug!("Received message: {}", text);
        
        let message: Method = serde_json::from_str(&text)?;

        match message {
            Method::New(peer_info) => {
                info!("New peer: {} ({})", peer_info.name, peer_info.id);
                
                let peer = Peer {
                    info: peer_info.clone(),
                    sender: sender.clone(),
                };
                
                self.peers.insert(peer_info.id.clone(), peer);
                *peer_id.lock().await = Some(peer_info.id);
                
                self.notify_peers_update();
            }
            Method::Offer(ref data) | Method::Answer(ref data) | Method::Candidate(ref data) => {
                if let Ok(negotiation) = serde_json::from_value::<Negotiation>(data.clone()) {
                    if let Some(target_peer) = self.peers.get(&negotiation.to) {
                        let relay_message = match &message {
                            Method::Offer(_) => Method::Offer(serde_json::to_value(&negotiation)?),
                            Method::Answer(_) => Method::Answer(serde_json::to_value(&negotiation)?),
                            Method::Candidate(_) => Method::Candidate(serde_json::to_value(&negotiation)?),
                            _ => unreachable!(),
                        };
                        
                        if let Err(_) = target_peer.sender.send(relay_message) {
                            let error_msg = Method::Error(SignalingError {
                                request: format!("{:?}", message),
                                reason: format!("Peer [{}] not reachable", negotiation.to),
                            });
                            let _ = sender.send(error_msg);
                        }
                    } else {
                        let error_msg = Method::Error(SignalingError {
                            request: format!("{:?}", message),
                            reason: format!("Peer [{}] not found", negotiation.to),
                        });
                        let _ = sender.send(error_msg);
                    }
                }
            }
            Method::Bye(bye) => {
                let session_parts: Vec<&str> = bye.session_id.split('-').collect();
                if session_parts.len() == 2 {
                    for &peer_id in &session_parts {
                        if let Some(peer) = self.peers.get(peer_id) {
                            let bye_message = Method::Bye(Byebye {
                                session_id: bye.session_id.clone(),
                                from: bye.from.clone(),
                            });
                            let _ = peer.sender.send(bye_message);
                        }
                    }
                }
            }
            Method::Keepalive => {
                let _ = sender.send(Method::Keepalive);
            }
            _ => {
                warn!("Unknown message type: {:?}", message);
            }
        }

        Ok(())
    }
}