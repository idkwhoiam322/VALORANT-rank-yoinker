use std::sync::Arc;
use std::time::Duration;

const UPDATES_CACHE_TTL: Duration = Duration::from_secs(300);
/// Bounds the updates cache so it cannot grow unbounded; TTL still applies on top.
const UPDATES_CACHE_CAP: usize = 200;
/// Match details are now TTL-bounded too (was LRU-only, so stale entries could
/// live forever).
const MATCH_DETAILS_TTL: Duration = Duration::from_secs(600);

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
use crate::models::auth::Entitlements;
use crate::models::mmr::{CompetitiveUpdatesResponse, MatchDetailsResponse, PlayerStats};
use crate::services::cache::TtlLruCache;

pub(crate) struct StatsService {
    client: Arc<ApiClient>,
    match_details_cache: TtlLruCache<String, MatchDetailsResponse>,
    updates_cache: TtlLruCache<String, PlayerStats>,
}

impl StatsService {
    pub(crate) fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            match_details_cache: TtlLruCache::new(200, MATCH_DETAILS_TTL),
            updates_cache: TtlLruCache::new(UPDATES_CACHE_CAP, UPDATES_CACHE_TTL),
        }
    }

    pub(crate) async fn clear_cache(&self) {
        self.match_details_cache.clear().await;
        self.updates_cache.clear().await;
    }

    pub(crate) async fn get_stats(
        &self,
        entitlements: &Entitlements,
        client_version: &str,
        puuid: &str,
    ) -> PlayerStats {
        // Check TTL cache first (double-checked locking)
        if let Some((stats, ttl_left)) = self.updates_cache.get(puuid).await {
            self.client
                .cache_hit("stats", &crate::api::client::anon_id(puuid), Some(ttl_left));
            return stats;
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
                log::warn!(
                    "stats: competitive updates failed for {}: {e:?}",
                    &crate::api::client::anon_id(puuid)
                );
                return PlayerStats::default_stats();
            }
        };

        log::debug!(
            "stats: got {} updates for {}",
            updates.matches.len(),
            &crate::api::client::anon_id(puuid)
        );

        let match_summary = match updates.matches.first() {
            Some(m) => m,
            None => {
                log::debug!(
                    "stats: no matches for {}",
                    &crate::api::client::anon_id(puuid)
                );
                return PlayerStats::default_stats();
            }
        };

        let stats = self.process_match_data(match_summary);
        self.updates_cache
            .insert(puuid.to_string(), stats.clone())
            .await;
        stats
    }

    /// Fetch match details for `match_id`, reusing `match_details_cache` so the
    /// match-end win/score lookup (state machine INGAME->ended transition) does
    /// not issue a duplicate `pd_match_details` call when the same match was
    /// already fetched during the per-player stats path. On a cache miss this
    /// performs the same retry+`matchInfo`-validation fetch the stats path uses,
    /// so behavior on a miss is identical to a direct `client.fetch_json_retry`.
    pub(crate) async fn get_match_details(
        &self,
        match_id: &str,
        entitlements: &Entitlements,
        client_version: &str,
    ) -> Result<MatchDetailsResponse, ApiError> {
        // Fast path: check cache
        if let Some(data) = self.match_details_cache.get(match_id).await {
            self.client.cache_hit(
                "match details",
                &crate::api::client::anon_id(match_id),
                None,
            );
            return Ok(data.0);
        }

        // Slow path: fetch, then store (store also acts as the double-check).
        let result = self
            .client
            .fetch_json_retry_typed::<MatchDetailsResponse>(
                UrlType::Pd,
                &endpoints::pd_match_details(match_id),
                entitlements,
                client_version,
                3,
                Duration::from_secs(2),
                |j| j.get("matchInfo").or_else(|| j.get("MatchInfo")).is_some(),
            )
            .await;

        match result {
            Ok(data) => {
                self.match_details_cache
                    .insert(match_id.to_string(), data.clone())
                    .await;
                Ok(data)
            }
            Err(e) => {
                log::warn!(
                    "stats: match details fetch failed for {}: {e:?}",
                    &crate::api::client::anon_id(match_id)
                );
                Err(e)
            }
        }
    }

    fn process_match_data(&self, summary: &crate::models::mmr::CompetitiveUpdate) -> PlayerStats {
        let last_active_epoch = summary.match_start_time.map(|t| t / 1000);

        PlayerStats { last_active_epoch }
    }
}
