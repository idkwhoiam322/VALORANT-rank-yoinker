use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use lru::LruCache;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
use crate::api::response_helpers::first_str;
use crate::models::auth::Entitlements;

/// Bounds the names cache so it cannot grow unbounded; TTL still applies on top.
const NAMES_CACHE_CAP: usize = 1000;

pub struct NamesService {
    client: Arc<ApiClient>,
    cache: Mutex<LruCache<String, (String, Instant)>>,
    cache_ttl: Duration,
}

impl NamesService {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(NAMES_CACHE_CAP).unwrap())),
            cache_ttl: Duration::from_secs(300),
        }
    }

    pub async fn clear_cache(&self) {
        self.cache.lock().await.clear();
    }

    pub async fn get_names_from_puuids(
        &self,
        entitlements: &Entitlements,
        client_version: &str,
        puuids: &[String],
    ) -> Result<HashMap<String, String>, ApiError> {
        if puuids.is_empty() {
            return Ok(HashMap::new());
        }

        // Separate cached (fresh) from uncached / expired PUUIDs
        let mut cached_names = HashMap::new();
        let mut missing = Vec::new();

        {
            let mut cache = self.cache.lock().await;
            for p in puuids {
                if let Some((name, time)) = cache.get(p.as_str()) {
                    if time.elapsed() < self.cache_ttl {
                        self.client
                            .cache_hit("names", &crate::api::client::anon_id(p), None);
                        cached_names.insert(p.clone(), name.clone());
                        continue;
                    }
                }
                missing.push(p.clone());
            }
        }

        if !missing.is_empty() {
            // Resolve via local API then PD fallback
            let mut newly_resolved: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let headers = entitlements.build_headers(client_version);
            let body = serde_json::json!({ "puuids": missing });
            match self
                .client
                .fetch_json_with_body::<serde_json::Value>(
                    UrlType::Local,
                    endpoints::LOCAL_NAME_LOOKUP,
                    &headers,
                    body,
                )
                .await
            {
                Ok(val) => {
                    if let Some(entries) = val.get("namesets").and_then(|v| v.as_array()) {
                        for entry in entries {
                            let puuid = entry.get("puuid").and_then(|v| v.as_str());
                            let alias = entry.get("alias");
                            if let (Some(puuid), Some(alias)) = (puuid, alias) {
                                let game_name =
                                    first_str(alias, &["gameName", "game_name", "GameName"]);
                                let tag_line =
                                    first_str(alias, &["tagLine", "tag_line", "TagLine"]);
                                if let (Some(game_name), Some(tag_line)) = (game_name, tag_line) {
                                    let name = format!("{}#{}", game_name, tag_line);
                                    cached_names.insert(puuid.to_string(), name);
                                    newly_resolved.insert(puuid.to_string());
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("names: local name lookup failed: {e}");
                }
            }

            // Fallback: fetch remaining via PD name-service only if local
            // resolution didn't resolve all requested puuids.
            let still_missing: Vec<String> = missing
                .iter()
                .filter(|p| !newly_resolved.contains(*p))
                .cloned()
                .collect();

            if !still_missing.is_empty() {
                match self
                    .client
                    .fetch_json_retry_with_body::<serde_json::Value>(
                        UrlType::Pd,
                        endpoints::PD_NAME_SERVICE,
                        entitlements,
                        client_version,
                        serde_json::json!(&still_missing),
                        Some(reqwest::Method::PUT),
                        3,
                        Duration::from_secs(1),
                        |j| j.is_array(),
                    )
                    .await
                {
                    Ok(val) => {
                        if let Some(arr) = val.as_array() {
                            for player in arr {
                                let subject = first_str(player, &["Subject", "subject"]);
                                let game_name = first_str(player, &["GameName", "game_name"]);
                                let tag_line = first_str(player, &["TagLine", "tag_line"]);
                                if let (Some(subject), Some(game_name), Some(tag_line)) =
                                    (subject, game_name, tag_line)
                                {
                                    let name = format!("{}#{}", game_name, tag_line);
                                    cached_names.insert(subject.to_string(), name);
                                    newly_resolved.insert(subject.to_string());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("names: PD name service fallback failed: {e}");
                    }
                }
            }

            // Store only the newly resolved names in cache (double-check: another
            // task may have inserted a fresh entry while we were fetching). The
            // flush is scoped to `newly_resolved` instead of re-iterating the full
            // `cached_names` set, so pre-existing entries are left untouched.
            let mut cache = self.cache.lock().await;
            for p in &newly_resolved {
                let Some(name) = cached_names.get(p) else {
                    continue;
                };
                if let Some((_, time)) = cache.get(p.as_str()) {
                    if time.elapsed() < self.cache_ttl {
                        continue; // a concurrent fetch already cached this
                    }
                }
                cache.put(p.clone(), (name.clone(), Instant::now()));
            }
        }

        Ok(cached_names)
    }
}
