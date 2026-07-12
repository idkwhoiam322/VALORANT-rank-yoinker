use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::state_machine::ServiceSnapshot;
use crate::models::auth::Entitlements;
use crate::models::heartbeat::{HeartbeatPayload, PlayerHeartbeat};
use crate::models::match_data::CoregamePlayer;
use crate::models::presences::{GameState, Presence};
use crate::services::encounters::EncounterRecord;

fn parse_server(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if let Some(idx) = lower.find("gp-") {
        let after = &lower[idx + 3..];
        if let Some(dash) = after.find('-') {
            after[..dash].to_uppercase()
        } else {
            raw.to_string()
        }
    } else {
        raw.to_string()
    }
}

async fn resolve_mode_from_queue_id(queue_id: &str, payload: &mut HeartbeatPayload) {
    if payload.mode.is_some() { return; }
    if !queue_id.is_empty() {
        payload.mode = Some(crate::services::config::get_gamemode_name(queue_id).to_string());
    }
}

async fn resolve_mode_from_presence(
    svc: &ServiceSnapshot, entitlements: &Entitlements,
    client_version: &str, puuid: &str,
    payload: &mut HeartbeatPayload,
) {
    if payload.mode.is_some() { return; }
    let Ok(presences) = svc.presences.get_presences(entitlements, client_version).await else { return };
    let Some(own) = crate::services::presences::PresenceService::find_own_presence(&presences, puuid) else { return };
    let Some(private) = crate::services::presences::PresenceService::decode_private_presence(&own.private) else { return };
    let Some(qid) = crate::services::presences::PresenceService::extract_queue_id(&private) else { return };
    if !qid.is_empty() {
        payload.mode = Some(crate::services::config::get_gamemode_name(&qid).to_string());
    }
}

fn resolve_mode_from_map(map_id: &str, payload: &mut HeartbeatPayload) {
    if payload.mode.is_some() { return; }
    let lower = map_id.to_lowercase();
    if lower.contains("/game/maps/triad/triad") {
        payload.mode = Some("Deathmatch".into());
    } else if lower.contains("/game/maps/jam/jam") || lower.contains("poveglia") {
        payload.mode = Some("Custom Game".into());
    }
}

fn is_custom_game(private: &serde_json::Value) -> bool {
    private.get("provisioningFlow").and_then(|v| v.as_str()) == Some("CustomGame")
        || private
            .get("partyPresenceData")
            .and_then(|ppd| ppd.get("partyState"))
            .and_then(|v| v.as_str())
            == Some("CUSTOM_GAME_SETUP")
        || private.get("partyState").and_then(|v| v.as_str()) == Some("CUSTOM_GAME_SETUP")
}

pub async fn build_heartbeat(
    svc: &ServiceSnapshot,
    entitlements: &Entitlements,
    client_version: &str,
    puuid: &str,
    state: GameState,
    known_match_id: Option<&str>,
    existing_match_data: Option<serde_json::Value>,
) -> HeartbeatPayload {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut payload = HeartbeatPayload {
        time: now,
        state: state.as_str().to_string(),
        r#type: "heartbeat".into(),
        mode: None,
        puuid: puuid.to_string(),
        map: None,
        server: None,
        players: HashMap::new(),
        version: 0,
        rank_icons: svc.content.rank_icons.clone(),
        already_played_with: vec![],
    };

    // Fetch presences once and reuse — sub-builders receive the data to avoid re-fetching.
    let presences = svc.presences.get_presences(entitlements, client_version).await;
    if let Ok(ref p) = presences {
        if let Some(own) = crate::services::presences::PresenceService::find_own_presence(p, puuid) {
            if let Some(private) = crate::services::presences::PresenceService::decode_private_presence(&own.private) {
                if is_custom_game(&private) {
                    payload.mode = Some("Custom Game".into());
                } else if let Some(qid) = crate::services::presences::PresenceService::extract_queue_id(&private) {
                    if !qid.is_empty() {
                        payload.mode = Some(crate::services::config::get_gamemode_name(&qid).to_string());
                    }
                }
            }
        }
    }
    let presences = presences.as_ref().ok().map(|v| v.as_slice());

    match state {
        GameState::INGAME => {
            build_ingame_payload(svc, entitlements, client_version, puuid, &mut payload, known_match_id, existing_match_data).await;
        }
        GameState::PREGAME => {
            build_pregame_payload(svc, entitlements, client_version, puuid, &mut payload, known_match_id, existing_match_data).await;
        }
        GameState::MENUS => {
            build_menus_payload(svc, entitlements, client_version, puuid, &mut payload, presences).await;
        }
        GameState::DISCONNECTED => {}
    }

    payload
}

