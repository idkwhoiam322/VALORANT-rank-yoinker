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
    Menus,
    Pregame,
    Ingame,
    Disconnected,
}

impl GameState {
    pub fn from_str(s: &str) -> Self {
        match s {
            "MENUS" => GameState::Menus,
            "PREGAME" => GameState::Pregame,
            "INGAME" => GameState::Ingame,
            _ => GameState::Disconnected,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            GameState::Menus => "MENUS",
            GameState::Pregame => "PREGAME",
            GameState::Ingame => "INGAME",
            GameState::Disconnected => "DISCONNECTED",
        }
    }
}
