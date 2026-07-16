use std::sync::Arc;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
use crate::models::content::*;

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
    client.fetch_valorant_api(endpoints::VAL_AGENTS).await
}

fn populate_agents(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Agent>>) {
    for agent in &resp.data {
        cache.agents.insert(agent.uuid.to_lowercase(), agent.display_name.clone());
    }
}

async fn fetch_maps_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<Map>>, ApiError> {
    client.fetch_valorant_api(endpoints::VAL_MAPS).await
}

fn populate_maps(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Map>>) {
    for map in &resp.data {
        if let Some(ref url) = map.map_url {
            cache.maps.insert(url.to_lowercase(), map.display_name.clone());
        }
    }
}

async fn fetch_weapons_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<WeaponData>>, ApiError> {
    client.fetch_valorant_api(endpoints::VAL_WEAPONS).await
}

async fn fetch_weapons_with_retry_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<WeaponData>>, ApiError> {
    let delays = [1, 2, 4];
    let mut last_err = None;
    for (i, delay) in delays.iter().enumerate() {
        match fetch_weapons_raw(client).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                log::warn!("weapons fetch attempt {} failed: {}", i + 1, e);
                last_err = Some(e);
                tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
            }
        }
    }
    Err(last_err.unwrap())
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
}

async fn fetch_sprays_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<Spray>>, ApiError> {
    client.fetch_valorant_api(endpoints::VAL_SPRAYS).await
}

fn populate_sprays(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Spray>>) {
    for spray in &resp.data {
        cache
            .sprays
            .insert(spray.uuid.to_lowercase(), spray.clone());
    }
}

async fn fetch_flex_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<Flex>>, ApiError> {
    client.fetch_valorant_api(endpoints::VAL_FLEX).await
}

fn populate_flex(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Flex>>) {
    for flex in &resp.data {
        cache.flex.insert(flex.uuid.to_lowercase(), flex.clone());
    }
}

async fn fetch_buddies_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<Buddy>>, ApiError> {
    client.fetch_valorant_api(endpoints::VAL_BUDDIES).await
}

fn populate_buddies(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<Buddy>>) {
    for buddy in &resp.data {
        cache.buddies.insert(buddy.uuid.to_lowercase(), buddy.clone());
    }
}

async fn fetch_player_titles_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<PlayerTitle>>, ApiError> {
    client.fetch_valorant_api(endpoints::VAL_PLAYER_TITLES).await
}

fn populate_player_titles(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<PlayerTitle>>) {
    for title in &resp.data {
        cache
            .player_titles
            .insert(title.uuid.to_lowercase(), title.clone());
    }
}

async fn fetch_player_cards_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<PlayerCard>>, ApiError> {
    client.fetch_valorant_api(endpoints::VAL_PLAYER_CARDS).await
}

fn populate_player_cards(cache: &mut ContentCache, resp: ValorantApiResponse<Vec<PlayerCard>>) {
    for card in &resp.data {
        cache
            .player_cards
            .insert(card.uuid.to_lowercase(), card.clone());
    }
}

async fn fetch_competitive_tiers_raw(client: &ApiClient) -> Result<ValorantApiResponse<Vec<CompetitiveTiers>>, ApiError> {
    client.fetch_valorant_api(endpoints::VAL_COMPETITIVE_TIERS).await
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
        fetch_weapons_with_retry_raw(client),
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

    macro_rules! handle {
        ($result:expr, $populate:expr, $label:expr) => {
            match $result {
                Ok(resp) => $populate(&mut cache, resp),
                Err(e) => {
                    log::warn!("Content fetch error ({}): {}", $label, e);
                    had_error = true;
                }
            }
        };
    }

    handle!(agents, populate_agents, "agents");
    handle!(maps, populate_maps, "maps");
    handle!(weapons, populate_weapons, "weapons");
    handle!(sprays, populate_sprays, "sprays");
    handle!(flex, populate_flex, "flex");
    handle!(buddies, populate_buddies, "buddies");
    handle!(titles, populate_player_titles, "player_titles");
    handle!(cards, populate_player_cards, "player_cards");
    handle!(tiers, populate_competitive_tiers, "competitive_tiers");

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
