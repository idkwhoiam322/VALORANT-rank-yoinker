use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MmrResponse {
    #[serde(default)]
    pub queue_skills: Option<QueueSkills>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueueSkills {
    #[serde(default, alias = "competitive")]
    pub competitive: Option<CompetitiveSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CompetitiveSkill {
    #[serde(default, alias = "SeasonalInfoBySeasonID")]
    pub seasonal_info_by_season_id: Option<HashMap<String, SeasonalInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SeasonalInfo {
    #[serde(default)]
    pub competitive_tier: Option<u32>,

    #[serde(default)]
    pub ranked_rating: Option<i32>,

    #[serde(default)]
    pub leaderboard_rank: Option<i32>,

    #[serde(default)]
    pub wins_by_tier: Option<HashMap<String, u32>>,

    #[serde(default)]
    pub number_of_wins_with_placements: Option<u32>,

    #[serde(default)]
    pub number_of_games: Option<u32>,

    #[serde(default)]
    pub number_of_wins: Option<u32>,

    #[serde(default)]
    pub peak_rank: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRank {
    pub rank: u32,
    pub rr: i32,
    pub leaderboard: i32,
    pub peak_rank: u32,
    pub peak_rank_act: Option<String>,
    pub peak_rank_ep: Option<String>,
    pub previous_rank: u32,
    pub wr: String,
    pub number_of_games: u32,
    pub status_good: bool,
}

impl PlayerRank {
    pub fn empty() -> Self {
        Self {
            rank: 0,
            rr: 0,
            leaderboard: 0,
            peak_rank: 0,
            peak_rank_act: None,
            peak_rank_ep: None,
            previous_rank: 0,
            wr: "N/A".into(),
            number_of_games: 0,
            status_good: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CompetitiveUpdatesResponse {
    #[serde(default)]
    pub matches: Vec<CompetitiveUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CompetitiveUpdate {
    #[serde(default, alias = "MatchID")]
    pub match_id: Option<String>,

    #[serde(default)]
    pub ranked_rating_earned: Option<i32>,

    #[serde(default, alias = "AFKPenalty")]
    pub afk_penalty: Option<i32>,

    #[serde(default)]
    pub match_start_time: Option<i64>,

    #[serde(default)]
    pub tier_after_update: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDetailsResponse {
    #[serde(default, rename = "matchInfo")]
    pub match_info: Option<MatchInfo>,

    #[serde(default)]
    pub players: Vec<MatchPlayer>,

    #[serde(default, rename = "roundResults")]
    pub round_results: Vec<RoundResult>,

    #[serde(default)]
    pub teams: Vec<MatchTeam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInfo {
    #[serde(default, rename = "gameStartMillis")]
    pub game_start_millis: Option<i64>,

    #[serde(default, rename = "gameLengthMillis")]
    pub game_length_millis: Option<i64>,

    #[serde(default, alias = "winningTeam", alias = "WinningTeam", alias = "winningTeamId", alias = "WinningTeamID")]
    pub winning_team: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchPlayer {
    #[serde(default)]
    pub subject: Option<String>,

    #[serde(default)]
    pub stats: Option<PlayerMatchStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMatchStats {
    #[serde(default)]
    pub kills: Option<u32>,

    #[serde(default)]
    pub deaths: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundResult {
    #[serde(default, rename = "playerStats")]
    pub player_stats: Vec<RoundPlayerStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundPlayerStats {
    #[serde(default)]
    pub subject: Option<String>,

    #[serde(default)]
    pub damage: Vec<DamageEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamageEntry {
    #[serde(default)]
    pub legshots: Option<u32>,

    #[serde(default)]
    pub bodyshots: Option<u32>,

    #[serde(default)]
    pub headshots: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchTeam {
    #[serde(default, alias = "teamId", alias = "teamID", alias = "TeamID", alias = "team_id")]
    pub team_id: Option<String>,

    #[serde(default, alias = "roundsWon", alias = "RoundsWon")]
    pub rounds_won: Option<i32>,

    #[serde(default)]
    pub won: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStats {
    pub kd: String,
    pub hs: String,
    pub ranked_rating_earned: String,
    pub afk_penalty: String,
    pub last_active_epoch: Option<i64>,
}

impl PlayerStats {
    pub fn default_stats() -> Self {
        Self {
            kd: "N/A".into(),
            hs: "N/A".into(),
            ranked_rating_earned: "N/A".into(),
            afk_penalty: "N/A".into(),
            last_active_epoch: None,
        }
    }
}