/// Returns (match_id, my_team, match_data) for the active match, or None.
/// match_data is the raw JSON from the match endpoint, reused by the heartbeat
/// builder to avoid a redundant API call.
pub async fn get_match_context(
    svc: &ServiceSnapshot,
    entitlements: &Entitlements,
    client_version: &str,
    puuid: &str,
    state: GameState,
) -> Option<(String, String, serde_json::Value)> {
    let headers = entitlements.build_headers(client_version);
    match state {
        GameState::INGAME => {
            let player_endpoint = format!("/core-game/v1/players/{}", puuid);
            let json = svc.client.fetch_json_retry(
                crate::api::client::UrlType::Glz, &player_endpoint, &headers,
                3, Duration::from_secs(1),
                |j| j["MatchID"].as_str().map_or(false, |s| !s.is_empty()),
            ).await.ok()?;
            let match_id = json["MatchID"].as_str()?.to_string();

            // Fetch match data to find self's team from Players array.
            // Also validates MapID — it may populate later than Players/TeamID.
            let match_endpoint = format!("/core-game/v1/matches/{}", match_id);
            let match_json = svc.client.fetch_json_retry(
                crate::api::client::UrlType::Glz, &match_endpoint, &headers,
                3, Duration::from_secs(2),
                |j| {
                    j["MapID"].as_str().map_or(false, |s| !s.is_empty())
                    && j["Players"].as_array().map_or(false, |a| {
                        a.iter().any(|p| p["Subject"].as_str() == Some(puuid) && p["TeamID"].as_str().is_some())
                    })
                },
            ).await.ok()?;

            let my_team = match_json["Players"].as_array()?
                .iter()
                .find(|p| p["Subject"].as_str() == Some(puuid))
                .and_then(|p| p["TeamID"].as_str())?;

            Some((match_id, my_team.to_string(), match_json))
        }
        GameState::PREGAME => {
            let endpoint = format!("/pregame/v1/players/{}", puuid);
            let json = svc.client.fetch_json_retry(
                crate::api::client::UrlType::Glz, &endpoint, &headers,
                3, Duration::from_secs(1),
                |j| j["MatchID"].as_str().map_or(false, |s| !s.is_empty()),
            ).await.ok()?;
            let match_id = json["MatchID"].as_str()?.to_string();

            // Fetch match data to find self's team.
            // Also validates MapID — it may populate later than AllyTeam.
            let match_endpoint = format!("/pregame/v1/matches/{}", match_id);
            let match_json = svc.client.fetch_json_retry(
                crate::api::client::UrlType::Glz, &match_endpoint, &headers,
                3, Duration::from_secs(2),
                |j| {
                    j["MapID"].as_str().map_or(false, |s| !s.is_empty())
                    && j["AllyTeam"]["TeamID"].as_str().map_or(false, |s| !s.is_empty())
                },
            ).await.ok()?;

            let my_team = match_json["AllyTeam"]["TeamID"].as_str()?;
            Some((match_id.to_string(), my_team.to_string(), match_json))
        }
        _ => None,
    }
}

