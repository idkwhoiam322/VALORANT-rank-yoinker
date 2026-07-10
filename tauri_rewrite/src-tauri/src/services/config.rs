use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableConfig {
    #[serde(default = "default_true")]
    pub skin: bool,
    #[serde(default = "default_true")]
    pub rr: bool,
    #[serde(default = "default_false")]
    pub earned_rr: bool,
    #[serde(default = "default_true")]
    pub peakrank: bool,
    #[serde(default = "default_false")]
    pub previousrank: bool,
    #[serde(default = "default_true")]
    pub leaderboard: bool,
    #[serde(default = "default_false")]
    pub headshot_percent: bool,
    #[serde(default = "default_true")]
    pub winrate: bool,
    #[serde(default = "default_false")]
    pub kd: bool,
    #[serde(default = "default_true")]
    pub level: bool,
    #[serde(default = "default_true")]
    pub last_active: bool,
}

impl Default for TableConfig {
    fn default() -> Self {
        Self {
            skin: true,
            rr: true,
            earned_rr: false,
            peakrank: true,
            previousrank: false,
            leaderboard: true,
            headshot_percent: false,
            winrate: true,
            kd: false,
            level: true,
            last_active: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagsConfig {
    #[serde(default = "default_true")]
    pub last_played: bool,
    #[serde(default = "default_true")]
    pub auto_hide_leaderboard: bool,
    #[serde(default = "default_false")]
    pub pre_cls: bool,
    #[serde(default = "default_false")]
    pub game_chat: bool,
    #[serde(default = "default_true")]
    pub peak_rank_act: bool,
    #[serde(default = "default_false")]
    pub discord_rpc: bool,
    #[serde(default = "default_true")]
    pub aggregate_rank_rr: bool,
    #[serde(default = "default_false")]
    pub server_id: bool,
    #[serde(default = "default_false")]
    pub short_ranks: bool,
    #[serde(default = "default_true")]
    pub truncate_skins: bool,
    #[serde(default = "default_false")]
    pub truncate_names: bool,
    #[serde(default = "default_true")]
    pub starting_side: bool,
}

impl Default for FlagsConfig {
    fn default() -> Self {
        Self {
            last_played: true,
            auto_hide_leaderboard: true,
            pre_cls: false,
            game_chat: false,
            peak_rank_act: true,
            discord_rpc: false,
            aggregate_rank_rr: true,
            server_id: false,
            short_ranks: false,
            truncate_skins: true,
            truncate_names: false,
            starting_side: true,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_cooldown")]
    pub cooldown: u64,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_weapon")]
    pub weapon: String,
    #[serde(default = "default_chat_limit")]
    pub chat_limit: u32,
    #[serde(default)]
    pub table: TableConfig,
    #[serde(default)]
    pub flags: FlagsConfig,
}

fn default_cooldown() -> u64 {
    10
}
fn default_port() -> u16 {
    1100
}
fn default_weapon() -> String {
    "Vandal".into()
}
fn default_chat_limit() -> u32 {
    5
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            cooldown: 10,
            port: 1100,
            weapon: "Vandal".into(),
            chat_limit: 5,
            table: TableConfig::default(),
            flags: FlagsConfig::default(),
        }
    }
}

pub struct ConfigManager {
    path: PathBuf,
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

        Self { path, config }
    }

    pub fn get(&self) -> &AppConfig {
        &self.config
    }

    pub fn set(&mut self, config: AppConfig) {
        self.config = config;
        self.save();
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.config) {
            let _ = fs::write(&self.path, &json);
        }
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
        "snowball" => "Snowball Fight",
        "valaram" => "All Random One Site",
        "dodgeball" => "Knockout",
        "custom" => "Custom",
        _ => "Custom",
    }
}
