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

#[derive(Debug, Clone)]
pub struct CallSession {
    pub session_id: String,
    pub caller_id: String,
    pub callee_id: String,
    pub started_at: chrono::DateTime<Utc>,
    pub status: CallStatus,
}

#[derive(Debug, Clone)]
pub enum CallStatus {
    Calling,    // Offer sent, waiting for answer
    Connected,  // Answer received, call in progress
    Ended,      // Call terminated
}

pub struct Signaler {
    pub peers: Arc<DashMap<String, Peer>>,
    pub sessions: Arc<DashMap<String, CallSession>>,
    pub turn_credentials: Arc<DashMap<String, ExpiredCredential>>,
    pub turn_config: crate::modules::config::TurnConfig,
}

impl Signaler {
    pub fn new(turn_config: crate::modules::config::TurnConfig) -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            sessions: Arc::new(DashMap::new()),
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
        info!("Starting WebSocket handler for new connection");
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
        let ping_sender = tx.clone();
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received WebSocket text message: {}", text);
                    if let Err(e) = self.handle_message(text, &tx, &peer_id_clone).await {
                        error!("Error handling message: {}", e);
                    }
                }
                Ok(Message::Close(close_frame)) => {
                    info!("WebSocket connection closed gracefully: {:?}", close_frame);
                    break;
                }
                Ok(Message::Ping(data)) => {
                    debug!("Received WebSocket ping, sending pong");
                    if let Err(e) = ping_sender.send(Method::Keepalive) {
                        error!("Failed to send pong response: {}", e);
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received WebSocket pong");
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {
                    debug!("Received other WebSocket message type");
                }
            }
        }

        // Cleanup on disconnect
        if let Some(id) = peer_id.lock().await.as_ref() {
            info!("WebSocket disconnected, removing peer: {}", id);
            peers_clone.remove(id);
            self.notify_peers_update();
        } else {
            info!("WebSocket disconnected before peer registration");
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
                info!("Registering new peer: {} (ID: {}, User-Agent: {})", 
                      peer_info.name, peer_info.id, peer_info.user_agent);
                
                let peer = Peer {
                    info: peer_info.clone(),
                    sender: sender.clone(),
                };
                
                self.peers.insert(peer_info.id.clone(), peer);
                *peer_id.lock().await = Some(peer_info.id.clone());
                
                info!("Peer {} successfully registered, notifying all peers", peer_info.id);
                self.notify_peers_update();
            }
            Method::Offer(ref data) => {
                if let Ok(negotiation) = serde_json::from_value::<Negotiation>(data.clone()) {
                    info!("📞 CALL INITIATED: {} calling {} (session: {})", 
                          negotiation.from, negotiation.to, negotiation.session_id);
                    
                    // Create call session
                    let session = CallSession {
                        session_id: negotiation.session_id.clone(),
                        caller_id: negotiation.from.clone(),
                        callee_id: negotiation.to.clone(),
                        started_at: Utc::now(),
                        status: CallStatus::Calling,
                    };
                    self.sessions.insert(negotiation.session_id.clone(), session);
                    info!("📝 Call session created: {}", negotiation.session_id);
                    
                    if let Some(target_peer) = self.peers.get(&negotiation.to) {
                        info!("📤 Forwarding offer to recipient: {}", negotiation.to);
                        let relay_message = Method::Offer(data.clone());
                        
                        if let Err(e) = target_peer.sender.send(relay_message) {
                            error!("❌ Failed to deliver offer to {}: {}", negotiation.to, e);
                            // Update session status to ended
                            if let Some(mut session) = self.sessions.get_mut(&negotiation.session_id) {
                                session.status = CallStatus::Ended;
                            }
                            let error_msg = Method::Error(SignalingError {
                                request: "offer".to_string(),
                                reason: format!("Recipient [{}] unreachable", negotiation.to),
                            });
                            let _ = sender.send(error_msg);
                        } else {
                            info!("✅ Offer successfully delivered to {}", negotiation.to);
                        }
                    } else {
                        error!("❌ CALL FAILED: Recipient {} not found", negotiation.to);
                        // Update session status to ended
                        if let Some(mut session) = self.sessions.get_mut(&negotiation.session_id) {
                            session.status = CallStatus::Ended;
                        }
                        let error_msg = Method::Error(SignalingError {
                            request: "offer".to_string(),
                            reason: format!("Recipient [{}] not available", negotiation.to),
                        });
                        let _ = sender.send(error_msg);
                    }
                } else {
                    error!("❌ Invalid offer format: {:?}", data);
                }
            }
            Method::Answer(ref data) => {
                if let Ok(negotiation) = serde_json::from_value::<Negotiation>(data.clone()) {
                    info!("📞 CALL ANSWERED: {} answered call from {} (session: {})", 
                          negotiation.from, negotiation.to, negotiation.session_id);
                    
                    // Update session status to connected
                    if let Some(mut session) = self.sessions.get_mut(&negotiation.session_id) {
                        session.status = CallStatus::Connected;
                        info!("🔗 Call session connected: {}", negotiation.session_id);
                    } else {
                        warn!("⚠️ No session found for answer: {}", negotiation.session_id);
                    }
                    
                    if let Some(target_peer) = self.peers.get(&negotiation.to) {
                        info!("📤 Forwarding answer to caller: {}", negotiation.to);
                        let relay_message = Method::Answer(data.clone());
                        
                        if let Err(e) = target_peer.sender.send(relay_message) {
                            error!("❌ Failed to deliver answer to {}: {}", negotiation.to, e);
                            // Update session status to ended
                            if let Some(mut session) = self.sessions.get_mut(&negotiation.session_id) {
                                session.status = CallStatus::Ended;
                            }
                            let error_msg = Method::Error(SignalingError {
                                request: "answer".to_string(),
                                reason: format!("Caller [{}] unreachable", negotiation.to),
                            });
                            let _ = sender.send(error_msg);
                        } else {
                            info!("✅ Answer successfully delivered to {}", negotiation.to);
                        }
                    } else {
                        error!("❌ ANSWER FAILED: Caller {} not found", negotiation.to);
                        // Update session status to ended
                        if let Some(mut session) = self.sessions.get_mut(&negotiation.session_id) {
                            session.status = CallStatus::Ended;
                        }
                        let error_msg = Method::Error(SignalingError {
                            request: "answer".to_string(),
                            reason: format!("Caller [{}] no longer available", negotiation.to),
                        });
                        let _ = sender.send(error_msg);
                    }
                } else {
                    error!("❌ Invalid answer format: {:?}", data);
                }
            }
            Method::Candidate(ref data) => {
                if let Ok(negotiation) = serde_json::from_value::<Negotiation>(data.clone()) {
                    debug!("🔗 ICE candidate from {} to {} (session: {})", 
                          negotiation.from, negotiation.to, negotiation.session_id);
                    
                    if let Some(target_peer) = self.peers.get(&negotiation.to) {
                        let relay_message = Method::Candidate(data.clone());
                        
                        if let Err(e) = target_peer.sender.send(relay_message) {
                            warn!("⚠️ Failed to relay ICE candidate to {}: {}", negotiation.to, e);
                        } else {
                            debug!("✅ ICE candidate relayed to {}", negotiation.to);
                        }
                    } else {
                        warn!("⚠️ ICE candidate target peer {} not found", negotiation.to);
                    }
                } else {
                    error!("❌ Invalid ICE candidate format: {:?}", data);
                }
            }
            Method::Bye(bye) => {
                info!("📞 CALL ENDED: {} ended call for session {}", bye.from, bye.session_id);
                
                // Update session status to ended
                if let Some(mut session) = self.sessions.get_mut(&bye.session_id) {
                    session.status = CallStatus::Ended;
                    info!("📝 Call session ended: {}", bye.session_id);
                }
                
                let session_parts: Vec<&str> = bye.session_id.split('-').collect();
                if session_parts.len() == 2 {
                    for &peer_id in &session_parts {
                        if peer_id != bye.from { // Don't send bye back to sender
                            if let Some(peer) = self.peers.get(peer_id) {
                                info!("📤 Notifying {} that call ended", peer_id);
                                let bye_message = Method::Bye(Byebye {
                                    session_id: bye.session_id.clone(),
                                    from: bye.from.clone(),
                                });
                                if let Err(e) = peer.sender.send(bye_message) {
                                    error!("❌ Failed to notify {} of call end: {}", peer_id, e);
                                } else {
                                    info!("✅ Call end notification sent to {}", peer_id);
                                }
                            } else {
                                warn!("⚠️ Peer {} not found for call end notification", peer_id);
                            }
                        }
                    }
                } else {
                    warn!("⚠️ Invalid session ID format for bye message: {}", bye.session_id);
                }
            }
            Method::Keepalive => {
                debug!("Received keepalive, responding with keepalive");
                if let Err(e) = sender.send(Method::Keepalive) {
                    error!("Failed to send keepalive response: {}", e);
                }
            }
            _ => {
                warn!("Received unknown/unhandled message type: {:?}", message);
            }
        }

        Ok(())
    }
}