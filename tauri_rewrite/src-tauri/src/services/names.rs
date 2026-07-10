use std::collections::HashMap;

use std::sync::Arc;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::models::auth::Entitlements;

pub struct NamesService {
    client: Arc<ApiClient>,
}

impl NamesService {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self { client }
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

        let mut names = HashMap::new();

        // Try local API first
        let headers = entitlements.build_headers(client_version);
        let body = serde_json::json!({ "puuids": puuids });
        match self
            .client
            .fetch_json_with_body::<serde_json::Value>(
                UrlType::Local,
                "/player-account/lookup/v2/namesets-for-puuids",
                &headers,
                body,
            )
            .await
        {
            Ok(val) => {
                #[cfg(debug_assertions)]
                println!("names: local API raw response: {}", serde_json::to_string(&val).unwrap_or_default().chars().take(500).collect::<String>());
                if let Some(entries) = val.get("namesets").and_then(|v| v.as_array()) {
                    for entry in entries {
                        let puuid = entry.get("puuid").and_then(|v| v.as_str());
                        let alias = entry.get("alias");
                        if let (Some(puuid), Some(alias)) = (puuid, alias) {
                            let game_name = alias.get("gameName").or_else(|| alias.get("game_name")).or_else(|| alias.get("GameName")).and_then(|v| v.as_str());
                            let tag_line = alias.get("tagLine").or_else(|| alias.get("tag_line")).or_else(|| alias.get("TagLine")).and_then(|v| v.as_str());
                            if let (Some(game_name), Some(tag_line)) = (game_name, tag_line) {
                                names.insert(puuid.to_string(), format!("{}#{}", game_name, tag_line));
                            }
                        }
                    }
                }
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                println!("names: local API failed: {_e:?}");
            }
        }

        // Fallback: fetch missing via PD name-service
        let failed: Vec<String> = puuids
            .iter()
            .filter(|p| !names.contains_key(*p))
            .cloned()
            .collect();

        if !failed.is_empty() {
            #[cfg(debug_assertions)]
            println!("names: falling back to PD for {} puuids", failed.len());
            let pd_headers = entitlements.build_headers(client_version);
            let resp = self
                .client
                .fetch_put_json_with_body::<serde_json::Value>(
                    UrlType::Pd,
                    "/name-service/v2/players",
                    &pd_headers,
                    serde_json::json!(failed),
                )
                .await;

            match resp {
                Ok(val) => {
                    if let Some(arr) = val.as_array() {
                        #[cfg(debug_assertions)]
                        println!("names: PD fallback returned {} players", arr.len());
                        for player in arr {
                            let subject = player.get("Subject").or_else(|| player.get("subject")).and_then(|v| v.as_str());
                            let game_name = player.get("GameName").or_else(|| player.get("game_name")).and_then(|v| v.as_str());
                            let tag_line = player.get("TagLine").or_else(|| player.get("tag_line")).and_then(|v| v.as_str());
                            if let (Some(subject), Some(game_name), Some(tag_line)) = (subject, game_name, tag_line) {
                                names.insert(subject.to_string(), format!("{}#{}", game_name, tag_line));
                            }
                        }
                    } else {
                        #[cfg(debug_assertions)]
                        println!("names: PD fallback returned non-array: {}", serde_json::to_string(&val).unwrap_or_default().chars().take(300).collect::<String>());
                    }
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    println!("names: PD fallback failed: {_e:?}");
                }
            }
        }

        #[cfg(debug_assertions)]
        println!("names: resolved {} of {} puuids", names.len(), puuids.len());

        Ok(names)
    }
}
