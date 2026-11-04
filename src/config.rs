use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use tracing::{info, warn};

const CONFIG_FILE_PATH: &str = "nsound_config.json";

/// Configuration for an individual communicator (e.g. Discord, Teams, Skype).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicatorConfig {
    pub id: String,
    pub name: String,
    pub process_names: Vec<String>,
    pub enabled: bool,
    /// Short text drawn in the UI badge (falls back to the first letter of `name`).
    #[serde(default)]
    pub short_label: String,
    /// RGB colour of the UI badge.
    #[serde(default = "default_brand_color")]
    pub brand_color: [u8; 3],
}

fn default_brand_color() -> [u8; 3] {
    [110, 110, 120]
}

impl CommunicatorConfig {
    pub fn discord() -> Self {
        Self {
            id: "discord".to_string(),
            name: "Discord".to_string(),
            process_names: vec![
                "Discord.exe".to_string(),
                "discord".to_string(),
                "DiscordCanary.exe".to_string(),
                "DiscordPTB.exe".to_string(),
            ],
            enabled: true,
            short_label: "D".to_string(),
            brand_color: [0x58, 0x65, 0xF2],
        }
    }

    pub fn teams() -> Self {
        Self {
            id: "teams".to_string(),
            name: "Microsoft Teams".to_string(),
            process_names: vec!["ms-teams.exe".to_string(), "Teams.exe".to_string()],
            enabled: false,
            short_label: "T".to_string(),
            brand_color: [0x62, 0x64, 0xA7],
        }
    }
}

/// Persistent application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Master toggle switch (ON/OFF).
    pub master_enabled: bool,
    /// RMS sensitivity threshold (0.001 to 0.5).
    pub rms_threshold: f32,
    /// Time (in milliseconds) the level must stay above the threshold before
    /// the pause is triggered; filters out short blips and noise spikes.
    #[serde(default = "default_attack_ms")]
    pub attack_ms: u64,
    /// Time (in milliseconds) to hold pause after voice activity stops before resuming media.
    pub hold_time_ms: u64,
    /// Whether to automatically send Play/Resume media command when speaking stops.
    pub auto_resume: bool,
    /// List of supported communicators.
    pub communicators: Vec<CommunicatorConfig>,
}

fn default_attack_ms() -> u64 {
    250
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            master_enabled: true,
            rms_threshold: 0.06, // approx -24 dB RMS
            attack_ms: default_attack_ms(),
            hold_time_ms: 1200,
            auto_resume: true,
            communicators: vec![
                CommunicatorConfig::discord(),
                CommunicatorConfig::teams(),
            ],
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        if Path::new(CONFIG_FILE_PATH).exists() {
            match File::open(CONFIG_FILE_PATH) {
                Ok(mut file) => {
                    let mut content = String::new();
                    if file.read_to_string(&mut content).is_ok() {
                        if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
                            info!("Loaded configuration from {}", CONFIG_FILE_PATH);
                            return config;
                        }
                    }
                }
                Err(err) => {
                    warn!("Failed to read config file: {}", err);
                }
            }
        }
        let default_config = Self::default();
        let _ = default_config.save();
        default_config
    }

    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        let mut file = File::create(CONFIG_FILE_PATH)
            .map_err(|e| format!("Failed to create config file: {}", e))?;
        file.write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write config file: {}", e))?;
        info!("Saved configuration to {}", CONFIG_FILE_PATH);
        Ok(())
    }
}

/// Real-time runtime status of the application.
#[derive(Debug, Clone)]
pub struct AppStatus {
    pub is_monitoring: bool,
    pub is_speaking: bool,
    pub current_rms: f32,
    pub current_db: f32,
    pub status_message: String,
    pub communicator_running: HashMap<String, bool>,
    pub media_paused: bool,
}

impl Default for AppStatus {
    fn default() -> Self {
        let mut running_map = HashMap::new();
        running_map.insert("discord".to_string(), false);
        running_map.insert("teams".to_string(), false);

        Self {
            is_monitoring: false,
            is_speaking: false,
            current_rms: 0.0,
            current_db: -100.0,
            status_message: "Initializing...".to_string(),
            communicator_running: running_map,
            media_paused: false,
        }
    }
}
