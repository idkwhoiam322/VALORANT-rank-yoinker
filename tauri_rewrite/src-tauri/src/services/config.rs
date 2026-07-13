use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_cooldown")]
    pub cooldown: u64,
}

fn default_cooldown() -> u64 {
    10
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            cooldown: 10,
        }
    }
}

pub struct ConfigManager {
    config: AppConfig,
}

impl ConfigManager {
    pub fn new(root: PathBuf) -> Self {
        let path = root.join("config.json");
        let config = if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            let cfg = AppConfig::default();
            if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                let _ = fs::write(&path, &json);
            }
            cfg
        };

        Self { config }
    }

    pub fn get(&self) -> &AppConfig {
        &self.config
    }
}

pub fn get_gamemode_name(queue_id: &str) -> &'static str {
    match queue_id {
        "competitive" => "Competitive",
        "unrated" => "Unrated",
        "swiftplay" => "Swiftplay",
        "spikerush" => "Spike Rush",
        "deathmatch" => "Deathmatch",
        "ggteam" => "Escalation",
        "onefa" => "Replication",
        "hurm" => "Team Deathmatch",
        "newmap" => "New Map",
        "fortcollins" => "Retake",
        "snowball" => "Snowball Fight",
        "valaram" => "All Random One Site",
        "dodgeball" => "Knockout",
        "custom" => "Custom",
        _ => "Custom",
    }
}
