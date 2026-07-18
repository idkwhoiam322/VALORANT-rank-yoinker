use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lru::LruCache;
use tokio::sync::Mutex;

const UPDATES_CACHE_TTL: Duration = Duration::from_secs(300);

use crate::api::client::{ApiClient, UrlType};
use crate::api::endpoints;
use crate::models::auth::Entitlements;
use crate::models::mmr::{
    CompetitiveUpdatesResponse, MatchDetailsResponse, PlayerStats,
};

pub struct StatsService {
    client: Arc<ApiClient>,
    match_details_cache: Mutex<LruCache<String, MatchDetailsResponse>>,
    updates_cache: Mutex<HashMap<String, (PlayerStats, Instant)>>,
}

impl StatsService {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            match_details_cache: Mutex::new(LruCache::new(NonZeroUsize::new(200).unwrap())),
            updates_cache: Mutex::new(HashMap::new()),
        }
    }

    pub async fn clear_cache(&self) {
        self.match_details_cache.lock().await.clear();
        self.updates_cache.lock().await.clear();
    }

    pub async fn get_stats(
        &self,
        entitlements: &Entitlements,
        client_version: &str,
        puuid: &str,
    ) -> PlayerStats {
        // Check TTL cache first (double-checked locking)
        {
            let cache = self.updates_cache.lock().await;
            if let Some((stats, ts)) = cache.get(puuid) {
                if ts.elapsed() < UPDATES_CACHE_TTL {
                    let ttl_left = (UPDATES_CACHE_TTL.as_secs() - ts.elapsed().as_secs()).max(0);
                    self.client.cache_hit("stats", &crate::api::client::anon_id(puuid), Some(ttl_left));
                    return stats.clone();
                }
            }
        }

        // Fetch competitive updates
        let updates: CompetitiveUpdatesResponse = match self
            .client
            .fetch_json_retry_typed(
                UrlType::Pd,
                &endpoints::pd_competitive_updates(puuid),
                entitlements,
                client_version,
                3,
                Duration::from_secs(1),
                |j| j.get("Matches").or_else(|| j.get("matches")).is_some(),
            )
            .await
        {
            Ok(u) => u,
            Err(e) => {
                log::warn!("stats: competitive updates failed for {}: {e:?}", &crate::api::client::anon_id(puuid));
                return PlayerStats::default_stats();
            },
        };

        log::debug!("stats: got {} updates for {}", updates.matches.len(), &crate::api::client::anon_id(puuid));

        let match_summary = match updates.matches.first() {
            Some(m) => m,
            None => {
                log::debug!("stats: no matches for {}", &crate::api::client::anon_id(puuid));
                return PlayerStats::default_stats();
            },
        };

        let match_id = match &match_summary.match_id {
            Some(id) => id.clone(),
            None => {
                log::debug!("stats: match_id is None for {}", &crate::api::client::anon_id(puuid));
                return PlayerStats::default_stats();
            },
        };

        log::debug!("stats: match_id={}", &crate::api::client::anon_id(&match_id));

        // Fetch match details (cached with LRU eviction) - use double-checked locking
        let match_data_opt = {
            // Fast path: check cache
            let cached = {
                let mut cache = self.match_details_cache.lock().await;
                cache.get(&match_id).cloned()
            };
            if let Some(data) = cached {
                self.client.cache_hit("match details", &crate::api::client::anon_id(&match_id), None);
                Some(data)
            } else {
                // Slow path: fetch outside lock, re-acquire for insert
                match self
                    .client
                    .fetch_json_retry_typed::<MatchDetailsResponse>(
                        UrlType::Pd,
                        &endpoints::pd_match_details(&match_id),
                        entitlements,
                        client_version,
                        3,
                        Duration::from_secs(1),
                        |j| j.get("matchInfo").or_else(|| j.get("MatchInfo")).is_some(),
                    )
                    .await
                {
                    Ok(data) => {
                        let mut cache = self.match_details_cache.lock().await;
                        // Double-check: another task may have inserted while we fetched
                        if let Some(existing) = cache.get(&match_id).cloned() {
                            self.client.cache_hit("match details", &crate::api::client::anon_id(&match_id), None);
                            Some(existing)
                        } else {
                            log::debug!("stats: match details fetched ok, {} players, {} rounds",
                                data.players.len(), data.round_results.len());
                            cache.put(match_id.clone(), data.clone());
                            Some(data)
                        }
                    }
                    Err(e) => {
                        log::warn!("stats: match details fetch failed for {}: {e:?}", &crate::api::client::anon_id(&match_id));
                        None
                    },
                }
            }
        };

        let stats = self.process_match_data(match_data_opt.as_ref(), match_summary);
        let mut cache = self.updates_cache.lock().await;
        if let Some((existing, ts)) = cache.get(puuid) {
            if ts.elapsed() < UPDATES_CACHE_TTL {
                return existing.clone();
            }
        }
        cache.insert(puuid.to_string(), (stats.clone(), Instant::now()));
        stats
    }

    fn process_match_data(
        &self,
        match_data: Option<&MatchDetailsResponse>,
        summary: &crate::models::mmr::CompetitiveUpdate,
    ) -> PlayerStats {
        let match_info = match_data.and_then(|md| md.match_info.as_ref());
        let last_comp_start = summary.match_start_time
            .or_else(|| match_info.and_then(|mi| mi.game_start_millis));
        let game_length = match_info
            .and_then(|mi| mi.game_length_millis)
            .unwrap_or(0);
        let last_active_epoch = last_comp_start.map(|t| (t + game_length) / 1000);

        PlayerStats {
            last_active_epoch,
        }
    }
}
