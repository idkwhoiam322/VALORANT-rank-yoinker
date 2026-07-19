use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lru::LruCache;
use tokio::sync::Mutex;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
use crate::models::auth::Entitlements;
use crate::models::content::ContentCache;
use crate::models::mmr::{MmrResponse, PlayerRank};

/// Bounds the rank cache so it cannot grow unbounded; TTL still applies on top.
const RANK_CACHE_CAP: usize = 500;

pub struct RankService {
    client: Arc<ApiClient>,
    cache: Mutex<LruCache<String, (PlayerRank, Instant)>>,
    cache_ttl: Duration,
}

impl RankService {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(RANK_CACHE_CAP).unwrap())),
            cache_ttl: Duration::from_secs(300),
        }
    }

    pub async fn invalidate_cache(&self) {
        self.cache.lock().await.clear();
    }

    pub async fn get_rank(
        &self,
        entitlements: &Entitlements,
        client_version: &str,
        puuid: &str,
        season_id: &str,
        previous_season_id: Option<&str>,
        content: &ContentCache,
    ) -> PlayerRank {
        // Fast path: read lock
        {
            let mut cache = self.cache.lock().await;
            if let Some((rank, time)) = cache.get(puuid) {
                if time.elapsed() < self.cache_ttl {
                    let ttl_left = self.cache_ttl.as_secs().saturating_sub(time.elapsed().as_secs());
                    self.client.cache_hit("rank", &crate::api::client::anon_id(puuid), Some(ttl_left));
                    return rank.clone();
                }
            }
        }

        // Slow path: release lock before HTTP, re-acquire for double-check + insert
        let result = self.fetch_rank(entitlements, client_version, puuid, season_id, previous_season_id, content).await;
        match result {
            Ok(rank) => {
                let mut cache = self.cache.lock().await;
                if let Some((existing, time)) = cache.get(puuid) {
                    if time.elapsed() < self.cache_ttl {
                        return existing.clone();
                    }
                }
                cache.put(puuid.to_string(), (rank.clone(), Instant::now()));
                rank
            }
            Err(_) => PlayerRank::empty(),
        }
    }

    async fn fetch_rank(
        &self,
        entitlements: &Entitlements,
        client_version: &str,
        puuid: &str,
        season_id: &str,
        previous_season_id: Option<&str>,
        content: &ContentCache,
    ) -> Result<PlayerRank, ApiError> {
        let endpoint = endpoints::pd_mmr_player(puuid);

        let resp: MmrResponse = self
            .client
            .fetch_json_retry_typed(
                UrlType::Pd,
                &endpoint,
                entitlements,
                client_version,
                3,
                Duration::from_secs(1),
                |j| j.get("QueueSkills").or_else(|| j.get("queue_skills")).is_some(),
            )
            .await?;

        let mut rank = PlayerRank::empty();

        if let Some(ref qs) = resp.queue_skills {
            if let Some(ref comp) = qs.competitive {
                if let Some(ref seasons) = comp.seasonal_info_by_season_id {
                    if let Some(info) = seasons.get(season_id) {
                        rank.rank = info.competitive_tier.unwrap_or(0);
                        rank.rr = info.ranked_rating.unwrap_or(0);
                        rank.leaderboard = info.leaderboard_rank.unwrap_or(0);

                        let wins = info.number_of_wins_with_placements.unwrap_or(0);
                        let games = info.number_of_games.unwrap_or(0);
                        if games > 0 {
                            rank.wr = format!("{}", (wins as f64 / games as f64 * 100.0) as u32);
                            rank.number_of_games = games;
                        }
                    }

                    // Calculate peak rank from WinsByTier keys
                    let (max_rank, max_season_id) =
                        compute_peak_rank(seasons, rank.rank, season_id);
                    rank.peak_rank = max_rank;

                    // Get act/episode for peak rank
                    let (act, episode) = content.get_act_episode_from_act_id(&max_season_id);
                    rank.peak_rank_act = act.clone();
                    // Format peakRankAct as " (e{ep}a{act})" or " ({ep}a{act})"
                    if let (Some(ep_val), Some(act_val)) = (&episode, &act) {
                        let has_letter = ep_val.chars().any(|c| c.is_ascii_alphabetic());
                        if has_letter {
                            rank.peak_rank_act = Some(format!(" ({}a{})", ep_val, act_val));
                        } else {
                            rank.peak_rank_act = Some(format!(" (e{}a{})", ep_val, act_val));
                        }
                    }
                }
            }
        }

        // Extract previous season's rank from the same API response
        if let Some(prev_sid) = previous_season_id {
            if let Some(ref qs) = resp.queue_skills {
                if let Some(ref comp) = qs.competitive {
                    if let Some(ref seasons) = comp.seasonal_info_by_season_id {
                        if let Some(info) = seasons.get(prev_sid) {
                            rank.previous_rank = info.competitive_tier.unwrap_or(0);
                        }
                    }
                }
            }
        }

        rank.status_good = true;
        Ok(rank)
    }

}

/// Compute the peak competitive tier across all seasons from each season's
/// `wins_by_tier` map. Returns `(max_rank, max_season_id)` where `max_season_id`
/// is the season that produced the peak (used to derive the act/episode label).
/// Seasons before Ascendant need a +3 adjustment for tiers above 20 (old numbering).
fn compute_peak_rank(
    seasons: &std::collections::HashMap<String, crate::models::mmr::SeasonalInfo>,
    current_rank: u32,
    season_id: &str,
) -> (u32, String) {
    let mut max_rank = current_rank;
    let mut max_season_id = season_id.to_string();
    for (sid, sinfo) in seasons {
        if let Some(ref wbt) = sinfo.wins_by_tier {
            for tier_str in wbt.keys() {
                let mut tier_val: u32 = match tier_str.parse() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let before = crate::api::content::is_before_ascendant(sid);
                if before && tier_val > 20 {
                    tier_val += 3;
                }
                if tier_val > max_rank {
                    log::debug!(
                        "rank: peak update sid={:.12} tier={} before={} max_season={:.12}",
                        &sid[..12.min(sid.len())],
                        tier_val,
                        before,
                        &max_season_id[..12.min(max_season_id.len())]
                    );
                    max_rank = tier_val;
                    max_season_id = sid.clone();
                }
            }
        }
    }
    (max_rank, max_season_id)
}


