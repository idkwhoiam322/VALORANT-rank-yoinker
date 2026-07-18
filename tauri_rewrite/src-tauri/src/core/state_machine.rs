use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use tokio::sync::RwLock;

use crate::api::auth;
use crate::api::client::{ApiClient, UrlType};
use crate::api::content::fetch_all_content;
use crate::api::endpoints;
use crate::api::response_helpers::{get_match_score, get_winning_team};
use crate::core::payload_builder::build_heartbeat;
use crate::models::auth::Entitlements;
use crate::models::content::ContentCache;
use crate::models::heartbeat::HeartbeatPayload;
use crate::models::mmr::{PlayerRank, PlayerStats};
use crate::models::presences::{GameState, Presence};
use crate::services::config::ConfigManager;
use crate::services::encounters::EncounterService;
use crate::services::loadouts::LoadoutService;
use crate::services::logging::Logger;
use crate::services::names::NamesService;
use crate::services::presences::PresenceService;
use crate::services::rank::RankService;
use crate::services::stats::StatsService;
use crate::services::websocket_presence::ValorantWs;

/// Redact obvious secrets (JWTs, long base64/hex blobs) from text that will be
/// surfaced to the frontend via the `auth_error` event. The backend log file may
/// still contain the full text; this only protects what leaves the process.
fn redact_secrets(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut token = String::new();
    let flush = |tok: &mut String, out: &mut String| {
        // A JWT is three dot-separated base64url segments; redact anything that
        // looks like one, plus any long alphanumeric blob (>= 32 chars).
        if tok.matches('.').count() == 2 && tok.len() >= 20 {
            out.push_str("[REDACTED]");
        } else if tok.len() >= 32 && tok.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+' || c == '/' || c == '=') {
            out.push_str("[REDACTED]");
        } else {
            out.push_str(tok);
        }
        tok.clear();
    };
    for c in input.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '+' || c == '/' || c == '=' {
            token.push(c);
        } else {
            flush(&mut token, &mut out);
            out.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}


// ---------------------------------------------------------------------------
// Snapshot of all services & session data extracted from the RwLock so the
// main loop can drop the guard before making HTTP calls.
// ---------------------------------------------------------------------------
pub struct ServiceSnapshot {
    pub logger: Arc<Logger>,
    pub client: Arc<ApiClient>,
    pub presences: Arc<PresenceService>,
    pub rank: Arc<RankService>,
    pub stats: Arc<StatsService>,
    pub names: Arc<NamesService>,
    pub loadouts: Arc<LoadoutService>,
    pub encounters: Arc<EncounterService>,
    pub heartbeat_log_path: std::path::PathBuf,
    pub content: Arc<ContentCache>,
    pub season_id: Arc<str>,
    pub previous_season_id: Option<String>,
    pub cooldown: u64,
    /// Match-scoped cache: match_id -> (puuid -> (PlayerRank, PlayerStats)).
    /// Cleared on MENUS transition OR when match_id changes.
    pub match_player_cache: Arc<std::sync::Mutex<HashMap<String, (PlayerRank, PlayerStats)>>>,
    /// Current match_id for cache scoping.
    pub current_match_id: Arc<std::sync::Mutex<Option<String>>>,
}

impl ServiceSnapshot {
    pub fn log_heartbeat(&self, heartbeat: &HeartbeatPayload) {
        use std::io::Write;
        if let Ok(line) = serde_json::to_string(heartbeat) {
            if let Ok(mut f) = fs::OpenOptions::new()
                .append(true).create(true).open(&self.heartbeat_log_path)
            {
                let _ = writeln!(f, "{line}");
            }
        }
    }

    pub fn clear_match_player_cache(&self) {
        let mut cache = self.match_player_cache.lock().unwrap();
        cache.clear();
        *self.current_match_id.lock().unwrap() = None;
    }

    /// Set the current match scope. If the match_id differs from the cached
    /// scope, the entire cache is cleared and the new scope is stored.
    /// Call once per heartbeat before processing players.
    pub fn set_match_cache_scope(&self, match_id: &str) {
        let mut current_id = self.current_match_id.lock().unwrap();
        if current_id.as_deref() != Some(match_id) {
            *current_id = Some(match_id.to_string());
            self.match_player_cache.lock().unwrap().clear();
        }
    }

    /// Get a single entry from the match-scoped cache for the given puuid.
    /// Pure getter - no side effects.
    pub fn get_match_cache_entry(
        &self,
        puuid: &str,
    ) -> Option<(PlayerRank, PlayerStats)> {
        self.match_player_cache.lock().unwrap().get(puuid).cloned()
    }

