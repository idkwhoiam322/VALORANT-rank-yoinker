use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

use crate::api::auth;
use crate::api::client::{ApiClient, UrlType};
use crate::api::content::fetch_all_content;
use crate::core::payload_builder::build_heartbeat;
use crate::models::auth::Entitlements;
use crate::models::content::ContentCache;
use crate::models::heartbeat::HeartbeatPayload;
use crate::models::mmr::{PlayerRank, PlayerStats};
use crate::models::presences::GameState;
use crate::services::config::ConfigManager;
use crate::services::encounters::EncounterService;
use crate::services::loadouts::LoadoutService;
use crate::services::logging::Logger;
use crate::services::names::NamesService;
use crate::services::presences::PresenceService;
use crate::services::rank::RankService;
use crate::services::stats::StatsService;
use crate::services::websocket_presence::ValorantWs;


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
    pub season_id: String,
    pub previous_season_id: Option<String>,
    pub cooldown: u64,
    pub weapon_name: String,
    /// Match-scoped cache keyed by PUUID, cleared on MENUS transition.
    pub match_player_cache: Arc<std::sync::Mutex<HashMap<String, (PlayerRank, PlayerStats)>>>,
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
        self.match_player_cache.lock().unwrap().clear();
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
    pub entitlements: Option<Entitlements>,
    pub client_version: String,
    pub puuid: String,
    pub content: Arc<ContentCache>,
    pub season_id: String,
    pub previous_season_id: Option<String>,
    pub match_player_cache: Arc<std::sync::Mutex<HashMap<String, (PlayerRank, PlayerStats)>>>,
}

impl AppServices {
    pub fn new(root: std::path::PathBuf, client: ApiClient) -> Self {
        let client = Arc::new(client);
        let logger = Arc::new(Logger::new(root.clone()));
        let config = ConfigManager::new(root.clone());
        let encounters = Arc::new(EncounterService::new(root.clone()));
        let presences = Arc::new(PresenceService::new(client.clone()));
        let rank = Arc::new(RankService::new(client.clone()));
        let stats = Arc::new(StatsService::new(client.clone()));
        let names = Arc::new(NamesService::new(client.clone()));
        let loadouts = Arc::new(LoadoutService::new(client.clone()));

        let heartbeat_log_path = root.join("logs").join("heartbeat.jsonl");
        let _ = fs::File::create(&heartbeat_log_path);

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
            entitlements: None,
            client_version: String::new(),
            puuid: String::new(),
            content: Arc::new(ContentCache::empty()),
            season_id: String::new(),
            previous_season_id: None,
            match_player_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
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
            weapon_name: self.config.get().weapon.clone(),
            match_player_cache: self.match_player_cache.clone(),
        }
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

