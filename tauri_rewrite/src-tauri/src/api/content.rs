use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
use crate::models::content::*;

/// Generic retry wrapper for valorant-api.com endpoints.
/// Retries up to 3 times (1s, 2s, 4s delays) on transient errors or
/// when the API returns HTTP 200 with an empty `data` array.
async fn fetch_valorant_api_with_retry<T: DeserializeOwned>(
    client: &ApiClient,
    endpoint: &str,
) -> Result<ValorantApiResponse<Vec<T>>, ApiError> {
    let delays = [1, 2, 4];
    let mut last_err = None;
    for (i, delay) in delays.iter().enumerate() {
        match client
            .fetch_valorant_api::<ValorantApiResponse<Vec<T>>>(endpoint)
            .await
        {
            Ok(v) => {
                if v.data.is_empty() {
                    log::warn!(
                        "[CONTENT] {} attempt {} returned empty data, retrying in {}s...",
                        endpoint,
                        i + 1,
                        delay,
                    );
                    last_err = Some(ApiError::ServerError(format!(
                        "empty {} data after attempt {}",
                        endpoint,
                        i + 1
                    )));
                    tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
                    continue;
                }
                return Ok(v);
            }
            Err(e) => {
                log::warn!("[CONTENT] {} attempt {} failed: {}", endpoint, i + 1, e);
                last_err = Some(e);
                tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
            }
        }
    }
    Err(last_err.unwrap_or(ApiError::ServerError(format!(
        "{} fetch exhausted after {} attempts",
        endpoint,
        delays.len()
    ))))
}

const BEFORE_ASCENDANT_SEASONS: &[&str] = &[
    "0df5adb9-4dcb-6899-1306-3e9860661dd3",
    "3f61c772-4560-cd3f-5d3f-a7ab5abda6b3",
    "0530b9c4-4980-f2ee-df5d-09864cd00542",
    "46ea6166-4573-1128-9cea-60a15640059b",
    "fcf2c8f4-4324-e50b-2e23-718e4a3ab046",
    "97b6e739-44cc-ffa7-49ad-398ba502ceb0",
    "ab57ef51-4e59-da91-cc8d-51a5a2b9b8ff",
    "52e9749a-429b-7060-99fe-4595426a0cf7",
    "71c81c67-4fae-ceb1-844c-aab2bb8710fa",
    "2a27e5d2-4d30-c9e2-b15a-93b8909a442c",
    "4cb622e1-4244-6da3-7276-8daaf1c01be2",
    "a16955a5-4ad0-f761-5e9e-389df1c892fb",
    "97b39124-46ce-8b55-8fd1-7cbf7ffe173f",
    "573f53ac-41a5-3a7d-d9ce-d6a6298e5704",
    "d929bc38-4ab6-7da4-94f0-ee84f8ac141e",
    "3e47230a-463c-a301-eb7d-67bb60357d4f",
    "808202d6-4f2b-a8ff-1feb-b3a0590ad79f",
];

async fn fetch_agents_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<Agent>>, ApiError> {
    fetch_valorant_api_with_retry(client, endpoints::VAL_AGENTS).await
}

fn populate_agents(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Agent>>) {
    for agent in &resp.data {
        cache.agents.insert(agent.uuid.to_lowercase(), agent.display_name.clone());
    }
    if cache.agents.is_empty() {
        log::warn!("[CONTENT] populate_agents: 0 agents inserted (empty response data)");
    }
}

async fn fetch_maps_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<Map>>, ApiError> {
    fetch_valorant_api_with_retry(client, endpoints::VAL_MAPS).await
}

fn populate_maps(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Map>>) {
    for map in &resp.data {
        if let Some(ref url) = map.map_url {
            // Key by the full valorant-api map_url (existing behavior).
            cache.maps.insert(url.to_lowercase(), map.display_name.clone());
            // Also key by the bare codename (last path segment, lower-cased) so
            // Riot's live MapID values resolve directly via ContentCache::get_map_name
            // without needing the full asset path. e.g. valorant-api url
            // "/Game/Maps/Summit/Summit" also becomes key "summit".
            if let Some(codename) = url.rsplit('/').find(|s| !s.is_empty()) {
                let key = codename.to_lowercase();
                cache.maps.entry(key).or_insert_with(|| map.display_name.clone());
            }
        }
    }
    if cache.maps.is_empty() {
        log::warn!("[CONTENT] populate_maps: 0 maps inserted (empty response data)");
    }
}

async fn fetch_weapons_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<WeaponData>>, ApiError> {
    fetch_valorant_api_with_retry(client, endpoints::VAL_WEAPONS).await
}

fn populate_weapons(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<WeaponData>>) {
    for weapon in &resp.data {
        cache
            .weapons
            .insert(weapon.uuid.to_lowercase(), weapon.clone());
        for skin in &weapon.skins {
            cache
                .skins_by_uuid
                .insert(skin.uuid.to_lowercase(), skin.clone());
        }
    }
    if cache.weapons.is_empty() {
        log::warn!("[CONTENT] populate_weapons: 0 weapons inserted (empty response data)");
    }
}