async fn build_ingame_payload(
    svc: &ServiceSnapshot,
    entitlements: &Entitlements,
    client_version: &str,
    puuid: &str,
    payload: &mut HeartbeatPayload,
    known_match_id: Option<&str>,
    existing_match_data: Option<serde_json::Value>,
) {
    let headers = entitlements.build_headers(client_version);

    // Use pre-fetched match data (from get_match_context) if available,
    // otherwise fetch fresh with retry.
    let (mut match_data, match_id) = match existing_match_data {
        Some(data) => {
            let mid = known_match_id.unwrap_or_default().to_string();
            if mid.is_empty() { return; }
            (data, mid)
        }
        None => {
            let mid = if let Some(id) = known_match_id {
                if !id.is_empty() { id.to_string() } else { return }
            } else {
                let player_endpoint = format!("/core-game/v1/players/{}", puuid);
                match svc.client.fetch_json_retry(
                    crate::api::client::UrlType::Glz,
                    &player_endpoint,
                    &headers,
                    3,
                    Duration::from_secs(2),
                    |json| json["MatchID"].as_str().map_or(false, |s| !s.is_empty()),
                ).await {
                    Ok(json) => match json["MatchID"].as_str() {
                        Some(id) if !id.is_empty() => id.to_string(),
                        _ => return,
                    }
                    Err(_) => return,
                }
            };

            let match_endpoint = format!("/core-game/v1/matches/{}", mid);
            match svc.client.fetch_json_retry(
                crate::api::client::UrlType::Glz,
                &match_endpoint,
                &headers,
                3,
                Duration::from_secs(2),
                |json| json["MapID"].as_str().map_or(false, |s| !s.is_empty()),
            ).await {
                Ok(json) => (json, mid),
                Err(_) => return,
            }
        }
    };

    payload.map = match_data["MapID"]
        .as_str()
        .and_then(|map_id| svc.content.maps.get(&map_id.to_lowercase()))
        .cloned();

    if let Some(qid) = match_data["QueueID"].as_str() {
        resolve_mode_from_queue_id(qid, payload).await;
    }
    payload.server = match_data["GamePodID"].as_str().map(parse_server);
    resolve_mode_from_presence(svc, entitlements, client_version, puuid, payload).await;
    if let Some(map_id) = match_data["MapID"].as_str() {
        resolve_mode_from_map(map_id, payload);
    }

    // Parse players (take ownership of Players to avoid clone)
    let players: Vec<CoregamePlayer> = match serde_json::from_value(match_data["Players"].take())
    {
        Ok(p) => p,
        Err(_) => return,
    };

    // Get names for all players
    let puuids: Vec<String> = players.iter().filter_map(|p| p.subject.clone()).collect();
    let names = svc
        .names
        .get_names_from_puuids(entitlements, client_version, &puuids)
        .await
        .unwrap_or_default();

    // Find ally team
    let ally_team = players
        .iter()
        .find(|p| p.subject.as_deref() == Some(puuid))
        .and_then(|p| p.team_id.as_deref());

    // Get loadouts
    let (_weapon_lists, loadout_json) = svc
        .loadouts
        .get_match_loadouts(
            entitlements,
            client_version,
            &match_id,
            &players,
            &svc.weapon_name,
            &svc.content,
            &names,
            "game",
        )
        .await
        .unwrap_or_default();

    for player in &players {
        let subject = match player.subject.as_ref() {
            Some(s) => s.clone(),
            None => continue,
        };
        let subject_lower = subject.to_lowercase();

        // Check match-scoped cache first (locked scope only for the lookup)
        let cached = { svc.match_player_cache.lock().unwrap().get(&subject).cloned() };
        let (player_rank, player_stats) = if let Some(entry) = cached {
            entry
        } else {
            let rank = svc
                .rank
                .get_rank(
                    entitlements,
                    client_version,
                    &subject,
                    &svc.season_id,
                    svc.previous_season_id.as_deref(),
                    &svc.content,
                )
                .await;
            let stats = svc
                .stats
                .get_stats(entitlements, client_version, &subject)
                .await;
            svc.match_player_cache.lock().unwrap().insert(subject.clone(), (rank.clone(), stats.clone()));
            (rank, stats)
        };

        let previous_rank = player_rank.previous_rank;

        let agent_name = player
            .character_id
            .as_ref()
            .and_then(|cid| svc.content.agents.get(&cid.to_lowercase()))
            .cloned();

        let player_loadout = loadout_json.players.get(&subject_lower);

        let heartbeat_player = PlayerHeartbeat {
            puuid: subject.clone(),
            name: names.get(&subject).cloned(),
            party_number: 0,
            agent: agent_name.clone(),
            rank: player_rank.rank,
            peak_rank: player_rank.peak_rank,
            peak_rank_act: player_rank.peak_rank_act,
            previous_rank,
            rr: player_rank.rr,
            kd: player_stats.kd,
            headshot_percentage: player_stats.hs,
            win_percentage: Some(format!("{} ({})", player_rank.wr, player_rank.number_of_games)),
            last_active: format_last_active(player_stats.last_active_epoch),
            level: player
                .player_identity
                .as_ref()
                .and_then(|pi| pi.account_level),
            leaderboard: player_rank.leaderboard,
            agent_img_link: player.character_id.as_ref().map(|cid| {
                format!("https://media.valorant-api.com/agents/{}/displayicon.png", cid.to_lowercase())
            }),
            team: player.team_id.clone(),
            sprays: player_loadout.and_then(|p| p.sprays.clone()),
            title: player_loadout.and_then(|p| p.title.clone()),
            title_name: player_loadout.and_then(|p| p.title_name.clone()),
            player_card: player_loadout.and_then(|p| p.player_card.clone()),
            player_card_name: player_loadout.and_then(|p| p.player_card_name.clone()),
            weapons: player_loadout.and_then(|p| p.weapons.clone()),
            earned_rr: Some(player_stats.ranked_rating_earned),
        };

        // Save encounters before moving subject into the map (skip self)
        if subject_lower != puuid.to_lowercase() {
            let name = names.get(&subject).cloned().unwrap_or_else(|| "Unknown".into());
            let team_str = player.team_id.clone().unwrap_or_else(|| "Unknown".into());

            svc.encounters.save_encounter(&subject, EncounterRecord {
                name: Some(name.clone()),
                agent: agent_name.clone(),
                map: payload.map.clone(),
                rank: None,
                rr: None,
                match_id: Some(match_id.clone()),
                epoch: Some(payload.time as f64),
                relation: Some(if team_str == ally_team.unwrap_or("") { "ally".into() } else { "enemy".into() }),
                team: Some(team_str),
                my_team: ally_team.map(|t| t.to_string()),
                result: None,
                score: None,
            });

            if let Some(entry) = svc.encounters.build_encounter_summary(
                &subject,
                &match_id,
                &name,
                None,
                agent_name.as_deref(),
                payload.map.as_deref(),
            ) {
                payload.already_played_with.push(entry);
            }
        }

        payload.players.insert(subject, heartbeat_player);
    }
}