        loop {
            match self.try_initialize(&app).await {
                Ok(()) => {
                    if let Err(e) = self.run_main_loop(&app).await {
                        let svc = services.read().await;
                        svc.log(&format!("Main loop error: {e}, reconnecting..."));
                    }
                }
                Err(e) => {
                    let svc = services.read().await;
                    svc.log(&format!("Init error: {e}, retrying in 5s..."));
                    drop(svc);
                    tokio::time::sleep(Duration::from_secs(5)).await;
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
        *self.lockfile_port.lock().unwrap() = Some(lockfile.port);

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
        svc.entitlements = Some(entitlements.clone());
        svc.client_version = client_version.clone();
        svc.puuid = entitlements.subject.clone();

        // 5. Update local auth on client
        svc.client.set_local_auth(lockfile.password.clone(), lockfile.port);

        // 6. Fetch all game content (cached for session)
        let (content, season_id, previous_season_id) =
            fetch_all_content(&svc.client, &region.shard, &entitlements, &client_version).await;
        svc.content = Arc::new(content);
        svc.season_id = season_id;
        svc.previous_season_id = previous_season_id;
        svc.log("Content cache initialized");

        // 7. Notify frontend
        let _ = app.emit("backend_ready", serde_json::json!({
            "puuid": entitlements.subject,
        }));

        Ok(())
    }

    async fn run_main_loop(&self, app: &AppHandle) -> Result<(), String> {
        let services = self.services.clone();
        let mut last_state: Option<GameState> = None;
        let mut last_heartbeat_key: Option<String> = None;
        let mut match_context: Option<(String, String)> = None;

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
                        snap.logger.log("WS presence unavailable — using polling");
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

                let entitlements = match &svc.entitlements {
                    Some(e) => e.clone(),
                    None => {
                        drop(svc);
                        return Err("Entitlements cleared — re-initializing".into());
                    }
                };
                let cv = svc.client_version.clone();
                let puuid = svc.puuid.clone();
                let snap = svc.snapshot();

                (snap, entitlements, cv, puuid)
            };
            // RwLock guard is dropped here – all processing below happens
            // without holding it, allowing concurrent writes (e.g. re-init).

            // ----- State detection: WebSocket (preferred) or polling (fallback) -----
            let current_state = self
                .detect_state(&snap, &entitlements, &cv, &puuid, &mut ws, last_state)
                .await;

            let current_state = match current_state {
                Some(s) => s,
                None => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            // During INGAME steady state: suppress all heartbeat/API processing
            if last_state == Some(GameState::INGAME) && current_state == GameState::INGAME {
                tokio::time::sleep(Duration::from_secs(snap.cooldown)).await;
                continue;
            }

            // State transition: INGAME -> not INGAME => update encounter results
            if last_state == Some(GameState::INGAME) && current_state != GameState::INGAME {
                if let Some((ref match_id, ref my_team)) = match_context.take() {
                    snap.logger.log(&format!("Match ended: {match_id} team {my_team}"));
                    let headers = entitlements.build_headers(&cv);
                    if let Ok(resp) = snap.client.fetch(
                        UrlType::Pd,
                        &format!("/match-details/v1/matches/{match_id}"),
                        &headers,
                        None,
                    ).await {
                        if let Ok(text) = resp.text().await {
                            if let Ok(match_data) = serde_json::from_str::<serde_json::Value>(&text) {
                                let winning_team = match_data["matchInfo"]["winningTeam"]
                                    .as_str()
                                    .or_else(|| match_data["matchInfo"]["WinningTeam"].as_str())
                                    .or_else(|| {
                                        match_data["teams"].as_array().and_then(|teams| {
                                            teams.iter().find(|t| t["won"].as_bool() == Some(true))
                                                .and_then(|t| {
                                                    t["teamId"].as_str()
                                                        .or_else(|| t["teamID"].as_str())
                                                        .or_else(|| t["TeamID"].as_str())
                                                })
                                        })
                                    });
                                let score = (|| -> Option<String> {
                                    let teams = match_data["teams"].as_array()?;
                                    if teams.len() < 2 { return None; }
                                    let t0 = teams[0]["roundsWon"].as_i64().or_else(|| teams[0]["RoundsWon"].as_i64()).unwrap_or(0);
                                    let t1 = teams[1]["roundsWon"].as_i64().or_else(|| teams[1]["RoundsWon"].as_i64()).unwrap_or(0);
                                    Some(format!("{t0}-{t1}"))
                                })();
                                if let Some(winning_team) = winning_team {
                                    snap.encounters.update_match_result(match_id, my_team, winning_team, score.clone());
                                    snap.logger.log(&format!("Updated encounter results: winning_team={winning_team}, score={}", score.as_deref().unwrap_or("unknown")));
                                } else {
                                    snap.logger.log("Match ended but could not determine winning team (match details may not be ready yet)");
                                }
                            }
                        }
                    }
                }
            }

            // State changed or first run — log, emit event, invalidate caches
            let is_transition = last_state != Some(current_state);
            if is_transition {
                snap.logger.log(&format!("State change: {:?} -> {:?}", last_state, current_state));
                let _ = app.emit("state_change", serde_json::json!({
                    "state": current_state.as_str(),
                }));

                if current_state == GameState::MENUS {
                    snap.rank.invalidate_cache();
                    snap.stats.clear_cache();
                    snap.clear_match_player_cache();
                }

                last_state = Some(current_state);
            }

            // Build heartbeat on state changes or periodic MENUS refresh
            // (MENUS rebuilds every loop so the frontend gets latest party members)
            if current_state != GameState::DISCONNECTED && (is_transition || current_state == GameState::MENUS) {
                // Fetch match context first (for INGAME/PREGAME) to avoid redundant
                // player-endpoint calls in build_heartbeat. Returns match_id, my_team,
                // and the raw match data which is reused by the heartbeat builder.
                let match_ctx = match current_state {
                    GameState::INGAME | GameState::PREGAME => {
                        crate::core::payload_builder::get_match_context(
                            &snap, &entitlements, &cv, &puuid, current_state,
                        ).await
                    }
                    _ => None,
                };

                let (known_match_id, pre_fetched_data) = match match_ctx {
                    Some((id, team, data)) => {
                        if current_state == GameState::INGAME {
                            match_context = Some((id.clone(), team));
                        }
                        (Some(id), Some(data))
                    }
                    None => (None, None),
                };

                let mut heartbeat = build_heartbeat(
                    &snap, &entitlements, &cv, &puuid, current_state,
                    known_match_id.as_deref(),
                    pre_fetched_data,
                )
                .await;

                let key = heartbeat.time.to_string();
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

            tokio::time::sleep(Duration::from_secs(snap.cooldown)).await;
        }
    }

    /// Detect game state using WebSocket (preferred) or HTTP polling (fallback).
    ///
    /// When WS is connected, races the WS channel against a cooldown timer:
    ///   - WS push arrives (~0ms) → return new state immediately, zero API calls.
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
    ) -> Option<GameState> {
        if let Some(ws_inner) = ws.as_mut() {
            tokio::select! {
                result = ws_inner.recv() => {
                    match result {
                        Some(state) => Some(state),  // WS push — instant detection
                        None => {
                            snap.logger.log("WS disconnected — falling back to polling");
                            *ws = None;
                            // Poll once to get current state
                            let (state, log_msg) =
                                snap.presences.detect_game_state_from_poll(entitlements, cv, puuid).await;
                            if let Some(msg) = log_msg { snap.logger.log(&msg); }
                            state
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(snap.cooldown)) => {
                    last_state  // wake-up only, no API call
                }
            }
        } else {
            // WS unavailable: poll at 1s interval
            let (state, log_msg) =
                snap.presences.detect_game_state_from_poll(entitlements, cv, puuid).await;
            if let Some(msg) = log_msg {
                snap.logger.log(&msg);
            }
            state
        }
    }

}