async fn fetch_sprays_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<Spray>>, ApiError> {
    fetch_valorant_api_with_retry(client, endpoints::VAL_SPRAYS).await
}

fn populate_sprays(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Spray>>) {
    for spray in &resp.data {
        cache
            .sprays
            .insert(spray.uuid.to_lowercase(), spray.clone());
    }
    if cache.sprays.is_empty() {
        log::warn!("[CONTENT] populate_sprays: 0 sprays inserted (empty response data)");
    }
}

async fn fetch_flex_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<Flex>>, ApiError> {
    fetch_valorant_api_with_retry(client, endpoints::VAL_FLEX).await
}

fn populate_flex(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Flex>>) {
    for flex in &resp.data {
        cache.flex.insert(flex.uuid.to_lowercase(), flex.clone());
    }
    if cache.flex.is_empty() {
        log::warn!("[CONTENT] populate_flex: 0 flex items inserted (empty response data)");
    }
}

async fn fetch_buddies_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<Buddy>>, ApiError> {
    fetch_valorant_api_with_retry(client, endpoints::VAL_BUDDIES).await
}

fn populate_buddies(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Buddy>>) {
    for buddy in &resp.data {
        cache.buddies.insert(buddy.uuid.to_lowercase(), buddy.clone());
    }
    if cache.buddies.is_empty() {
        log::warn!("[CONTENT] populate_buddies: 0 buddies inserted (empty response data)");
    }
}

async fn fetch_player_titles_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<PlayerTitle>>, ApiError> {
    fetch_valorant_api_with_retry(client, endpoints::VAL_PLAYER_TITLES).await
}

fn populate_player_titles(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<PlayerTitle>>) {
    for title in &resp.data {
        cache
            .player_titles
            .insert(title.uuid.to_lowercase(), title.clone());
    }
    if cache.player_titles.is_empty() {
        log::warn!("[CONTENT] populate_player_titles: 0 titles inserted (empty response data)");
    }
}

async fn fetch_player_cards_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<PlayerCard>>, ApiError> {
    fetch_valorant_api_with_retry(client, endpoints::VAL_PLAYER_CARDS).await
}

fn populate_player_cards(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<PlayerCard>>) {
    for card in &resp.data {
        cache
            .player_cards
            .insert(card.uuid.to_lowercase(), card.clone());
    }
    if cache.player_cards.is_empty() {
        log::warn!("[CONTENT] populate_player_cards: 0 cards inserted (empty response data)");
    }
}

async fn fetch_competitive_tiers_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<CompetitiveTiers>>, ApiError> {
    fetch_valorant_api_with_retry(client, endpoints::VAL_COMPETITIVE_TIERS).await
}

fn populate_competitive_tiers(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<CompetitiveTiers>>) {
    if let Some(latest) = resp.data.last() {
        let mut icons = Vec::new();
        for tier in &latest.tiers {
            if tier.tier as usize >= icons.len() {
                icons.resize(tier.tier as usize + 1, None);
            }
            icons[tier.tier as usize] = tier.small_icon.clone();
        }
        cache.rank_icons = Arc::new(icons);
    }
    if resp.data.is_empty() {
        log::warn!("[CONTENT] populate_competitive_tiers: 0 tier sets inserted (empty response data)");
    }
}

async fn fetch_seasons_raw(
    client: &ApiClient,
    region_shard: &str,
    entitlements: &crate::models::auth::Entitlements,
    client_version: &str,
) -> Result<serde_json::Value, ApiError> {
    let headers = entitlements.build_headers(client_version);
    client
        .fetch_json(
            UrlType::Custom,
            &format!(
                "https://shared.{region_shard}.a.pvp.net/content-service/v3/content",
            ),
            &headers,
        )
        .await
}

fn process_seasons(content: serde_json::Value, cache: &mut ContentCache) -> (String, Option<String>) {
    let mut current_season_id = String::new();
    let mut previous_season_id: Option<String> = None;

    if let Some(seasons) = content["Seasons"].as_array() {
        cache.seasons = seasons.iter()
            .filter_map(|s| {
                serde_json::from_value::<crate::models::content::Season>(s.clone()).ok()
            })
            .collect();

        let mut current_start_time = String::new();
        for season in seasons {
            if season["Type"] != "act" { continue; }
            if season["IsActive"] == true {
                current_season_id = season["ID"].as_str().unwrap_or("").to_string();
                current_start_time = season["StartTime"].as_str().unwrap_or("").to_string();
            }
        }
        if !current_start_time.is_empty() {
            for season in seasons {
                if season["Type"] == "act"
                    && season["EndTime"].as_str().unwrap_or("") == current_start_time
                {
                    previous_season_id = season["ID"].as_str().map(|s| s.to_string());
                    break;
                }
            }
        }
    }

    (current_season_id, previous_season_id)
}