    /// Insert or update an entry in the match-scoped cache.
    pub fn put_match_cache_entry(
        &self,
        puuid: String,
        entry: (PlayerRank, PlayerStats),
    ) {
        self.match_player_cache.lock().unwrap().insert(puuid, entry);
    }
}

// ---------------------------------------------------------------------------
// AppServices – session state behind a RwLock so initialisation can mutate.
// ---------------------------------------------------------------------------
pub struct AppServices {
    pub logger: Arc<Logger>,
    pub config: ConfigManager,
    pub client: Arc<ApiClient>,
    pub presences: Arc<PresenceService>,
    pub rank: Arc<RankService>,
    pub stats: Arc<StatsService>,
    pub names: Arc<NamesService>,
    pub loadouts: Arc<LoadoutService>,
    pub encounters: Arc<EncounterService>,
    pub heartbeat_log_path: std::path::PathBuf,
    pub entitlements: Arc<Mutex<Option<Entitlements>>>,
    pub client_version: String,
    pub puuid: String,
    pub content: Arc<ContentCache>,
    pub season_id: Arc<str>,
    pub previous_season_id: Option<String>,
    pub match_player_cache: Arc<Mutex<HashMap<String, (PlayerRank, PlayerStats)>>>,
    pub current_match_id: Arc<Mutex<Option<String>>>,
    pub auth_retry: Arc<Notify>,
}

impl AppServices {
    pub fn new(root: std::path::PathBuf, client: ApiClient) -> Self {
        let client = Arc::new(client);
        let logger = Arc::new(Logger::new(root.clone()));
        client.set_logger(logger.clone());
        let config = ConfigManager::new(root.clone());
        let encounters = Arc::new(EncounterService::new(root.clone()));
        let presences = Arc::new(PresenceService::new(client.clone()));
        let rank = Arc::new(RankService::new(client.clone()));
        let stats = Arc::new(StatsService::new(client.clone()));
        let names = Arc::new(NamesService::new(client.clone()));
        let loadouts = Arc::new(LoadoutService::new(client.clone()));

        let heartbeat_log_path = root.join("logs").join("heartbeat.jsonl");
        let _ = fs::File::create(&heartbeat_log_path);

        let entitlements = client.entitlements_arc();

        Self {
            logger,
            config,
            client,
            presences,
            rank,
            stats,
            names,
            loadouts,
            encounters,
            heartbeat_log_path,
            entitlements,
            client_version: String::new(),
            puuid: String::new(),
            content: Arc::new(ContentCache::empty()),
            season_id: Arc::from(""),
            previous_season_id: None,
            match_player_cache: Arc::new(Mutex::new(HashMap::new())),
            current_match_id: Arc::new(Mutex::new(None)),
            auth_retry: Arc::new(Notify::new()),
        }
    }

    /// Clone the services + scalars needed during the main loop so the caller
    /// can drop the RwLock guard before making any HTTP requests.
    pub fn snapshot(&self) -> ServiceSnapshot {
        ServiceSnapshot {
            logger: self.logger.clone(),
            client: self.client.clone(),
            presences: self.presences.clone(),
            rank: self.rank.clone(),
            stats: self.stats.clone(),
            names: self.names.clone(),
            loadouts: self.loadouts.clone(),
            encounters: self.encounters.clone(),
            heartbeat_log_path: self.heartbeat_log_path.clone(),
            content: self.content.clone(),
            season_id: self.season_id.clone(),
            previous_season_id: self.previous_season_id.clone(),
            cooldown: self.config.get().cooldown,
            match_player_cache: self.match_player_cache.clone(),
            current_match_id: self.current_match_id.clone(),
        }
    }

    pub fn clear_match_player_cache(&self) {
        self.match_player_cache.lock().unwrap().clear();
        *self.current_match_id.lock().unwrap() = None;
    }

    pub fn log(&self, msg: &str) {
        self.logger.log(msg);
    }
}

pub struct MainLoop {
    pub services: Arc<RwLock<AppServices>>,
    heartbeat_version: AtomicU64,
    lockfile_port: std::sync::Mutex<Option<u16>>,
}

impl MainLoop {
    pub fn new(root: std::path::PathBuf, pd_url: String, glz_url: String) -> Self {
        let client = ApiClient::new(pd_url, glz_url);
        let services = Arc::new(RwLock::new(AppServices::new(root, client)));
        Self {
            services,
            heartbeat_version: AtomicU64::new(1),
            lockfile_port: std::sync::Mutex::new(None),
        }
    }

