use anyhow::Result;
use ini::Ini;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub domain: String,
    pub cert: String,
    pub key: String,
    pub bind: String,
    pub port: u16,
    pub html_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnConfig {
    pub public_ip: String,
    pub port: u16,
    pub realm: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    pub turn: TurnConfig,
}

impl Config {
    pub fn load_from_file(path: &str) -> Result<Self> {
        let conf = Ini::load_from_file(path)?;

        let general_section = conf.section(Some("general")).ok_or_else(|| {
            anyhow::anyhow!("Missing [general] section in config file")
        })?;

        let turn_section = conf.section(Some("turn")).ok_or_else(|| {
            anyhow::anyhow!("Missing [turn] section in config file")
        })?;

        let general = GeneralConfig {
            domain: general_section.get("domain").unwrap_or("localhost").to_string(),
            cert: general_section.get("cert").unwrap_or("configs/certs/cert.pem").to_string(),
            key: general_section.get("key").unwrap_or("configs/certs/key.pem").to_string(),
            bind: general_section.get("bind").unwrap_or("0.0.0.0").to_string(),
            port: general_section.get("port").unwrap_or("8086").parse().unwrap_or(8086),
            html_root: general_section.get("html_root").unwrap_or("web").to_string(),
        };

        let turn = TurnConfig {
            public_ip: turn_section.get("public_ip").unwrap_or("127.0.0.1").to_string(),
            port: turn_section.get("port").unwrap_or("19302").parse().unwrap_or(19302),
            realm: turn_section.get("realm").unwrap_or("flutter-webrtc").to_string(),
            username: turn_section.get("username").unwrap_or("user").to_string(),
            password: turn_section.get("password").unwrap_or("password").to_string(),
        };

        Ok(Config { general, turn })
    }
}