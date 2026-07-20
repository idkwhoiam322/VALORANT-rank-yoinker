use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub time: i64,
    pub state: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(default)]
    pub mode: Option<String>,
    pub puuid: String,
    #[serde(default)]
    pub map: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default, rename = "matchId")]
    pub match_id: Option<String>,
    pub players: HashMap<String, PlayerHeartbeat>,
    #[serde(default, rename = "rankIcons")]
    pub rank_icons: Arc<Vec<Option<String>>>,
    #[serde(default)]
    pub version: u64,
    #[serde(default, rename = "sessionId")]
    pub session_id: u64,
    #[serde(default, rename = "alreadyPlayedWith")]
    pub already_played_with: Vec<EncounterEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerHeartbeat {
    pub puuid: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "partyNumber")]
    pub party_number: u32,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default, rename = "agentSelectionState")]
    pub agent_selection_state: Option<String>,
    #[serde(default)]
    pub rank: u32,
    #[serde(default, rename = "peakRank")]
    pub peak_rank: u32,
    #[serde(default, rename = "peakRankAct")]
    pub peak_rank_act: Option<String>,
    #[serde(default, rename = "previousRank")]
    pub previous_rank: u32,
    #[serde(default)]
    pub rr: i32,
    #[serde(default, rename = "winPercentage")]
    pub win_percentage: Option<String>,
    #[serde(default, rename = "lastActive")]
    pub last_active: Option<String>,
    #[serde(default)]
    pub level: Option<u32>,
    #[serde(default)]
    pub leaderboard: i32,
    #[serde(default, rename = "agentImgLink")]
    pub agent_img_link: Option<String>,
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub sprays: Option<HashMap<String, super::loadout::SprayEntry>>,

    #[serde(default)]
    pub title: Option<String>,

    #[serde(default, rename = "playerCard")]
    pub player_card: Option<String>,

    #[serde(default, rename = "playerCardName")]
    pub player_card_name: Option<String>,

    #[serde(default)]
    pub weapons: Option<HashMap<String, super::loadout::WeaponEntry>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterEntry {
    pub times: usize,
    pub name: String,
    pub agent: String,
    pub map: String,
    #[serde(default, rename = "lastAgent")]
    pub last_agent: Option<String>,
    #[serde(default, rename = "lastMap")]
    pub last_map: Option<String>,
    pub relation: String,
    pub relation_name: String,
    pub time_diff: f64,
    pub ally_wins: usize,
    pub ally_losses: usize,
    pub ally_unknown: usize,
    pub ally_count: usize,
    pub enemy_wins: usize,
    pub enemy_losses: usize,
    pub enemy_unknown: usize,
    pub enemy_count: usize,
}