async fn build_pregame_payload(
    svc: &ServiceSnapshot,
    entitlements: &Entitlements,
    client_version: &str,
    puuid: &str,
    payload: &mut HeartbeatPayload,
    known_match_id: Option<&str>,
    existing_match_data: Option<serde_json::Value>,
) {
    let headers = entitlements.build_headers(client_version);

    // Use pre-fetched match data (from get_match_context) if available,
    // otherwise fetch fresh with retry.
    let (match_data, match_id) = match existing_match_data {
        Some(data) => {
            let mid = known_match_id.unwrap_or_default().to_string();
            if mid.is_empty() { return; }
            (data, mid)
        }
        None => {
            let mid = if let Some(id) = known_match_id {
                if !id.is_empty() { id.to_string() } else { return }
            } else {
                let player_endpoint = format!("/pregame/v1/players/{}", puuid);
                match svc.client.fetch_json_retry(
                    crate::api::client::UrlType::Glz,
                    &player_endpoint,
                    &headers,
                    3,
                    Duration::from_secs(2),
                    |json| json["MatchID"].as_str().map_or(false, |s| !s.is_empty()),
                ).await {
                    Ok(json) => match json["MatchID"].as_str() {
                        Some(id) if !id.is_empty() => id.to_string(),
                        _ => return,
                    }
                    Err(_) => return,
                }
            };

            let match_endpoint = format!("/pregame/v1/matches/{}", mid);
            match svc.client.fetch_json_retry(
                crate::api::client::UrlType::Glz,
                &match_endpoint,
                &headers,
                3,
                Duration::from_secs(2),
                |json| json["MapID"].as_str().map_or(false, |s| !s.is_empty()),
            ).await {
                Ok(json) => (json, mid),
                Err(_) => return,
            }
        }
    };

    payload.map = match_data["MapID"]
        .as_str()
        .and_then(|map_id| svc.content.maps.get(&map_id.to_lowercase()))
        .cloned();

    if let Some(qid) = match_data["QueueID"].as_str() {
        resolve_mode_from_queue_id(qid, payload).await;
    }
    payload.server = match_data["GamePodID"].as_str().map(parse_server);
    resolve_mode_from_presence(svc, entitlements, client_version, puuid, payload).await;
    if let Some(map_id) = match_data["MapID"].as_str() {
        resolve_mode_from_map(map_id, payload);
    }

    // Extract ally team players
    let mut players: Vec<CoregamePlayer> = vec![];

    if let Some(ally_team) = match_data["AllyTeam"].as_object() {
        let team_id = ally_team["TeamID"].as_str().unwrap_or("Blue");
        if let Some(ally_players) = ally_team["Players"].as_array() {
            for p in ally_players {
                let mut player = CoregamePlayer {
                    subject: p["Subject"].as_str().map(|s| s.to_string()),
                    team_id: Some(team_id.to_string()),
                    character_id: p["CharacterID"].as_str().map(|s| s.to_string()),
                    player_identity: None,
                };

                if let Some(identity) = p["PlayerIdentity"].as_object() {
                    player.player_identity = Some(crate::models::match_data::PlayerIdentity {
                        account_level: identity["AccountLevel"].as_u64().map(|v| v as u32),
                        incognito: identity["Incognito"].as_bool(),
                        hide_account_level: identity["HideAccountLevel"].as_bool(),
                        player_title_id: identity["PlayerTitleID"].as_str().map(|s| s.to_string()),
                        player_card_id: identity["PlayerCardID"].as_str().map(|s| s.to_string()),
                    });
                }
                players.push(player);
            }
        }
    }

    // Fetch loadouts once — used for enemy extraction
    let saved_loadouts_text: Option<String> = match svc.client.fetch_json_retry(
        crate::api::client::UrlType::Glz,
        &format!("/pregame/v1/matches/{}/loadouts", match_id),
        &headers,
        3,
        Duration::from_secs(2),
        |json| json["Loadouts"].as_array().map_or(false, |a| !a.is_empty()),
    ).await {
        Ok(loadouts_json_value) => {
            if let Some(loadouts) = loadouts_json_value["Loadouts"].as_array() {
                let ally_puuids: Vec<String> =
                    players.iter().filter_map(|p| p.subject.clone()).collect();
                let enemy_team_id = if match_data["AllyTeam"]["TeamID"].as_str() == Some("Blue") {
                    "Red"
                } else {
                    "Blue"
                };
                for l in loadouts {
                    if let Some(l_subject) = l["Subject"].as_str() {
                        if !ally_puuids.iter().any(|s| s == l_subject) {
                            players.push(CoregamePlayer {
                                subject: Some(l_subject.to_string()),
                                team_id: Some(enemy_team_id.to_string()),
                                character_id: l["CharacterID"].as_str().map(|s| s.to_string()),
                                player_identity: None,
                            });
                        }
                    }
                }
            }
            Some(loadouts_json_value.to_string())
        }
        Err(_) => None,
    };

    // Get names (now with enemies populated)
    let puuids: Vec<String> = players.iter().filter_map(|p| p.subject.clone()).collect();
    let names = svc
        .names
        .get_names_from_puuids(entitlements, client_version, &puuids)
        .await
        .unwrap_or_default();

    // Build loadout_json from saved response (no second HTTP call)
    let loadout_json = if let Some(ref text) = saved_loadouts_text {
        if let Ok(structured) =
            serde_json::from_str::<crate::models::loadout::CoregameLoadoutsResponse>(text)
        {
            let (_wl, lj) = svc.loadouts.build_loadout_json(
                &structured,
                &players,
                &svc.weapon_name,
                &svc.content,
                &names,
            );
            lj
        } else {
            Default::default()
        }
    } else {
        Default::default()
    };

    for player in &players {
        let subject = player.subject.clone().unwrap_or_default();

        let player_rank = svc
            .rank
            .get_rank(
                entitlements,
                client_version,
                &subject,
                &svc.season_id,
                svc.previous_season_id.as_deref(),
                &svc.content,
            )
            .await;

                let previous_rank = player_rank.previous_rank;

                let agent_name = player
                    .character_id
                    .as_ref()
                    .and_then(|cid| svc.content.agents.get(&cid.to_lowercase()))
                    .cloned();

                let player_stats = svc
                    .stats
                    .get_stats(entitlements, client_version, &subject)
                    .await;

                let player_loadout = loadout_json.players.get(&subject.to_lowercase());

                let heartbeat_player = PlayerHeartbeat {
                    puuid: subject.clone(),
                    name: names.get(&subject).cloned(),
                    party_number: 0,
                    agent: agent_name,
                    rank: player_rank.rank,
                    peak_rank: player_rank.peak_rank,
                    peak_rank_act: player_rank.peak_rank_act,
                    previous_rank,
                    rr: player_rank.rr,
                    kd: "N/A".into(),
                    headshot_percentage: "N/A".into(),
                    win_percentage: Some(format!("{} ({})", player_rank.wr, player_rank.number_of_games)),
                    last_active: format_last_active(player_stats.last_active_epoch),
            level: player
                .player_identity
                .as_ref()
                .and_then(|pi| pi.account_level),
            leaderboard: player_rank.leaderboard,
            agent_img_link: None,
            team: player.team_id.clone(),
            sprays: player_loadout.and_then(|p| p.sprays.clone()),
            title: player_loadout.and_then(|p| p.title.clone()),
            title_name: player_loadout.and_then(|p| p.title_name.clone()),
            player_card: player_loadout.and_then(|p| p.player_card.clone()),
            player_card_name: player_loadout.and_then(|p| p.player_card_name.clone()),
            weapons: player_loadout.and_then(|p| p.weapons.clone()),
            earned_rr: None,
        };

        payload.players.insert(subject, heartbeat_player);
    }
}

