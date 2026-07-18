use std::collections::HashMap;
use std::time::{Duration, Instant};

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
use crate::api::response_helpers::first_str;
use crate::models::auth::Entitlements;

pub struct NamesService {
    client: Arc<ApiClient>,
    cache: Mutex<HashMap<String, (String, Instant)>>,
    cache_ttl: Duration,
}

impl NamesService {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            cache: Mutex::new(HashMap::new()),
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
            let cache = self.cache.lock().await;
            for p in puuids {
                if let Some((name, time)) = cache.get(p) {
                    if time.elapsed() < self.cache_ttl {
                        self.client.cache_hit("names", &crate::api::client::anon_id(&p), None);
                        cached_names.insert(p.clone(), name.clone());
                        continue;
                    }
                }
                missing.push(p.clone());
            }
        }

        if !missing.is_empty() {
            // Resolve via local API then PD fallback
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
                                let game_name = first_str(alias, &["gameName", "game_name", "GameName"]);
                                let tag_line = first_str(alias, &["tagLine", "tag_line", "TagLine"]);
                                if let (Some(game_name), Some(tag_line)) = (game_name, tag_line) {
                                    cached_names.insert(puuid.to_string(), format!("{}#{}", game_name, tag_line));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("names: local name lookup failed: {e}");
                }
            }

            // Fallback: fetch remaining via PD name-service
            let still_missing: Vec<String> = missing
                .iter()
                .filter(|p| !cached_names.contains_key(*p))
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
                                if let (Some(subject), Some(game_name), Some(tag_line)) = (subject, game_name, tag_line) {
                                    cached_names.insert(subject.to_string(), format!("{}#{}", game_name, tag_line));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("names: PD name service fallback failed: {e}");
                    }
                }
            }

            // Store newly resolved names in cache (double-check: another task
            // may have inserted a fresh entry while we were fetching).
            let mut cache = self.cache.lock().await;
            for (p, name) in &cached_names {
                if let Some((_, time)) = cache.get(p) {
                    if time.elapsed() < self.cache_ttl {
                        continue; // a concurrent fetch already cached this
                    }
                }
                cache.insert(p.clone(), (name.clone(), Instant::now()));
            }
        }

        Ok(cached_names)
    }
}