    pub async fn run(&self, app: AppHandle) {
        let services = self.services.clone();

        {
            let svc = services.read().await;
            svc.logger.set_app_handle(app.clone());
        }

        // Small delay so the frontend JS can register Tauri event listeners
        // before the backend emits its first events (avoiding a startup race
        // where no listeners exist yet to receive early log_update/auth_error).
        tokio::time::sleep(Duration::from_millis(500)).await;

        loop {
            match self.try_initialize(&app).await {
                Ok(()) => {
                    if let Err(e) = self.run_main_loop(&app).await {
                        let svc = services.read().await;
                        svc.log(&format!("Main loop error: {e}, reconnecting..."));
                    }
                }
                Err(e) => {
                    let is_auth_error = e.starts_with("Auth:");

                    if is_auth_error {
                        let auth_retry = {
                            let svc = services.read().await;
                            svc.log(&format!("Auth error: {e}. Waiting for user to click Refresh..."));
                            svc.auth_retry.clone()
                        };
                        let _ = app.emit("auth_error", serde_json::json!({
                            "message": redact_secrets(&e),
                            "action": "Please sign in to Riot Client and click Refresh below."
                        }));
                        auth_retry.notified().await;
                    } else {
                        let svc = services.read().await;
                        svc.log(&format!("Init error: {e}, retrying in 5s..."));
                        drop(svc);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }

    async fn try_initialize(&self, app: &AppHandle) -> Result<(), String> {
        let services = self.services.clone();
        let mut svc = services.write().await;
        svc.log("Initializing...");

        // 1. Read lockfile
        let lockfile_path = auth::get_lockfile_path();
        if !lockfile_path.exists() {
            return Err("Lockfile not found. Is Riot Client running?".into());
        }
        let lockfile = auth::parse_lockfile(&lockfile_path)
            .map_err(|e| format!("Lockfile parse: {e}"))?;
        *self.lockfile_port.lock().unwrap_or_else(|e| e.into_inner()) = Some(lockfile.port);

        // 2. Read region from logs
        let log_path = auth::get_log_path();
        let region = auth::parse_region_from_logs(&log_path)
            .map_err(|e| format!("Region parse: {e}"))?;

        // 3. Update API URLs
        svc.client.update_urls(region.pd_url(), region.glz_url());

        // 4. Authenticate
        let (entitlements, client_version) = auth::authenticate(&svc.client, &lockfile).await
            .map_err(|e| format!("Auth: {e}"))?;
        svc.log(&format!("Authenticated as {}", entitlements.subject));
        *svc.entitlements.lock().unwrap() = Some(entitlements.clone());
        svc.client_version = client_version.clone();
        svc.client.set_client_version(&client_version);
        svc.puuid = entitlements.subject.clone();

        // 5. Update local auth on client
        svc.client.set_local_auth(lockfile.password.clone(), lockfile.port);

        // 6. Fetch all game content (cached for session)
        let (content, season_id, previous_season_id) =
            fetch_all_content(&svc.client, &region.shard, &entitlements, &client_version).await;
        svc.content = Arc::new(content);
        svc.season_id = Arc::from(season_id);
        svc.previous_season_id = previous_season_id;
        svc.log("Content cache initialized");

        // Emit rank icons once at startup. The frontend only needs them to
        // render rank badges; sending them on every heartbeat (30 URLs) was
        // wasteful - see Analysis.md 2.4.
        let _ = app.emit("rank_icons", svc.content.rank_icons.as_ref().clone());

        // 7. Notify frontend
        let _ = app.emit("cache_cleared", ());
        let _ = app.emit("backend_ready", serde_json::json!({
            "puuid": entitlements.subject,
        }));
        // Re-emit rank icons alongside backend_ready so a frontend that loads
        // (or hot-reloads) after the one-time startup emit still receives them.
        // try_initialize() re-runs on reconnect, so this also covers reconnects.
        // See reviewer note on Analysis.md 2.4.
        let _ = app.emit("rank_icons", svc.content.rank_icons.as_ref().clone());

        Ok(())
    }

    async fn run_main_loop(&self, app: &AppHandle) -> Result<(), String> {
        let services = self.services.clone();
        let mut last_state: Option<GameState> = None;
        let mut last_heartbeat_key: Option<String> = None;
        let mut last_presences: Option<Vec<Presence>> = None;
        let mut match_context: Option<(String, String)> = None;
        // Pregame loadouts are immutable during agent select, so cache the raw
        // response per match_id and reuse it across ticks. (match_id, loadouts text)
        let mut pregame_loadout_cache: Option<(String, String)> = None;
        // Last fully-populated heartbeat, retained per match_id so a tick whose
        // fresh build comes back empty (e.g. a PREGAME->INGAME 404 handoff) never
        // downgrades known data (map/server/players) to unknown within the same
        // match. (match_id, payload)
        let mut last_known_snapshot: Option<(String, HeartbeatPayload)> = None;

        // Attempt WebSocket presence detection at startup.
        let mut ws: Option<ValorantWs> = {
            let (snap, puuid) = {
                let svc = services.read().await;
                let puuid = svc.puuid.clone();
                let snap = svc.snapshot();
                (snap, puuid)
            };
            let port = { *self.lockfile_port.lock().unwrap_or_else(|e| e.into_inner()) };
            let password = snap.client.get_local_password();
            if let Some(port) = port {
                match ValorantWs::connect(port, &password, &puuid, snap.logger.clone()).await {
                    Some(ws) => Some(ws),
                    None => {
                        snap.logger.log("WS presence unavailable - using polling");
                        None
                    }
                }
            } else {
                None
            }
        };

        loop {
            // --- Snapshot phase: briefly hold read lock, then drop ---
            let (snap, entitlements, cv, puuid) = {
                let svc = services.read().await;

                let entitlements = match svc.entitlements.lock().unwrap().as_ref() {
                    Some(e) => e.clone(),
                    None => return Err("Entitlements cleared - re-initializing".into()),
                };
                let cv = svc.client_version.clone();
                let puuid = svc.puuid.clone();
                let snap = svc.snapshot();

                (snap, entitlements, cv, puuid)
            };
            // RwLock guard is dropped here – all processing below happens
            // without holding it, allowing concurrent writes (e.g. re-init).

            // ----- State detection: WebSocket (preferred) or polling (fallback) -----
            let (current_state, new_presences) = self
                .detect_state(&snap, &entitlements, &cv, &puuid, &mut ws, last_state)
                .await;

            let current_state = match current_state {
                Some(s) => s,
                None => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            // Cache presence data from WS pushes so build_heartbeat can reuse them
            // for mode/queue resolution without a separate HTTP call.
            if let Some(p) = new_presences {
                last_presences = Some(p);
            }

            // During INGAME steady state: suppress all heartbeat/API processing
            // UNLESS we still don't have match_context (map unknown) - keep retrying
            if last_state == Some(GameState::INGAME) && current_state == GameState::INGAME {
                if match_context.is_some() {
                    tokio::time::sleep(Duration::from_secs(snap.cooldown)).await;
                    continue;
                }
                snap.logger.log("INGAME map still unknown - retrying match context");
            }

            // State transition: INGAME -> not INGAME => update encounter results
            if last_state == Some(GameState::INGAME) && current_state != GameState::INGAME {
                if let Some((ref match_id, ref my_team)) = match_context.take() {
                    // Drop the last-known snapshot so the carry-over safety net
                    // cannot leak this match's data into the next one.
                    last_known_snapshot = None;
                    snap.logger.log(&format!("Match ended: {match_id} team {my_team}"));
                    match snap.client.fetch_json_retry(
                        UrlType::Pd,
                        &endpoints::pd_match_details(match_id),
                        &entitlements, &cv,
                        3, Duration::from_secs(2),
                        |j| j.get("matchInfo").or_else(|| j.get("MatchInfo")).is_some(),
                    ).await {
                        Ok(match_data) => {
                            let winning_team = get_winning_team(&match_data);
                            let score = get_match_score(&match_data);
                            if let Some(winning_team) = winning_team {
                                snap.encounters.update_match_result(match_id, my_team, &winning_team, score.clone());
                                snap.logger.log(&format!("Updated encounter results: winning_team={winning_team}, score={}", score.as_deref().unwrap_or("unknown")));
                            } else {
                                snap.logger.log("Match ended but could not determine winning team (match details may not be ready yet)");
                            }
                        }
                        Err(e) => snap.logger.log(&format!("Match details fetch failed for {match_id}: {e}")),
                    }
                }
            }

            // State changed or first run - log, emit event, invalidate caches
            let is_transition = last_state != Some(current_state);
            if is_transition {
                snap.logger.log(&format!("State change: {:?} -> {:?}", last_state, current_state));
                let _ = app.emit("state_change", serde_json::json!({
                    "state": current_state.as_str(),
                }));

                if current_state == GameState::MENUS {
                    snap.rank.invalidate_cache().await;
                    snap.stats.clear_cache().await;
                    snap.clear_match_player_cache();
                }

                last_state = Some(current_state);
            }

            // Build heartbeat on state changes or periodic MENUS refresh
            // (MENUS rebuilds every loop so the frontend gets latest party members)
            if current_state != GameState::DISCONNECTED && (is_transition || current_state == GameState::MENUS || current_state == GameState::PREGAME || current_state == GameState::INGAME) {
                // Fetch match context first (for INGAME/PREGAME) to avoid redundant
                // player-endpoint calls in build_heartbeat. Returns match_id, my_team,
                // and the raw match data which is reused by the heartbeat builder.
                //
                // When PREGAME context 404s (Riot already moved the player into the
                // live match but the local WS state is still PREGAME), fall back to an
                // INGAME context lookup for the same player. This keeps map/server/
                // players populated across the handoff instead of emitting an empty
                // "unknown" tick. In that case we build the heartbeat using the INGAME
                // builder (the fetched data is live-match shaped), so the frontend
                // sees a seamless transition with correct players/loadouts.
                let (match_ctx, used_ingame_fallback) = match current_state {
                    GameState::INGAME => {
                        let ctx = crate::core::payload_builder::get_match_context(
                            &snap, &entitlements, &cv, &puuid, current_state,
                        ).await;
                        (ctx, false)
                    }
                    GameState::PREGAME => {
                        let pregame_ctx = crate::core::payload_builder::get_match_context(
                            &snap, &entitlements, &cv, &puuid, current_state,
                        ).await;
                        match pregame_ctx {
                            Some(ctx) => (Some(ctx), false),
                            None => {
                                // PREGAME 404'd: player likely already in the live match.
                                snap.logger.log("PREGAME context 404 - falling back to INGAME context");
                                let ingame_ctx = crate::core::payload_builder::get_match_context(
                                    &snap, &entitlements, &cv, &puuid, GameState::INGAME,
                                ).await;
                                let used_fallback = ingame_ctx.is_some();
                                (ingame_ctx, used_fallback)
                            }
                        }
                    }
                    _ => (None, false),
                };

                // The state used to *build* the heartbeat. If we had to fall back to
                // the INGAME context, build as INGAME (the data is live-match shaped).
                let build_state = if used_ingame_fallback {
                    GameState::INGAME
                } else {
                    current_state
                };

                let (known_match_id, pre_fetched_data) = match match_ctx {
                    Some((id, team, data)) => {
                        // The INGAME fallback above means a PREGAME tick may carry a
                        // committed live match. Treat it exactly like a real INGAME
                        // context so match-end detection (and the "context obtained"
                        // log) fire correctly.
                        let had_context = match_context.is_some();
                        match_context = Some((id.clone(), team.clone()));
                        if !had_context {
                            snap.logger.log(&format!("INGAME match context obtained: match={id} team={team}"));
                        }
                        (Some(id), Some(data))
                    }
                    None => (None, None),
                };

                // Reuse cached pregame loadouts when the match id is unchanged.
                let cached_pregame_loadouts = if current_state == GameState::PREGAME {
                    let mid = known_match_id.clone().map(|id| id.to_string());
                    mid.as_deref()
                        .filter(|id| pregame_loadout_cache.as_ref().map_or(false, |(cid, _)| cid == id))
                        .and_then(|_id| pregame_loadout_cache.as_ref().map(|(_, text)| text.clone()))
                } else {
                    pregame_loadout_cache = None;
                    None
                };

                let (mut heartbeat, used_pregame_loadouts) = build_heartbeat(
                    &snap, &entitlements, &cv, &puuid, build_state,
                    known_match_id.as_deref(),
                    pre_fetched_data,
                    last_presences.as_deref(),
                    cached_pregame_loadouts,
                )
                .await;

                // Store the loadouts response for reuse on the next tick.
                if current_state == GameState::PREGAME {
                    if let (Some(id), Some(text)) =
                        (known_match_id.as_deref(), used_pregame_loadouts)
                    {
                        pregame_loadout_cache = Some((id.to_string(), text));
                    }
                }

                // Safety net: if the fresh build came back empty (no map and no
                // players) but we already have a fully-populated heartbeat for the
                // same match, backfill known fields so we never downgrade known
                // data (map/server/players/mode) to "unknown". Guarded by match_id
                // equality so it cannot leak stale data across matches.
                let heartbeat_is_empty =
                    heartbeat.map.is_none() && heartbeat.players.is_empty();
                if heartbeat_is_empty {
                    if let Some((snap_id, snap_payload)) = last_known_snapshot.as_ref() {
                        if known_match_id.as_deref() == Some(snap_id.as_str()) {
                            snap.logger.log(&format!(
                                "Heartbeat build empty for match={snap_id} - restoring last known data (map={})",
                                snap_payload.map.as_deref().unwrap_or("unknown")
                            ));
                            if heartbeat.map.is_none() {
                                heartbeat.map = snap_payload.map.clone();
                            }
                            if heartbeat.server.is_none() {
                                heartbeat.server = snap_payload.server.clone();
                            }
                            if heartbeat.mode.is_none() {
                                heartbeat.mode = snap_payload.mode.clone();
                            }
                            if heartbeat.players.is_empty() {
                                heartbeat.players = snap_payload.players.clone();
                            }
                            heartbeat.rank_icons = snap_payload.rank_icons.clone();
                            heartbeat.already_played_with =
                                snap_payload.already_played_with.clone();
                        }
                    }
                } else if let Some(id) = known_match_id.as_deref() {
                    // Retain this populated heartbeat for potential carry-over on a
                    // later empty tick within the same match.
                    last_known_snapshot =
                        Some((id.to_string(), heartbeat.clone()));
                }

                let key = format!("{}:{}", heartbeat.time, heartbeat.state);
                if last_heartbeat_key.as_deref() != Some(&key) {
                    heartbeat.version = self.heartbeat_version.fetch_add(1, Ordering::Relaxed);
                    snap.logger.log(&format!("Emitting heartbeat v{} state={} mode={} map={}",
                        heartbeat.version,
                        heartbeat.state,
                        heartbeat.mode.as_deref().unwrap_or("unknown"),
                        heartbeat.map.as_deref().unwrap_or("unknown")));
                    snap.log_heartbeat(&heartbeat);
                    let _ = app.emit("heartbeat", &heartbeat);
                    last_heartbeat_key = Some(key);
                }
            }

        }
    }

    /// Detect game state using WebSocket (preferred) or HTTP polling (fallback).
    ///
    /// When WS is connected, races the WS channel against a cooldown timer:
    ///   - WS push arrives (~0ms) → return new state + presence data, zero API calls.
    ///   - WS channel returns `None` → connection lost, fall back to polling.
    ///   - Cooldown timer fires (~5s) → return `last_state` as a wake-up signal for
    ///     periodic work (pending match results), still zero API calls.
    ///
    /// When WS is disconnected or unavailable:
    ///   - Poll the presence API every `STATE_POLL_INTERVAL_SECS` (1s).
    async fn detect_state(
        &self,
        snap: &ServiceSnapshot,
        entitlements: &Entitlements,
        cv: &str,
        puuid: &str,
        ws: &mut Option<ValorantWs>,
        last_state: Option<GameState>,
    ) -> (Option<GameState>, Option<Vec<Presence>>) {
        // Cold start: poll immediately instead of waiting for the cooldown timer.
        if last_state.is_none() {
            let (state, log_msg) =
                snap.presences.detect_game_state_from_poll(entitlements, cv, puuid).await;
            if let Some(msg) = log_msg { snap.logger.log(&msg); }
            return (state, None);
        }

        if let Some(ws_inner) = ws.as_mut() {
            tokio::select! {
                result = ws_inner.recv() => {
                    match result {
                        Some(event) => {
                            snap.logger.log(&format!("WS state push: {:?}", event.state));
                            (Some(event.state), Some(event.presences))
                        }
                        None => {
                            snap.logger.log(&format!("WS disconnected (was {:?}) - falling back to polling", last_state));
                            *ws = None;
                            let (state, log_msg) =
                                snap.presences.detect_game_state_from_poll(entitlements, cv, puuid).await;
                            if let Some(msg) = log_msg { snap.logger.log(&msg); }
                            (state, None)
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(snap.cooldown)) => {
                    (last_state, None)
                }
            }
        } else {
            // WS unavailable: poll at 1s interval
            let (state, log_msg) =
                snap.presences.detect_game_state_from_poll(entitlements, cv, puuid).await;
            if let Some(msg) = log_msg {
                snap.logger.log(&msg);
            }
            (state, None)
        }
    }

}
