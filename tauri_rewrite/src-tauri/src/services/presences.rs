use std::sync::Arc;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
use crate::models::auth::Entitlements;
use crate::models::presences::{GameState, Presence, PresencesResponse};

pub struct PresenceService {
    client: Arc<ApiClient>,
}

impl PresenceService {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self { client }
    }

    pub async fn get_presences(
        &self,
        entitlements: &Entitlements,
        client_version: &str,
    ) -> Result<Vec<Presence>, ApiError> {
        let headers = entitlements.build_headers(client_version);
        let resp: PresencesResponse = self
            .client
            .fetch_json(UrlType::Local, endpoints::LOCAL_PRESENCES, &headers)
            .await?;
        Ok(resp.presences)
    }

    pub fn find_own_presence<'a>(
        presences: &'a [Presence],
        puuid: &str,
    ) -> Option<&'a Presence> {
        presences
            .iter()
            .find(|p| p.puuid.as_deref() == Some(puuid) && p.product.as_deref() == Some("valorant"))
    }

    pub fn decode_private_presence(private_val: &Option<String>) -> Option<serde_json::Value> {
        let b64 = private_val.as_deref().unwrap_or("");
        if b64.is_empty() {
            return None;
        }
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            b64,
        ).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn extract_game_state(private: &serde_json::Value) -> Option<String> {
        if let Some(state) = private
            .get("matchPresenceData")
            .and_then(|mpd| mpd.get("sessionLoopState"))
            .and_then(|v| v.as_str())
        {
            return Some(state.to_string());
        }
        private
            .get("sessionLoopState")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    pub fn extract_queue_id(private: &serde_json::Value) -> Option<String> {
        // Check nested matchPresenceData first, then flat queueId
        if let Some(qid) = private
            .get("matchPresenceData")
            .and_then(|mpd| mpd.get("queueId"))
            .and_then(|v| v.as_str())
        {
            return Some(qid.to_string());
        }
        private
            .get("queueId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Extract account level from private presence (nested or flat)
    pub fn extract_account_level(private: &serde_json::Value) -> Option<u32> {
        if let Some(level) = private
            .get("playerPresenceData")
            .and_then(|ppd| ppd.get("accountLevel"))
            .and_then(|v| v.as_u64())
        {
            return Some(level as u32);
        }
        private
            .get("accountLevel")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
    }

    /// Riot swaps between a nested `partyPresenceData.partyId` and a flat
    /// `partyId` field depending on the endpoint/client version. Check both.
    pub fn extract_party_id(private: &serde_json::Value) -> Option<String> {
        if let Some(id) = private
            .get("partyPresenceData")
            .and_then(|ppd| ppd.get("partyId"))
            .and_then(|v| v.as_str())
        {
            return Some(id.to_string());
        }
        private
            .get("partyId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Returns the puuids of every presence that shares `self_puuid`'s current
    /// party (mirrors Python's `Menu.get_party_members`).
    pub fn find_party_member_puuids(presences: &[Presence], self_puuid: &str) -> Vec<String> {
        let own_party_id = presences
            .iter()
            .find(|p| p.puuid.as_deref() == Some(self_puuid))
            .and_then(|p| Self::decode_private_presence(&p.private))
            .and_then(|private| Self::extract_party_id(&private));

        let Some(own_party_id) = own_party_id else {
            return Vec::new();
        };

        presences
            .iter()
            .filter(|p| p.puuid.as_deref() != Some(self_puuid))
            .filter_map(|p| {
                let private = Self::decode_private_presence(&p.private)?;
                let party_id = Self::extract_party_id(&private)?;
                if party_id == own_party_id {
                    p.puuid.clone()
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn detect_game_state_from_poll(
        &self,
        entitlements: &Entitlements,
        client_version: &str,
        puuid: &str,
    ) -> (Option<GameState>, Option<String>) {
        match self.get_presences(entitlements, client_version).await {
            Ok(presences) => {
                let own = Self::find_own_presence(&presences, puuid);
                if own.is_none() {
                    let msg = "detect_game_state: own presence not found (product != valorant or puuid mismatch)".into();
                    log::warn!("{msg}");
                    return (None, Some(msg));
                }
                let own = own.unwrap();
                let private = Self::decode_private_presence(&own.private);
                if private.is_none() {
                    let packed_raw = own.private.as_deref().unwrap_or("");
                    let decoded_preview = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, packed_raw)
                        .ok()
                        .and_then(|b| Some(String::from_utf8_lossy(&b[..b.len().min(300)]).to_string()))
                        .unwrap_or_default();
                    let msg = format!("detect_game_state: failed to decode private presence (b64 len={}, decoded_preview={:?})", packed_raw.len(), decoded_preview);
                    log::warn!("{msg}");
                    return (None, Some(msg));
                }
                let private = private.unwrap();
                let state_str = Self::extract_game_state(&private);
                if state_str.is_none() {
                    let msg = "detect_game_state: could not extract sessionLoopState from private presence (camelCase rename issue?)".into();
                    log::warn!("{msg}");
                    return (None, Some(msg));
                }
                let state_str = state_str.unwrap();
                let state = GameState::from_str(&state_str);
                log::info!("detect_game_state: detected state = {:?}", state);
                (Some(state), None)
            }
            Err(e) => {
                let msg = format!("detect_game_state: get_presences failed: {e:?}");
                log::warn!("{msg}");
                (None, Some(msg))
            }
        }
    }
}
