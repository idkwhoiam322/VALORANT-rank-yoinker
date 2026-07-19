/// All API endpoint paths and URL templates in one place.

// -- Base URL templates --
pub const VALORANT_API_BASE: &str = "https://valorant-api.com/v1";

// -- Local endpoints (https://127.0.0.1:{port}) --
pub const LOCAL_ENTITLEMENTS: &str = "/entitlements/v1/token";
pub const LOCAL_PRESENCES: &str = "/chat/v4/presences";
pub const LOCAL_NAME_LOOKUP: &str = "/player-account/lookup/v2/namesets-for-puuids";

// -- Pd endpoints (https://pd.{region}.a.pvp.net) --
pub fn pd_mmr_player(puuid: &str) -> String {
    format!("/mmr/v1/players/{puuid}")
}
pub fn pd_competitive_updates(puuid: &str) -> String {
    format!("/mmr/v1/players/{puuid}/competitiveupdates?startIndex=0&endIndex=1&queue=competitive")
}
pub fn pd_match_details(match_id: &str) -> String {
    format!("/match-details/v1/matches/{match_id}")
}
pub const PD_NAME_SERVICE: &str = "/name-service/v2/players";

// -- Glz endpoints (https://glz-{host}.{shard}.a.pvp.net) --
pub fn glz_core_player(puuid: &str) -> String {
    format!("/core-game/v1/players/{puuid}")
}
pub fn glz_core_match(match_id: &str) -> String {
    format!("/core-game/v1/matches/{match_id}")
}
pub fn glz_core_loadouts(match_id: &str) -> String {
    format!("/core-game/v1/matches/{match_id}/loadouts")
}
pub fn glz_pregame_player(puuid: &str) -> String {
    format!("/pregame/v1/players/{puuid}")
}
pub fn glz_pregame_match(match_id: &str) -> String {
    format!("/pregame/v1/matches/{match_id}")
}
pub fn glz_pregame_loadouts(match_id: &str) -> String {
    format!("/pregame/v1/matches/{match_id}/loadouts")
}

// -- Valorant API endpoints (https://valorant-api.com/v1/) --
pub const VAL_AGENTS: &str = "agents?isPlayableCharacter=true";
pub const VAL_MAPS: &str = "maps";
pub const VAL_WEAPONS: &str = "weapons";
pub const VAL_SPRAYS: &str = "sprays";
pub const VAL_FLEX: &str = "flex";
pub const VAL_BUDDIES: &str = "buddies";
pub const VAL_PLAYER_TITLES: &str = "playertitles";
pub const VAL_PLAYER_CARDS: &str = "playercards";
pub const VAL_COMPETITIVE_TIERS: &str = "competitivetiers";
pub const VAL_CONTENT_TIERS: &str = "contenttiers";

// -- Image URL templates --
pub fn media_agent_icon(agent_uuid: &str) -> String {
    format!("https://media.valorant-api.com/agents/{agent_uuid}/displayicon.png")
}

pub fn media_weapon_icon(weapon_uuid: &str) -> String {
    format!("https://media.valorant-api.com/weapons/{weapon_uuid}/displayicon.png")
}

