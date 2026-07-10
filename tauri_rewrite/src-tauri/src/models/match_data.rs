use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CoregamePlayer {
    #[serde(default)]
    pub subject: Option<String>,

    #[serde(default, alias = "TeamID")]
    pub team_id: Option<String>,

    #[serde(default, alias = "CharacterID")]
    pub character_id: Option<String>,

    #[serde(default)]
    pub player_identity: Option<PlayerIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayerIdentity {
    #[serde(default)]
    pub account_level: Option<u32>,

    #[serde(default)]
    pub incognito: Option<bool>,

    #[serde(default)]
    pub hide_account_level: Option<bool>,

    #[serde(default, alias = "PlayerTitleID")]
    pub player_title_id: Option<String>,

    #[serde(default, alias = "PlayerCardID")]
    pub player_card_id: Option<String>,
}
