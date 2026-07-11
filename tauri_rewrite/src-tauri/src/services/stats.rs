use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::Mutex;

use lru::LruCache;

use crate::api::client::{ApiClient, UrlType};
use crate::models::auth::Entitlements;
use crate::models::mmr::{
    CompetitiveUpdatesResponse, MatchDetailsResponse, PlayerStats,
};

pub struct StatsService {
    client: Arc<ApiClient>,
    match_details_cache: Mutex<LruCache<String, MatchDetailsResponse>>,
}

impl StatsService {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            match_details_cache: Mutex::new(LruCache::new(NonZeroUsize::new(200).unwrap())),
        }
    }

    pub fn clear_cache(&self) {
        self.match_details_cache.lock().unwrap().clear();
    }

    pub async fn get_stats(
        &self,
        entitlements: &Entitlements,
        client_version: &str,
        puuid: &str,
    ) -> PlayerStats {
        let headers = entitlements.build_headers(client_version);

        // Fetch competitive updates
        let updates: CompetitiveUpdatesResponse = match self
            .client
            .fetch_json(
                UrlType::Pd,
                &format!(
                    "/mmr/v1/players/{}/competitiveupdates?startIndex=0&endIndex=1&queue=competitive",
                    puuid
                ),
                &headers,
            )
            .await
        {
            Ok(u) => u,
                Err(_e) => {
                log::debug!("stats: competitive updates failed for {}: {_e:?}", &puuid[..8]);
                return PlayerStats::default_stats();
            },
        };

        log::debug!("stats: got {} updates for {}", updates.matches.len(), &puuid[..8]);

        let match_summary = match updates.matches.first() {
            Some(m) => m,
            None => {
                log::debug!("stats: no matches for {}", &puuid[..8]);
                return PlayerStats::default_stats();
            },
        };

        let match_id = match &match_summary.match_id {
            Some(id) => id.clone(),
            None => {
                log::debug!("stats: match_id is None for {}", &puuid[..8]);
                return PlayerStats::default_stats();
            },
        };

        log::debug!("stats: match_id={}", &match_id[..8.min(match_id.len())]);

        // Fetch match details (cached with LRU eviction)
        let match_data_opt = {
            // Check cache first (drop lock before await)
            let cached = {
                let mut cache = self.match_details_cache.lock().unwrap();
                cache.get(&match_id).cloned()
            };
            if let Some(data) = cached {
                Some(data)
            } else {
                match self
                    .client
                    .fetch_json::<MatchDetailsResponse>(
                        UrlType::Pd,
                        &format!("/match-details/v1/matches/{}", match_id),
                        &headers,
                    )
                    .await
                {
                    Ok(data) => {
                        log::debug!("stats: match details fetched ok, {} players, {} rounds",
                            data.players.len(), data.round_results.len());
                        let mut cache = self.match_details_cache.lock().unwrap();
                        cache.put(match_id.clone(), data.clone());
                        Some(data)
                    }
                    Err(e) => {
                        log::debug!("stats: match details fetch failed for {}: {e:?}", &match_id[..8]);
                        None
                    },
                }
            }
        };

        self.process_match_data(puuid, match_data_opt.as_ref(), match_summary)
    }

    fn process_match_data(
        &self,
        puuid: &str,
        match_data: Option<&MatchDetailsResponse>,
        summary: &crate::models::mmr::CompetitiveUpdate,
    ) -> PlayerStats {
        let mut total_hits = 0u32;
        let mut total_headshots = 0u32;
        let mut kills: Option<u32> = None;
        let mut deaths: Option<u32> = None;

        if let Some(md) = match_data {
            for round in &md.round_results {
                for player_stats in &round.player_stats {
                    if player_stats.subject.as_deref() == Some(puuid) {
                        for damage in &player_stats.damage {
                            total_hits += damage.legshots.unwrap_or(0)
                                + damage.bodyshots.unwrap_or(0)
                                + damage.headshots.unwrap_or(0);
                            total_headshots += damage.headshots.unwrap_or(0);
                        }
                    }
                }
            }

            for player in &md.players {
                if player.subject.as_deref() == Some(puuid) {
                    if let Some(ref stats) = player.stats {
                        kills = stats.kills;
                        deaths = stats.deaths;
                    }
                    break;
                }
            }
        }

        let kd = match (kills, deaths) {
            (Some(k), Some(d)) if d > 0 => format!("{:.2}", k as f64 / d as f64),
            (Some(k), Some(_)) => k.to_string(),
            _ => "N/A".into(),
        };

        let hs = if total_hits > 0 {
            format!("{}", (total_headshots as f64 / total_hits as f64 * 100.0) as u32)
        } else {
            "N/A".into()
        };

        let ranked_rating_earned = summary
            .ranked_rating_earned
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into());

        let afk_penalty = summary
            .afk_penalty
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into());

        let match_info = match_data.and_then(|md| md.match_info.as_ref());
        let last_comp_start = summary.match_start_time
            .or_else(|| match_info.and_then(|mi| mi.game_start_millis));
        let game_length = match_info
            .and_then(|mi| mi.game_length_millis)
            .unwrap_or(0);
        let last_active_epoch = last_comp_start.map(|t| (t + game_length) / 1000);

        PlayerStats {
            kd,
            hs,
            ranked_rating_earned,
            afk_penalty,
            last_active_epoch,
        }
    }
}