async fn build_menus_payload(
    svc: &ServiceSnapshot,
    entitlements: &Entitlements,
    client_version: &str,
    puuid: &str,
    payload: &mut HeartbeatPayload,
    presences: Option<&[Presence]>,
) {
    // Use caller-supplied presences (fetched once in build_heartbeat) or fetch fresh.
    let presences = match presences {
        Some(p) => p.to_vec(),
        None => match svc.presences.get_presences(entitlements, client_version).await {
            Ok(p) => p,
            Err(_) => return,
        },
    };

    // Extract self presence data: mode + account level
    let mut self_level: Option<u32> = None;
    let own_presence = crate::services::presences::PresenceService::find_own_presence(&presences, puuid);
    if let Some(own) = own_presence {
        if let Some(private) = crate::services::presences::PresenceService::decode_private_presence(&own.private) {
            if is_custom_game(&private) {
                payload.mode = Some("Custom Game".into());
            } else if let Some(qid) =
                crate::services::presences::PresenceService::extract_queue_id(&private)
            {
                if !qid.is_empty() && payload.mode.is_none() {
                    payload.mode =
                        Some(crate::services::config::get_gamemode_name(&qid).to_string());
                }
            }
            self_level =
                crate::services::presences::PresenceService::extract_account_level(&private);
        }
    }

    // Collect puuids to fetch: self + party members
    let party_puuids = crate::services::presences::PresenceService::find_party_member_puuids(
        &presences, puuid,
    );

    let mut all_puuids = vec![puuid.to_string()];
    all_puuids.extend(party_puuids.iter().filter(|p| *p != puuid).cloned());

    for subject in &all_puuids {
        let subject = subject.clone();

        let player_rank = svc
            .rank
            .get_rank(
                entitlements,
                client_version,
                &subject,
                &svc.season_id,
                svc.previous_season_id.as_deref(),
                &svc.content,
            )
            .await;
        let previous_rank = player_rank.previous_rank;

        let player_stats = if subject == puuid {
            svc
                .stats
                .get_stats(entitlements, client_version, &subject)
                .await
        } else {
            crate::models::mmr::PlayerStats::default_stats()
        };

        let heartbeat_player = PlayerHeartbeat {
            puuid: subject.clone(),
            name: None, // Will be resolved below
            party_number: 1,
            agent: None,
            rank: player_rank.rank,
            peak_rank: player_rank.peak_rank,
            peak_rank_act: player_rank.peak_rank_act,
            previous_rank,
            rr: player_rank.rr,
            kd: player_stats.kd.clone(),
            headshot_percentage: player_stats.hs.clone(),
            win_percentage: Some(format!("{} ({})", player_rank.wr, player_rank.number_of_games)),
            last_active: format_last_active(player_stats.last_active_epoch),
            level: if subject == puuid { self_level } else { None },
            leaderboard: player_rank.leaderboard,
            agent_img_link: None,
            team: None,
            sprays: None,
            title: None,
            title_name: None,
            player_card: None,
            player_card_name: None,
            weapons: None,
            earned_rr: None,
        };

        payload.players.insert(subject, heartbeat_player);
    }

    // Resolve names
    let puuids: Vec<String> = payload.players.keys().cloned().collect();
    if let Ok(names) = svc
        .names
        .get_names_from_puuids(entitlements, client_version, &puuids)
        .await
    {
        for (puuid, name) in names {
            if let Some(player) = payload.players.get_mut(&puuid) {
                player.name = Some(name);
            }
        }
    }

    // Populate already_played_with from stored encounters
    payload.already_played_with = svc.encounters.get_all_summaries(puuid);
}

fn format_last_active(epoch: Option<i64>) -> Option<String> {
    let epoch = epoch?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let diff = (now - epoch).max(0);

    if diff < 60 {
        Some("now".into())
    } else if diff < 3600 {
        Some(format!("{}m ago", diff / 60))
    } else if diff < 86400 {
        Some(format!("{}h ago", diff / 3600))
    } else {
        Some(format!("{}d ago", diff / 86400))
    }
}
