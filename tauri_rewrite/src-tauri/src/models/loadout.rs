use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CoregameLoadoutsResponse {
    #[serde(default)]
    pub loadouts: Vec<CoregameLoadoutEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CoregameLoadoutEntry {
    #[serde(default)]
    pub subject: Option<String>,

    #[serde(default, alias = "CharacterID")]
    pub character_id: Option<String>,

    /// Nested loadout (used by in-game/coregame endpoint)
    #[serde(default)]
    pub loadout: Option<Loadout>,

    /// Flat fields (used by pregame endpoint - Items/Expressions at top level)
    #[serde(default)]
    pub items: Option<HashMap<String, WeaponSlot>>,

    #[serde(default)]
    pub expressions: Option<ExpressionSelections>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Loadout {
    #[serde(default)]
    pub items: Option<HashMap<String, WeaponSlot>>,

    #[serde(default)]
    pub expressions: Option<ExpressionSelections>,

    #[serde(default, alias = "CharacterID")]
    pub character_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct WeaponSlot {
    #[serde(default)]
    pub sockets: Option<HashMap<String, Socket>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Socket {
    #[serde(default)]
    pub item: Option<SocketItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SocketItem {
    #[serde(default, alias = "ID")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ExpressionSelections {
    #[serde(default, alias = "AESSelections")]
    pub aes_selections: Vec<Expression>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Expression {
    #[serde(default, alias = "AssetID")]
    pub asset_id: Option<String>,

    #[serde(default, alias = "SlotID")]
    pub slot_id: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadoutJson {
    #[serde(flatten)]
    pub players: HashMap<String, PlayerLoadoutData>,

    #[serde(default)]
    pub map: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerLoadoutData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_card: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_card_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprays: Option<HashMap<String, SprayEntry>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapons: Option<HashMap<String, WeaponEntry>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SprayEntry {
    #[serde(rename = "type")]
    pub spray_type: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "displayName")]
    pub display_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "displayIcon")]
    pub display_icon: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "fullTransparentIcon")]
    pub full_transparent_icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin_level: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin_chroma: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "skinDisplayName")]
    pub skin_display_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "skinDisplayIcon")]
    pub skin_display_icon: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "chromaDisplayName")]
    pub chroma_display_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "buddy_displayIcon")]
    pub buddy_display_icon: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "buddy_displayName")]
    pub buddy_display_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "weaponDisplayIcon")]
    pub weapon_display_icon: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "contentTierName")]
    pub skin_content_tier_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "contentTierColor")]
    pub skin_content_tier_color: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "contentTierIcon")]
    pub skin_content_tier_icon: Option<String>,
}


