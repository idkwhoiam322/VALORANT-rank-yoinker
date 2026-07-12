use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presence {
    #[serde(default)]
    pub puuid: Option<String>,
    #[serde(default)]
    pub private: Option<String>,
    #[serde(default)]
    pub product: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresencesResponse {
    #[serde(default)]
    pub presences: Vec<Presence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameState {
    MENUS,
    PREGAME,
    INGAME,
    DISCONNECTED,
}

impl GameState {
    pub fn from_str(s: &str) -> Self {
        match s {
            "MENUS" => GameState::MENUS,
            "PREGAME" => GameState::PREGAME,
            "INGAME" => GameState::INGAME,
            _ => GameState::DISCONNECTED,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            GameState::MENUS => "MENUS",
            GameState::PREGAME => "PREGAME",
            GameState::INGAME => "INGAME",
            GameState::DISCONNECTED => "DISCONNECTED",
        }
    }
}
