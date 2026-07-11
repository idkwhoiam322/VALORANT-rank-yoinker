use crate::api::client::{ApiClient, ApiError, UrlType};
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

pub async fn fetch_all_content(
    client: &ApiClient,
    region_shard: &str,
    entitlements: &crate::models::auth::Entitlements,
    client_version: &str,
) -> (ContentCache, String, Option<String>) {
    let mut cache = ContentCache::empty();
    let mut had_error = false;

    if let Err(e) = fetch_agents(client, &mut cache).await {
        log::warn!("Content fetch error (agents): {}", e);
        had_error = true;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    if let Err(e) = fetch_maps(client, &mut cache).await {
        log::warn!("Content fetch error (maps): {}", e);
        had_error = true;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    if let Err(e) = fetch_weapons_with_retry(client, &mut cache).await {
        log::warn!("Content fetch error (weapons): {}", e);
        had_error = true;
    }
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    if let Err(e) = fetch_sprays(client, &mut cache).await {
        log::warn!("Content fetch error (sprays): {}", e);
        had_error = true;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    if let Err(e) = fetch_flex(client, &mut cache).await {
        log::warn!("Content fetch error (flex): {}", e);
        had_error = true;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    if let Err(e) = fetch_buddies(client, &mut cache).await {
        log::warn!("Content fetch error (buddies): {}", e);
        had_error = true;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    if let Err(e) = fetch_player_titles(client, &mut cache).await {
        log::warn!("Content fetch error (player_titles): {}", e);
        had_error = true;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    if let Err(e) = fetch_player_cards(client, &mut cache).await {
        log::warn!("Content fetch error (player_cards): {}", e);
        had_error = true;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    if let Err(e) = fetch_competitive_tiers(client, &mut cache).await {
        log::warn!("Content fetch error (competitive_tiers): {}", e);
        had_error = true;
    }
    // Riot API season fetch (different auth, can be faster)
    let (season_id, previous_season_id) =
        fetch_seasons(client, region_shard, &mut cache, entitlements, client_version).await.unwrap_or_default();

    if had_error {
        log::warn!("One or more content API calls failed — some data may be missing");
    }

    (cache, season_id, previous_season_id)
}

async fn fetch_agents(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let resp: ValorantApiResponse<Vec<Agent>> =
        client.fetch_valorant_api("agents?isPlayableCharacter=true").await?;
    for agent in &resp.data {
        cache.agents.insert(agent.uuid.to_lowercase(), agent.display_name.clone());
        cache.agent_uuids.insert(
            agent.display_name.to_lowercase(),
            agent.uuid.to_lowercase(),
        );
    }
    Ok(())
}

async fn fetch_maps(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let resp: ValorantApiResponse<Vec<Map>> = client.fetch_valorant_api("maps").await?;
    for map in &resp.data {
        if let Some(ref url) = map.map_url {
            cache.maps.insert(url.to_lowercase(), map.display_name.clone());
        }
        cache
            .map_splashes
            .insert(map.display_name.clone(), map.splash.clone().unwrap_or_default());
    }
    Ok(())
}

async fn fetch_weapons(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let resp: ValorantApiResponse<Vec<WeaponData>> =
        client.fetch_valorant_api("weapons").await?;
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
    Ok(())
}

async fn fetch_weapons_with_retry(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let delays = [1, 2, 4];
    let mut last_err = None;
    for (i, delay) in delays.iter().enumerate() {
        match fetch_weapons(client, cache).await {
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

async fn fetch_sprays(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let resp: ValorantApiResponse<Vec<Spray>> = client.fetch_valorant_api("sprays").await?;
    for spray in &resp.data {
        cache
            .sprays
            .insert(spray.uuid.to_lowercase(), spray.clone());
    }
    Ok(())
}

async fn fetch_flex(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let resp: ValorantApiResponse<Vec<Flex>> = client.fetch_valorant_api("flex").await?;
    for flex in &resp.data {
        cache.flex.insert(flex.uuid.to_lowercase(), flex.clone());
    }
    Ok(())
}

async fn fetch_buddies(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let resp: ValorantApiResponse<Vec<Buddy>> = client.fetch_valorant_api("buddies").await?;
    for buddy in &resp.data {
        cache.buddies.insert(buddy.uuid.to_lowercase(), buddy.clone());
    }
    Ok(())
}

async fn fetch_player_titles(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let resp: ValorantApiResponse<Vec<PlayerTitle>> =
        client.fetch_valorant_api("playertitles").await?;
    for title in &resp.data {
        cache
            .player_titles
            .insert(title.uuid.to_lowercase(), title.clone());
    }
    Ok(())
}

async fn fetch_player_cards(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let resp: ValorantApiResponse<Vec<PlayerCard>> =
        client.fetch_valorant_api("playercards").await?;
    for card in &resp.data {
        cache
            .player_cards
            .insert(card.uuid.to_lowercase(), card.clone());
    }
    Ok(())
}

async fn fetch_competitive_tiers(client: &ApiClient, cache: &mut ContentCache) -> Result<(), ApiError> {
    let resp: ValorantApiResponse<Vec<CompetitiveTiers>> =
        client.fetch_valorant_api("competitivetiers").await?;
    if let Some(latest) = resp.data.last() {
        cache.competitive_tiers = latest.tiers.clone();
        for tier in &latest.tiers {
            if tier.tier as usize >= cache.rank_icons.len() {
                cache.rank_icons.resize(tier.tier as usize + 1, None);
            }
            cache.rank_icons[tier.tier as usize] = tier.small_icon.clone();
        }
    }
    Ok(())
}

async fn fetch_seasons(
    client: &ApiClient,
    region_shard: &str,
    cache: &mut ContentCache,
    entitlements: &crate::models::auth::Entitlements,
    client_version: &str,
) -> Result<(String, Option<String>), ApiError> {
    let headers = entitlements.build_headers(client_version);
    let content: serde_json::Value = client
        .fetch_json(
            UrlType::Custom,
            &format!(
                "https://shared.{}.a.pvp.net/content-service/v3/content",
                region_shard
            ),
            &headers,
        )
        .await?;

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
            if season["IsActive"] == true && season["Type"] == "act" {
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
                }
            }
        }
    }

    Ok((current_season_id, previous_season_id))
}

pub fn is_before_ascendant(season_id: &str) -> bool {
    BEFORE_ASCENDANT_SEASONS.contains(&season_id)
}
