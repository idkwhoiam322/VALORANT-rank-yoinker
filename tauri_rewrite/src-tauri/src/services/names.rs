use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use std::sync::Arc;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
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
            let cache = self.cache.lock().unwrap();
            for p in puuids {
                if let Some((name, time)) = cache.get(p) {
                    if time.elapsed() < self.cache_ttl {
                        self.client.cache_hit("names", &p[..8.min(p.len())], None);
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
                                let game_name = alias.get("gameName").or_else(|| alias.get("game_name")).or_else(|| alias.get("GameName")).and_then(|v| v.as_str());
                                let tag_line = alias.get("tagLine").or_else(|| alias.get("tag_line")).or_else(|| alias.get("TagLine")).and_then(|v| v.as_str());
                                if let (Some(game_name), Some(tag_line)) = (game_name, tag_line) {
                                    cached_names.insert(puuid.to_string(), format!("{}#{}", game_name, tag_line));
                                }
                            }
                        }
                    }
                }
                Err(_e) => {}
            }

            // Fallback: fetch remaining via PD name-service
            let still_missing: Vec<String> = missing
                .iter()
                .filter(|p| !cached_names.contains_key(*p))
                .cloned()
                .collect();

            if !still_missing.is_empty() {
                if let Ok(val) = self
                    .client
                    .fetch_put_json_with_body::<serde_json::Value>(
                        UrlType::Pd,
                        endpoints::PD_NAME_SERVICE,
                        &headers,
                        serde_json::json!(still_missing),
                    )
                    .await
                {
                    if let Some(arr) = val.as_array() {
                        for player in arr {
                            let subject = player.get("Subject").or_else(|| player.get("subject")).and_then(|v| v.as_str());
                            let game_name = player.get("GameName").or_else(|| player.get("game_name")).and_then(|v| v.as_str());
                            let tag_line = player.get("TagLine").or_else(|| player.get("tag_line")).and_then(|v| v.as_str());
                            if let (Some(subject), Some(game_name), Some(tag_line)) = (subject, game_name, tag_line) {
                                cached_names.insert(subject.to_string(), format!("{}#{}", game_name, tag_line));
                            }
                        }
                    }
                }
            }

            // Store newly resolved names in cache
            let mut cache = self.cache.lock().unwrap();
            for (p, name) in &cached_names {
                cache.insert(p.clone(), (name.clone(), Instant::now()));
            }
        }

        Ok(cached_names)
    }
}