pub async fn fetch_all_content(
    client: &ApiClient,
    region_shard: &str,
    entitlements: &crate::models::auth::Entitlements,
    client_version: &str,
) -> (ContentCache, String, Option<String>) {
    // Run all valorant-api.com fetches concurrently (non-Riot, no rate-limit concern)
    let (agents, maps, weapons, sprays, flex, buddies, titles, cards, tiers, seasons) = tokio::join!(
        fetch_agents_raw(client),
        fetch_maps_raw(client),
        fetch_weapons_raw(client),
        fetch_sprays_raw(client),
        fetch_flex_raw(client),
        fetch_buddies_raw(client),
        fetch_player_titles_raw(client),
        fetch_player_cards_raw(client),
        fetch_competitive_tiers_raw(client),
        fetch_seasons_raw(client, region_shard, entitlements, client_version),
    );

    let mut cache = ContentCache::empty();
    let mut had_error = false;

    /// Generic handler for the 8 structurally-identical
    /// `fetch_x_raw` / `populate_x` content pairs. Replaces the previous
    /// `handle!` macro so the error-bookkeeping + populate path lives in one
    /// place.
    ///
    /// Note: `populate_competitive_tiers` was originally intended to stay
    /// bespoke, but its outer signature (`fn(&mut ContentCache,
    /// ValorantApiResponse<Vec<CompetitiveTiers>>)`) matches the generic shape,
    /// so it is routed through `apply_content` below like the rest. The
    /// `seasons` fetch remains bespoke (different response/deserialize shape).
    fn apply_content<T: DeserializeOwned>(
        result: Result<ValorantApiResponse<Vec<T>>, ApiError>,
        populate: fn(&mut ContentCache, ValorantApiResponse<Vec<T>>),
        label: &str,
        cache: &mut ContentCache,
        had_error: &mut bool,
    ) {
        match result {
            Ok(resp) => populate(cache, resp),
            Err(e) => {
                log::warn!("Content fetch error ({}): {}", label, e);
                *had_error = true;
            }
        }
    }

    apply_content(agents, populate_agents, "agents", &mut cache, &mut had_error);
    apply_content(maps, populate_maps, "maps", &mut cache, &mut had_error);
    apply_content(weapons, populate_weapons, "weapons", &mut cache, &mut had_error);
    apply_content(sprays, populate_sprays, "sprays", &mut cache, &mut had_error);
    apply_content(flex, populate_flex, "flex", &mut cache, &mut had_error);
    apply_content(buddies, populate_buddies, "buddies", &mut cache, &mut had_error);
    apply_content(titles, populate_player_titles, "player_titles", &mut cache, &mut had_error);
    apply_content(cards, populate_player_cards, "player_cards", &mut cache, &mut had_error);
    apply_content(tiers, populate_competitive_tiers, "competitive_tiers", &mut cache, &mut had_error);

    let (season_id, previous_season_id) = match seasons {
        Ok(content) => process_seasons(content, &mut cache),
        Err(e) => {
            log::warn!("Content fetch error (seasons): {}", e);
            had_error = true;
            (String::new(), None)
        }
    };

    if had_error {
        log::warn!("One or more content API calls failed - some data may be missing");
    }

    // Log content cache summary
    let mut agent_names: Vec<&str> = cache.agents.values().map(|s| s.as_str()).collect();
    agent_names.sort();
    client.app_log(&format!(
        "[CONTENT] Agents ({}): {}",
        agent_names.len(),
        agent_names.join(", "),
    ));

    let mut map_names: Vec<&str> = cache.maps.values().map(|s| s.as_str()).collect();
    map_names.sort();
    client.app_log(&format!(
        "[CONTENT] Maps ({}): {}",
        map_names.len(),
        map_names.join(", "),
    ));

    for weapon in cache.weapons.values() {
        let skin_names: Vec<&str> = weapon.skins.iter()
            .map(|s| s.display_name.as_str())
            .collect();
        client.app_log(&format!(
            "[CONTENT] Weapon {} ({} skins): {}",
            weapon.display_name,
            weapon.skins.len(),
            skin_names.join(", "),
        ));
    }

    client.app_log(&format!(
        "[CONTENT] Sprays ({}), Flex ({}), Buddies ({}), Player titles ({}), Player cards ({})",
        cache.sprays.len(), cache.flex.len(), cache.buddies.len(),
        cache.player_titles.len(), cache.player_cards.len(),
    ));

    client.app_log(&format!(
        "[CONTENT] Competitive tiers ({}), Seasons ({}), Season ID: {}",
        cache.rank_icons.len(), cache.seasons.len(), season_id,
    ));

    (cache, season_id, previous_season_id)
}

pub fn is_before_ascendant(season_id: &str) -> bool {
    BEFORE_ASCENDANT_SEASONS.contains(&season_id)
}
