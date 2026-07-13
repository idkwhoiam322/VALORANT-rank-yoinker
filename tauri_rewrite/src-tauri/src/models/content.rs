use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValorantApiResponse<T> {
    pub status: u32,
    pub data: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub uuid: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub display_icon: Option<String>,
    #[serde(default)]
    pub is_playable_character: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Map {
    pub uuid: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub map_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponData {
    pub uuid: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub display_icon: Option<String>,
    #[serde(default)]
    pub skins: Vec<Skin>,
    #[serde(default)]
    pub content_tier_uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    pub uuid: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub display_icon: Option<String>,
    #[serde(default)]
    pub chromas: Vec<Chroma>,
    #[serde(default)]
    pub levels: Vec<Level>,
    #[serde(default)]
    pub content_tier_uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chroma {
    pub uuid: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub display_icon: Option<String>,
    #[serde(default)]
    pub full_render: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub uuid: String,
    #[serde(default)]
    pub display_icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spray {
    pub uuid: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub display_icon: Option<String>,
    #[serde(default)]
    pub full_transparent_icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Buddy {
    pub uuid: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub display_icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerTitle {
    pub uuid: String,
    #[serde(default)]
    pub title_text: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerCard {
    pub uuid: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub large_art: Option<String>,
    #[serde(default)]
    pub wide_art: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompetitiveTiers {
    pub uuid: String,
    #[serde(default)]
    pub tiers: Vec<Tier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tier {
    pub tier: u32,
    #[serde(default)]
    pub tier_name: Option<String>,
    #[serde(default)]
    pub small_icon: Option<String>,
    #[serde(default)]
    pub large_icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flex {
    pub uuid: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub display_icon: Option<String>,
    #[serde(default)]
    pub full_transparent_icon: Option<String>,
}

/// A single entry from the Riot content-service `Seasons` array. Used to resolve a
/// season/act UUID to its human act/episode numbers (mirrors the fields the Python
/// original reads off of `content["Seasons"]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Season {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Type", default)]
    pub season_type: Option<String>,
    #[serde(rename = "StartTime", default)]
    pub start_time: String,
    #[serde(rename = "EndTime", default)]
    pub end_time: String,
    #[serde(rename = "IsActive", default)]
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct ContentCache {
    pub agents: HashMap<String, String>,
    pub maps: HashMap<String, String>,
    pub weapons: HashMap<String, WeaponData>,
    pub skins_by_uuid: HashMap<String, Skin>,
    pub sprays: HashMap<String, Spray>,
    pub flex: HashMap<String, Flex>,
    pub buddies: HashMap<String, Buddy>,
    pub player_titles: HashMap<String, PlayerTitle>,
    pub player_cards: HashMap<String, PlayerCard>,
    pub rank_icons: Arc<Vec<Option<String>>>,
    pub seasons: Vec<Season>,
}

impl ContentCache {
    pub fn empty() -> Self {
        Self {
            agents: HashMap::new(),
            maps: HashMap::new(),
            weapons: HashMap::new(),
            skins_by_uuid: HashMap::new(),
            sprays: HashMap::new(),
            flex: HashMap::new(),
            buddies: HashMap::new(),
            player_titles: HashMap::new(),
            player_cards: HashMap::new(),
            rank_icons: Arc::new(Vec::new()),
            seasons: Vec::new(),
        }
    }

    /// Resolve an act UUID to its (act, episode) display numbers. Faithful port of
    /// the Python `Content.get_act_episode_from_act_id` (name parsing incl. Roman
    /// numerals for legacy "ACT I"/"EPISODE II" seasons, and combined "E9A1"-style
    /// names for newer seasons).
    pub fn get_act_episode_from_act_id(&self, act_id: &str) -> (Option<String>, Option<String>) {
        let mut act: Option<String> = None;
        let mut episode: Option<String> = None;

        let mut trailing_episode: Option<&Season> = self.seasons.first();

        for season in &self.seasons {
            if season.season_type.as_deref() == Some("episode") {
                trailing_episode = Some(season);
            }
            if season.id.eq_ignore_ascii_case(act_id) {
                if let Some(num) = parse_season_number(&season.name) {
                    act = Some(num);
                }
                if let Some(ep) = trailing_episode.and_then(|s| parse_season_number(&s.name)) {
                    episode = Some(ep);
                }
                break;
            }
        }

        (act, episode)
    }
}

fn has_letter_and_number(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_alphabetic()) && text.chars().any(|c| c.is_ascii_digit())
}

fn roman_to_int(roman: &str) -> Option<i64> {
    let mut total: i64 = 0;
    let mut prev = 0i64;
    for c in roman.to_uppercase().chars().rev() {
        let value = match c {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            'C' => 100,
            _ => return None,
        };
        if value < prev {
            total -= value;
        } else {
            total += value;
        }
        prev = value;
    }
    Some(total)
}

/// Port of Python's `parse_season_number`: pulls the trailing token off a season
/// name (e.g. "EPISODE 9", "ACT III", "E9A1") and normalizes it to a display string.
fn parse_season_number(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    let number_part = name.split_whitespace().last()?;

    if has_letter_and_number(number_part) {
        return Some(number_part.to_lowercase());
    }

    let upper_name = name.to_uppercase();
    if upper_name.starts_with("EPISODE") {
        if let Ok(n) = number_part.parse::<i64>() {
            return Some(n.to_string());
        }
        return roman_to_int(number_part).map(|n| n.to_string());
    } else if upper_name.starts_with("ACT") {
        if let Some(n) = roman_to_int(number_part) {
            return Some(n.to_string());
        }
        if let Ok(n) = number_part.parse::<i64>() {
            return Some(n.to_string());
        }
    }

    None
}
