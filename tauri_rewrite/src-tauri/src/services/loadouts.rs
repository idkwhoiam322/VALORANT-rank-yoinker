use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use std::sync::Arc;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
use crate::models::auth::Entitlements;
use crate::models::content::ContentCache;
use crate::models::match_data::CoregamePlayer;
use crate::models::loadout::{
    CoregameLoadoutsResponse, LoadoutJson, PlayerLoadoutData, SprayEntry, WeaponEntry,
};

const SOCKET_SKIN: &str = "bcef87d6-209b-46c6-8b19-fbe40bd95abc";
const SOCKET_SKIN_LEVEL: &str = "e7c63390-eda7-46e0-bb7a-a6abdacd2433";
const SOCKET_SKIN_CHROMA: &str = "3ad1b2b2-acdb-4524-852f-954a76ddae0a";
const SOCKET_BUDDY: &str = "77258665-71d1-4623-bc72-44db9bd5b3b3";

pub struct LoadoutService {
    client: Arc<ApiClient>,
}

impl LoadoutService {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self { client }
    }

    pub async fn get_match_loadouts(
        &self,
        entitlements: &Entitlements,
        client_version: &str,
        match_id: &str,
        players: &[crate::models::match_data::CoregamePlayer],
        content: &ContentCache,
        names: &HashMap<String, String>,
        state: &str,
    ) -> Result<LoadoutJson, ApiError> {
        let headers = entitlements.build_headers(client_version);
        let endpoint = if state == "game" {
            endpoints::glz_core_loadouts(match_id)
        } else {
            endpoints::glz_pregame_loadouts(match_id)
        };

        let loadouts_resp: CoregameLoadoutsResponse = self
            .client
            .fetch_json(UrlType::Glz, &endpoint, &headers)
            .await?;

        Ok(self.build_loadout_json(&loadouts_resp, players, content, names))
    }

    pub fn build_loadout_json(
        &self,
        loadouts_resp: &CoregameLoadoutsResponse,
        players: &[CoregamePlayer],
        content: &ContentCache,
        names: &HashMap<String, String>,
    ) -> LoadoutJson {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut json = LoadoutJson {
            players: HashMap::new(),
            time: now,
            map: None,
        };

        let player_map: HashMap<&str, &CoregamePlayer> = players
            .iter()
            .filter_map(|p| p.subject.as_deref().map(|s| (s, p)))
            .collect();

        for entry in &loadouts_resp.loadouts {
            let subject = match &entry.subject {
                Some(s) => s.to_lowercase(),
                None => continue,
            };

            let player = entry.subject.as_deref().and_then(|s| player_map.get(s).copied());

            let char_id = entry.character_id.as_deref().unwrap_or("").to_lowercase();

            // Build player loadout data
            let mut player_data = PlayerLoadoutData {
                name: names.get(&subject).cloned(),
                team: player.and_then(|p| p.team_id.clone()),
                level: player.and_then(|p| {
                    p.player_identity
                        .as_ref()
                        .and_then(|pi| pi.account_level)
                }),
                agent: content.agents.get(&char_id).cloned(),
                sprays: None,
                weapons: None,
                title: None,
                player_card: None,
                player_card_name: None,
            };

            // Resolve title and player card from the CoregamePlayer's identity
            if let Some(ref identity) = player.and_then(|p| p.player_identity.as_ref()) {
                if let Some(ref title_id) = identity.player_title_id {
                    if let Some(title_obj) = content.player_titles.get(&title_id.to_lowercase()) {
                        player_data.title = title_obj.title_text.clone();
                    }
                }
                if let Some(ref card_id) = identity.player_card_id {
                    if let Some(card) = content.player_cards.get(&card_id.to_lowercase()) {
                        player_data.player_card = card.large_art.clone();
                        player_data.player_card_name = card.display_name.clone();
                    }
                }
            }

            // Resolve items/expressions: try nested Loadout first, then flat top-level fields
            let loadout_items = entry
                .loadout
                .as_ref()
                .and_then(|l| l.items.as_ref())
                .or(entry.items.as_ref());
            let loadout_expressions = entry
                .loadout
                .as_ref()
                .and_then(|l| l.expressions.as_ref())
                .or(entry.expressions.as_ref());

            // Build weapons
            if let Some(items) = loadout_items {
                let mut weapons = HashMap::new();
                for (weapon_uuid, slot) in items {
                    let weapon_uuid_lower = weapon_uuid.to_lowercase();
                    if let Some(ref sockets) = slot.sockets {
                        let skin_id = sockets
                            .get(SOCKET_SKIN)
                            .and_then(|s| s.item.as_ref())
                            .and_then(|i| i.id.clone());

                        let skin_level_id = sockets
                            .get(SOCKET_SKIN_LEVEL)
                            .and_then(|s| s.item.as_ref())
                            .and_then(|i| i.id.clone());

                        let skin_chroma_id = sockets
                            .get(SOCKET_SKIN_CHROMA)
                            .and_then(|s| s.item.as_ref())
                            .and_then(|i| i.id.clone());

                        let buddy_id = sockets
                            .get(SOCKET_BUDDY)
                            .and_then(|s| s.item.as_ref())
                            .and_then(|i| i.id.clone());

                        let mut entry = WeaponEntry {
                            skin: skin_id.clone(),
                            skin_level: skin_level_id,
                            skin_chroma: skin_chroma_id.clone(),
                            weapon: None,
                            skin_display_name: None,
                            skin_display_icon: None,
                            chroma_display_name: None,
                            buddy_display_icon: None,
                            buddy_display_name: None,
                        };

                        // Resolve weapon name and skin
                        if let Some(weapon_data) = content.weapons.get(&weapon_uuid_lower) {
                            entry.weapon = Some(weapon_data.display_name.clone());

                            // Resolve skin
                            if let Some(ref sid) = skin_id {
                                if let Some(skin) = content.skins_by_uuid.get(&sid.to_lowercase()) {
                                    entry.skin_display_name = Some(skin.display_name.clone());

                                    // Resolve display icon
                                    let is_standard = skin.content_tier_uuid.is_none();
                                    if is_standard {
                                        entry.skin_display_icon = weapon_data.display_icon.clone();
                                    } else if let Some(ref chroma_id) = skin_chroma_id {
                                        for chroma in &skin.chromas {
                                            if chroma.uuid.to_lowercase() == chroma_id.to_lowercase() {
                                                entry.chroma_display_name = Some(chroma.display_name.clone());
                                                entry.skin_display_icon = chroma.display_icon.clone()
                                                    .or_else(|| chroma.full_render.clone())
                                                    .or_else(|| skin.display_icon.clone())
                                                    .or_else(|| {
                                                        skin.levels.first().and_then(|l| l.display_icon.clone())
                                                    })
                                                    .or_else(|| weapon_data.display_icon.clone());
                                                break;
                                            }
                                        }
                                    } else {
                                        entry.skin_display_icon = skin.display_icon.clone()
                                            .or_else(|| weapon_data.display_icon.clone());
                                    }

                                    // Resolve buddy
                                    if let Some(ref bid) = buddy_id {
                                        if let Some(buddy) = content.buddies.get(&bid.to_lowercase()) {
                                            entry.buddy_display_icon = buddy.display_icon.clone();
                                            entry.buddy_display_name = Some(buddy.display_name.clone());
                                        }
                                    }
                                }
                            }
                        } else {
                            log::warn!("Weapon UUID {} not found in content cache (content.weapons has {} entries)",
                                weapon_uuid_lower, content.weapons.len());
                            entry.weapon = Some(weapon_uuid.clone());
                            entry.skin_display_icon = Some(endpoints::media_weapon_icon(&weapon_uuid_lower));
                        }

                        weapons.insert(weapon_uuid_lower.clone(), entry);
                    }
                }
                player_data.weapons = Some(weapons);
            }

            // Resolve sprays
            if let Some(expressions) = loadout_expressions {
                let mut sprays = HashMap::new();
                for (i, expr) in expressions.aes_selections.iter().enumerate() {
                    if let Some(ref asset_id) = expr.asset_id {
                        let aid = asset_id.to_lowercase();
                        let (spray_type, display_name, display_icon, full_transparent_icon) =
                            if let Some(spray) = content.sprays.get(&aid) {
                                (
                                    Some("spray".into()),
                                    Some(spray.display_name.clone()),
                                    spray.display_icon.clone(),
                                    spray.full_transparent_icon.clone().or_else(|| spray.display_icon.clone()),
                                )
                            } else if let Some(flex) = content.flex.get(&aid) {
                                (
                                    Some("flex".into()),
                                    Some(flex.display_name.clone()),
                                    flex.display_icon.clone(),
                                    flex.full_transparent_icon.clone().or_else(|| flex.display_icon.clone()),
                                )
                            } else {
                                (Some("unknown".into()), None, None, None)
                            };

                        sprays.insert(
                            i.to_string(),
                            SprayEntry {
                                spray_type,
                                display_name,
                                display_icon,
                                full_transparent_icon,
                            },
                        );
                    }
                }
                player_data.sprays = Some(sprays);
            }

            json.players.insert(subject, player_data);
        }

        json
    }
}
