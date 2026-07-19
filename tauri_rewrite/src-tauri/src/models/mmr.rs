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
    pub match_start_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamResult {
    #[serde(default, rename = "teamId", alias = "teamID", alias = "TeamID")]
    pub team_id: Option<String>,
    #[serde(default)]
    pub won: Option<bool>,
    #[serde(default, rename = "roundsWon", alias = "RoundsWon")]
    pub rounds_won: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDetailsResponse {
    #[serde(default, rename = "matchInfo")]
    pub match_info: Option<MatchInfo>,

    #[serde(default)]
    pub teams: Option<Vec<TeamResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInfo {
    #[serde(default, rename = "gameStartMillis")]
    pub game_start_millis: Option<i64>,

    #[serde(default, rename = "gameLengthMillis")]
    pub game_length_millis: Option<i64>,

    #[serde(default, rename = "winningTeam", alias = "WinningTeam")]
    pub winning_team: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStats {
    pub last_active_epoch: Option<i64>,
}

impl PlayerStats {
    pub fn default_stats() -> Self {
        Self {
            last_active_epoch: None,
        }
    }
}
