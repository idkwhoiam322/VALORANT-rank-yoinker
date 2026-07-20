use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use tokio::sync::RwLock;

use crate::api::auth;
use crate::api::client::ApiClient;
use crate::api::content::fetch_all_content;
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
        } else if tok.len() >= 32
            && tok.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || c == '-'
                    || c == '_'
                    || c == '+'
                    || c == '/'
                    || c == '='
            })
        {
            out.push_str("[REDACTED]");
        } else {
            out.push_str(tok);
        }
        tok.clear();
    };
    for c in input.chars() {
        if c.is_ascii_alphanumeric()
            || c == '-'
            || c == '_'
            || c == '.'
            || c == '+'
            || c == '/'
            || c == '='
        {
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
//
// NOTE: `ServiceSnapshot` and `AppServices` must be kept in sync -- every field
// added to one must be mirrored in the other (see `AppServices::snapshot()`).
// Forgetting to copy a field into the snapshot is a silent bug.
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
                .append(true)
                .create(true)
                .open(&self.heartbeat_log_path)
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
    pub fn get_match_cache_entry(&self, puuid: &str) -> Option<(PlayerRank, PlayerStats)> {
        self.match_player_cache.lock().unwrap().get(puuid).cloned()
    }

    /// Clear all volatile service caches (rank, stats, names, match-scoped).
    /// Preserves the content cache (agents, maps, weapons — stable per patch).
    /// Used by both automatic re-auth (503) and the frontend clear_all_cache command.
    pub(crate) async fn clear_volatile_caches(&self) {
        self.rank.invalidate_cache().await;
        self.stats.clear_cache().await;
        self.names.clear_cache().await;
        self.clear_match_player_cache();
    }

    /// Insert or update an entry in the match-scoped cache.
    pub fn put_match_cache_entry(&self, puuid: String, entry: (PlayerRank, PlayerStats)) {
        self.match_player_cache.lock().unwrap().insert(puuid, entry);
    }
}

// ---------------------------------------------------------------------------
// AppServices – session state behind a RwLock so initialisation can mutate.
// ---------------------------------------------------------------------------
//
// NOTE: `AppServices` and `ServiceSnapshot` must be kept in sync -- see the
// cross-reference comment on `ServiceSnapshot`. Any new session field here must
// also be copied into the snapshot in `AppServices::snapshot()`.
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
    pub restart_request: Arc<Notify>,
    pub restart_requested: Arc<AtomicBool>,
    /// Set by clear_all_cache so the running main loop drops its per-match
    /// carry-over locals (match_context / last_known_snapshot /
    /// pregame_loadout_cache) on the next tick, preventing stale match data
    /// from leaking into the next match after an explicit cache clear.
    pub loop_reset_requested: Arc<AtomicBool>,
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
            restart_request: Arc::new(Notify::new()),
            restart_requested: Arc::new(AtomicBool::new(false)),
            loop_reset_requested: Arc::new(AtomicBool::new(false)),
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
            match_player_cache: self.match_player_cache.clone(),
            current_match_id: self.current_match_id.clone(),
        }
    }

    pub fn log(&self, msg: &str) {
        self.logger.log(msg);
    }

    pub async fn clear_volatile_caches(&self) {
        self.snapshot().clear_volatile_caches().await;
    }
}

pub struct MainLoop {
    pub services: Arc<RwLock<AppServices>>,
    heartbeat_version: AtomicU64,
    session_id: AtomicU64,
    lockfile_port: std::sync::Mutex<Option<u16>>,
}

impl MainLoop {
    pub fn new(root: std::path::PathBuf, pd_url: String, glz_url: String) -> Self {
        let client = ApiClient::new(pd_url, glz_url);
        let services = Arc::new(RwLock::new(AppServices::new(root, client)));
        Self {
            services,
            heartbeat_version: AtomicU64::new(1),
            session_id: AtomicU64::new(0),
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
                        if svc.restart_requested.swap(false, Ordering::Relaxed) {
                            svc.log("Restarted by user, reinitializing...");
                        } else {
                            svc.log(&format!("Main loop error: {e}, reconnecting..."));
                        }
                    }
                }
                Err(e) => {
                    let is_auth_error = e.starts_with("Auth:");

                    if is_auth_error {
                        let restart_request = {
                            let svc = services.read().await;
                            svc.log(&format!(
                                "Auth error: {e}. Waiting for user to click Refresh..."
                            ));
                            svc.restart_request.clone()
                        };
                        let _ = app.emit(
                            "auth_error",
                            serde_json::json!({
                                "message": redact_secrets(&e),
                                "action": "Please sign in to Riot Client and click Refresh below."
                            }),
                        );
                        restart_request.notified().await;
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

        // 1. Read lockfile (launch Riot Client if fully closed, then wait)
        svc.log("Launching Riot Client if needed, waiting for lockfile…");
        let _ = app.emit("riot_client_launching", serde_json::json!({}));
        let lockfile =
            match auth::ensure_lockfile_ready(std::time::Duration::from_secs(60)).await {
                Some(lf) => lf,
                None => return Err(
                    "Riot Client did not start / lockfile not found within 60s. Is it installed?"
                        .into(),
                ),
            };
        let _ = app.emit("riot_client_waiting", serde_json::json!({}));
        *self.lockfile_port.lock().unwrap_or_else(|e| e.into_inner()) = Some(lockfile.port);

        // 2. Read region from logs
        let log_path = auth::get_log_path();
        let region =
            auth::parse_region_from_logs(&log_path).map_err(|e| format!("Region parse: {e}"))?;

        // 3. Update API URLs
        svc.client.update_urls(region.pd_url(), region.glz_url());

        // 4. Authenticate
        let (entitlements, client_version) = auth::authenticate(&svc.client, &lockfile)
            .await
            .map_err(|e| format!("Auth: {e}"))?;
        svc.log(&format!("Authenticated as {}", entitlements.subject));
        *svc.entitlements.lock().unwrap() = Some(entitlements.clone());
        svc.client_version = client_version.clone();
        svc.client.set_client_version(&client_version);
        svc.puuid = entitlements.subject.clone();

        // 5. Update local auth on client
        svc.client
            .set_local_auth(lockfile.password.clone(), lockfile.port);

        // 6. Fetch all game content (cached for session)
        let (content, season_id, previous_season_id) =
            fetch_all_content(&svc.client, &region.shard, &entitlements, &client_version).await;
        svc.content = Arc::new(content);
        svc.season_id = Arc::from(season_id);
        svc.previous_season_id = previous_season_id;
        svc.log("Content cache initialized");

        // Emit rank icons once at startup. The frontend only needs them to
        // render rank badges; sending them on every heartbeat (30 URLs) was
        // wasteful - rank icons now sent once via dedicated event.
        let _ = app.emit("rank_icons", svc.content.rank_icons.as_ref().clone());

        // 7. Notify frontend
        // Bump the session id on every (re)initialization. A new id is minted after
        // a restart or a re-auth cycle, so the frontend can reject stale events
        // (late heartbeats / state_change) from the previous session that arrive
        // out of order after the restart.
        let session_id = self.session_id.fetch_add(1, Ordering::SeqCst) + 1;
        // Reset the heartbeat version counter per session. The frontend dedups
        // payloads by (version === lastRenderKey); without this reset the globally
        // monotonic counter can collide with a value the frontend still remembers
        // from the previous session, causing the new payload to be dropped and the
        // previous match's players to render.
        self.heartbeat_version.store(1, Ordering::SeqCst);
        // Emit backend_ready (with the new sessionId) BEFORE cache_cleared so the
        // frontend re-arms its epoch guard before any late event from the previous
        // session can apply. cache_cleared now also carries the sessionId so the
        // frontend can re-arm atomically inside resetState.
        let _ = app.emit(
            "backend_ready",
            serde_json::json!({
                "puuid": entitlements.subject,
                "sessionId": session_id,
            }),
        );
        let _ = app.emit(
            "cache_cleared",
            serde_json::json!({ "sessionId": session_id }),
        );

        // Clear any stale 503 flag that may remain from a failed re-auth in a
        // previous run_main_loop cycle. Without this, the first iteration of
        // the new main loop would spuriously trigger another re-auth despite
        // try_initialize having just set up valid credentials.
        svc.client.clear_local_api_dead();

        Ok(())
    }

    async fn run_main_loop(&self, app: &AppHandle) -> Result<(), String> {
        let services = self.services.clone();
        let mut last_state: Option<GameState> = None;
        let mut last_emitted: Option<HeartbeatPayload> = None;
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
                match ValorantWs::connect(
                    port,
                    password.expose_secret(),
                    &puuid,
                    snap.logger.clone(),
                )
                .await
                {
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

            // Honor an explicit cache-clear request: drop per-match carry-over
            // locals so stale match data can't leak into the next match.
            if services
                .read()
                .await
                .loop_reset_requested
                .swap(false, Ordering::Relaxed)
            {
                pregame_loadout_cache = None;
                last_known_snapshot = None;
                match_context = None;
                last_state = None;
                last_emitted = None;
                last_presences = None;
            }

            // ----- State detection: WebSocket (preferred) or polling (fallback) -----
            let (current_state, new_presences) = self
                .detect_state(&snap, &entitlements, &cv, &puuid, &mut ws, last_state)
                .await;

            // Detect 503 from local API — Riot client session expired (e.g. user
            // signed out). Trigger silent re-auth that preserves content cache.
            if snap.client.is_local_api_dead() {
                // Extracted 503/re-auth handling — see `handle_local_api_dead`.
                // The method returns `true` only when the confirmed-sign-out
                // path ran (caches cleared); in that case we must also reset the
                // caller's per-tick local state, mirroring the original in-loop
                // Err branch. On the plain-expiry path these are preserved.
                let cleared = self
                    .handle_local_api_dead(&app, &services, &snap, &mut ws)
                    .await;
                if cleared {
                    pregame_loadout_cache = None;
                    last_known_snapshot = None;
                    last_presences = None;
                    match_context = None;
                    last_state = None;
                    last_emitted = None;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }

            let current_state = match current_state {
                Some(s) => s,
                None => {
                    // Presence unavailable (e.g. VALORANT not running).
                    // No need to spam the local API; wait longer.
                    tokio::time::sleep(Duration::from_secs(10)).await;
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
                    let cooldown = services.read().await.config.get().cooldown;
                    tokio::time::sleep(Duration::from_secs(cooldown)).await;
                    continue;
                }
                snap.logger
                    .log("INGAME map still unknown - retrying match context");
            }

            // State transition: INGAME -> not INGAME => update encounter results
            if last_state == Some(GameState::INGAME) && current_state != GameState::INGAME {
                if let Some((ref match_id, ref my_team)) = match_context.take() {
                    // Drop the last-known snapshot so the carry-over safety net
                    // cannot leak this match's data into the next one.
                    last_known_snapshot = None;
                    snap.logger
                        .log(&format!("Match ended: {match_id} team {my_team}"));
                    match snap
                        .stats
                        .get_match_details(match_id, &entitlements, &cv)
                        .await
                    {
                        Ok(match_data) => {
                            if let Ok(json) = serde_json::to_value(&match_data) {
                                let winning_team = get_winning_team(&json);
                                let score = get_match_score(&json);
                                if let Some(winning_team) = winning_team {
                                    snap.encounters.update_match_result(
                                        match_id,
                                        my_team,
                                        &winning_team,
                                        score.clone(),
                                    );
                                    snap.logger.log(&format!("Updated encounter results: winning_team={winning_team}, score={}", score.as_deref().unwrap_or("unknown")));
                                } else {
                                    snap.logger.log("Match ended but could not determine winning team (match details may not be ready yet)");
                                }
                            } else {
                                snap.logger
                                    .log("Match ended but failed to serialize match details");
                            }
                        }
                        Err(e) => snap
                            .logger
                            .log(&format!("Match details fetch failed for {match_id}: {e}")),
                    }
                }
            }

            // Resolve the build state and fetch match context up front so the
            // state_change event and the heartbeat built later in this same tick
            // always describe the same state. When the PREGAME context 404s (Riot
            // already moved the player into the live match but the local WS/poll
            // state is still PREGAME), we fall back to the INGAME context and build
            // as INGAME. The state_change emitted below must reflect that build
            // state, not the stale detected PREGAME state — otherwise the frontend
            // receives a PREGAME transition immediately followed by an INGAME-shaped
            // heartbeat and desyncs its transition handling.
            let (match_ctx, _, build_state) = match current_state {
                GameState::INGAME => {
                    let ctx = crate::core::payload_builder::get_match_context(
                        &snap,
                        &entitlements,
                        &cv,
                        &puuid,
                        current_state,
                    )
                    .await;
                    (ctx, false, GameState::INGAME)
                }
                GameState::PREGAME => {
                    let pregame_ctx = crate::core::payload_builder::get_match_context(
                        &snap,
                        &entitlements,
                        &cv,
                        &puuid,
                        current_state,
                    )
                    .await;
                    match pregame_ctx {
                        Some(ctx) => (Some(ctx), false, GameState::PREGAME),
                        None => {
                            // PREGAME 404'd: player likely already in the live match.
                            snap.logger
                                .log("PREGAME context 404 - falling back to INGAME context");
                            let ingame_ctx = crate::core::payload_builder::get_match_context(
                                &snap,
                                &entitlements,
                                &cv,
                                &puuid,
                                GameState::INGAME,
                            )
                            .await;
                            let used_fallback = ingame_ctx.is_some();
                            (
                                ingame_ctx,
                                used_fallback,
                                if used_fallback {
                                    GameState::INGAME
                                } else {
                                    GameState::PREGAME
                                },
                            )
                        }
                    }
                }
                _ => (None, false, current_state),
            };

            // State changed or first run - log, emit event, invalidate caches.
            // The emitted state is `build_state` (not the detected `current_state`)
            // so it always matches the heartbeat that follows in this same tick.
            let is_transition = last_state != Some(current_state);
            if is_transition {
                snap.logger.log(&format!(
                    "State change: {:?} -> {:?} (build {:?})",
                    last_state, current_state, build_state
                ));
                let _ = app.emit(
                    "state_change",
                    serde_json::json!({
                        "state": build_state.as_str(),
                        "sessionId": self.session_id.load(Ordering::SeqCst),
                    }),
                );

                if build_state == GameState::MENUS {
                    snap.rank.invalidate_cache().await;
                    snap.stats.clear_cache().await;
                    snap.clear_match_player_cache();
                    // Drop per-match carry-over state. match_context is cleared on
                    // the INGAME->not-INGAME branch below, but a PREGAME->MENUS
                    // (or clear_all_cache mid-match) path can otherwise leave a
                    // stale snapshot/loadout that leaks into the next match's
                    // first empty tick via the last_known_snapshot safety net.
                    match_context = None;
                    last_known_snapshot = None;
                    pregame_loadout_cache = None;
                }

                last_state = Some(current_state);
            }

            // Build heartbeat on state changes or periodic MENUS refresh
            // (MENUS rebuilds every loop so the frontend gets latest party members)
            if current_state != GameState::DISCONNECTED
                && (is_transition
                    || current_state == GameState::MENUS
                    || current_state == GameState::PREGAME
                    || current_state == GameState::INGAME)
            {
                let (known_match_id, pre_fetched_data) = match match_ctx {
                    Some((id, team, data)) => {
                        // The INGAME fallback above means a PREGAME tick may carry a
                        // committed live match. Treat it exactly like a real INGAME
                        // context so match-end detection (and the "context obtained"
                        // log) fire correctly.
                        let had_context = match_context.is_some();
                        match_context = Some((id.clone(), team.clone()));
                        if !had_context {
                            snap.logger.log(&format!(
                                "INGAME match context obtained: match={id} team={team}"
                            ));
                        }
                        (Some(id), Some(data))
                    }
                    None => (None, None),
                };

                // Reuse cached pregame loadouts when the match id is unchanged.
                let cached_pregame_loadouts = if current_state == GameState::PREGAME {
                    let mid = known_match_id.clone().map(|id| id.to_string());
                    mid.as_deref()
                        .filter(|id| {
                            pregame_loadout_cache
                                .as_ref()
                                .map_or(false, |(cid, _)| cid == id)
                        })
                        .and_then(|_id| {
                            pregame_loadout_cache.as_ref().map(|(_, text)| text.clone())
                        })
                } else {
                    pregame_loadout_cache = None;
                    None
                };

                let (mut heartbeat, used_pregame_loadouts) = build_heartbeat(
                    &snap,
                    &entitlements,
                    &cv,
                    &puuid,
                    build_state,
                    known_match_id.as_deref(),
                    pre_fetched_data,
                    last_presences.as_deref(),
                    cached_pregame_loadouts,
                )
                .await;

                // session_id is a per-session constant, fully known as soon as the
                // heartbeat is built — assign it here, not inside the should_emit
                // branch, so a freshly-built candidate always carries the same
                // session_id as last_emitted and the dedup comparison in
                // heartbeats_equal_ignoring_time is meaningful. (Contrast with
                // `version`, which genuinely can't be known until the emit decision
                // is made below, since it's a per-emission counter.)
                heartbeat.session_id = self.session_id.load(Ordering::SeqCst);

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
                let heartbeat_is_empty = heartbeat.map.is_none() && heartbeat.players.is_empty();
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
                    last_known_snapshot = Some((id.to_string(), heartbeat.clone()));
                }

                let should_emit = match last_emitted.as_ref() {
                    Some(prev) => {
                        let eq = heartbeats_equal_ignoring_time(prev, &heartbeat);
                        if eq {
                            snap.logger.log(&format!(
                                "Heartbeat dedup: suppressing identical rebuild (same content, time advanced {}s)",
                                heartbeat.time - prev.time
                            ));
                        } else {
                            let changed = diff_heartbeat_fields(prev, &heartbeat);
                            snap.logger.log(&format!(
                                "Heartbeat dedup: rebuild differs, changed fields: {:?}",
                                changed
                            ));
                            // DEBUG: throwaway logging of prev/new values, to be removed after diagnosis.
                            let describe = |h: &HeartbeatPayload| {
                                format!(
                                "state={} type={} mode={:?} puuid={} map={:?} server={:?} players={} rank_icons={} session_id={} already_played_with={}",
                                h.state,
                                h.r#type,
                                h.mode,
                                h.puuid,
                                h.map,
                                h.server,
                                h.players.len(),
                                h.rank_icons.len(),
                                h.session_id,
                                h.already_played_with.len(),
                            )
                            };
                            snap.logger
                                .log(&format!("Heartbeat dedup DEBUG prev: {}", describe(prev)));
                            snap.logger.log(&format!(
                                "Heartbeat dedup DEBUG new:  {}",
                                describe(&heartbeat)
                            ));
                        }
                        !eq
                    }
                    None => true,
                };
                if should_emit {
                    heartbeat.version = self.heartbeat_version.fetch_add(1, Ordering::Relaxed);
                    snap.logger.log(&format!(
                        "Emitting heartbeat v{} state={} mode={} map={}",
                        heartbeat.version,
                        heartbeat.state,
                        heartbeat.mode.as_deref().unwrap_or("unknown"),
                        heartbeat.map.as_deref().unwrap_or("unknown")
                    ));
                    snap.log_heartbeat(&heartbeat);
                    let _ = app.emit("heartbeat", &heartbeat);
                    last_emitted = Some(heartbeat.clone());
                }
            }
        }
    }

    /// Handle a 503 from the local API — the Riot client session expired (e.g.
    /// the user signed out). Pulled out of the main tick loop so that loop stays
    /// scannable.
    ///
    /// Returns `true` iff the sign-out path ran (refresh failed, caches cleared
    /// and the deferred-retry loop was entered). The caller uses this to reset
    /// its own per-tick local state (`pregame_loadout_cache`, `last_*`,
    /// `match_context`, …) — those live in `run_main_loop` and cannot be touched
    /// here. On the `false` (plain-expiry) path the caller must NOT reset them.
    ///
    /// The two paths are deliberately asymmetric and must be preserved exactly:
    ///   * refresh **succeeds** (plain token expiry, user still signed in) →
    ///     caches are *preserved* (same as BAD_CLAIMS re-auth) — returns `false`.
    ///   * refresh **fails** (confirmed sign-out) → `clear_volatile_caches()` is
    ///     called and we retry silently every 5s until the user signs back in —
    ///     returns `true`.
    async fn handle_local_api_dead(
        &self,
        app: &AppHandle,
        services: &Arc<RwLock<AppServices>>,
        snap: &ServiceSnapshot,
        ws: &mut Option<ValorantWs>,
    ) -> bool {
        snap.client.clear_local_api_dead();
        snap.logger
            .log("Local API 503 — session expired, re-authenticating...");

        // Drop WS connection; re-established on next tick
        *ws = None;

        // Re-authenticate using existing lockfile auth (no lockfile re-read
        // needed — Riot Client process is still running).
        // Refreshes entitlements + client_version from local Riot client
        // endpoints, then updates ApiClient's internal state.
        //
        // If the refresh succeeds (simple token expiry, user still signed
        // in), keep caches intact — same as BAD_CLAIMS re-auth. Only clear
        // caches when the refresh confirms the user signed out (400
        // "not ready"), and then retry silently every 5s until they sign
        // back in.
        let cleared = match snap.client.refresh_entitlements_with_retry().await {
            Ok(()) => {
                let fresh_entitlements = snap.client.get_entitlements();
                let fresh_cv = snap.client.get_client_version();
                {
                    let mut svc = services.write().await;
                    svc.client_version = fresh_cv;
                    if let Some(ref e) = fresh_entitlements {
                        svc.puuid = e.subject.clone();
                    }
                }
                snap.logger.log("Re-authentication successful, resuming...");
                false
            }
            Err(e) => {
                snap.logger.log(&format!(
                    "Re-auth deferred ({e}) — Riot client session inactive, retrying until sign-in"
                ));
                snap.clear_volatile_caches().await;

                // If RC is fully closed the lockfile is gone; wait for the
                // user to (re)launch it and re-read the *fresh* port /
                // password. A backgrounded RC keeps its lockfile, so this
                // only triggers on a genuine close — never relaunches.
                if !auth::get_lockfile_path().exists() {
                    snap.logger
                        .log("Riot Client closed — waiting for it to come back…");
                    let _ = app.emit("riot_client_waiting", serde_json::json!({}));
                    if let Some(lf) =
                        auth::ensure_lockfile_ready(std::time::Duration::from_secs(60)).await
                    {
                        let fresh_port = lf.port;
                        snap.client.set_local_auth(lf.password.clone(), fresh_port);
                        *self.lockfile_port.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(fresh_port);
                    }
                }

                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    match snap.client.refresh_entitlements_with_retry().await {
                        Ok(()) => {
                            let fresh_entitlements = snap.client.get_entitlements();
                            let fresh_cv = snap.client.get_client_version();
                            {
                                let mut svc = services.write().await;
                                svc.client_version = fresh_cv;
                                if let Some(ref e) = fresh_entitlements {
                                    svc.puuid = e.subject.clone();
                                }
                            }
                            snap.logger.log(
                                "Re-authentication successful after deferred retry, resuming...",
                            );
                            break;
                        }
                        Err(_) => continue,
                    }
                }
                true
            }
        };

        // Reconnect WS with fresh puuid to restore 10s cooldown cadence
        // instead of falling back to 1s polling permanently.
        if ws.is_none() {
            let (port, password, puuid) = {
                let port = *self.lockfile_port.lock().unwrap_or_else(|e| e.into_inner());
                let password = snap.client.get_local_password();
                let puuid = {
                    let svc = services.read().await;
                    svc.puuid.clone()
                };
                (port, password, puuid)
            };
            if let Some(port) = port {
                match ValorantWs::connect(
                    port,
                    password.expose_secret(),
                    &puuid,
                    snap.logger.clone(),
                )
                .await
                {
                    Some(w) => {
                        *ws = Some(w);
                        snap.logger.log("WS reconnected after re-auth");
                        let _ = app.emit("rank_icons", snap.content.rank_icons.as_ref().clone());
                    }
                    None => {
                        snap.logger.log("WS reconnect unavailable — using polling");
                    }
                }
            }
        }

        cleared
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
            let (state, log_msg) = snap
                .presences
                .detect_game_state_from_poll(entitlements, cv, puuid)
                .await;
            if let Some(msg) = log_msg {
                snap.logger.log(&msg);
            }
            return (state, None);
        }

        if let Some(ws_inner) = ws.as_mut() {
            tokio::select! {
                result = ws_inner.recv() => {
                    match result {
                        Some(event) => {
                            // Collapse any immediately-available backlog to the latest
                            // event, so a burst of queued presences converges to "now"
                            // instead of being replayed one stale tick at a time.
                            let mut coalesced = 0u32;
                            let mut latest = event;
                            while let Ok(ev) = ws_inner.try_recv() {
                                latest = ev;
                                coalesced += 1;
                            }
                            if coalesced > 0 {
                                snap.logger.log(&format!("WS backlog coalesced {coalesced} presences"));
                            }
                            snap.logger.log(&format!("WS state push: {:?}", latest.state));
                            (Some(latest.state), Some(latest.presences))
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
                _ = tokio::time::sleep(Duration::from_secs(self.services.read().await.config.get().cooldown)) => {
                    (last_state, None)
                }
            }
        } else {
            // WS unavailable: poll at 1s interval
            let (state, log_msg) = snap
                .presences
                .detect_game_state_from_poll(entitlements, cv, puuid)
                .await;
            if let Some(msg) = log_msg {
                snap.logger.log(&msg);
            }
            (state, None)
        }
    }
}

/// Two heartbeats are "the same" for emission purposes if nothing the user
/// would see has changed. `time` always differs between builds by
/// construction (it's `SystemTime::now()`), `version` is assigned at
/// emission time (not yet meaningful at comparison time), and
/// `time_diff` (in `already_played_with`) is `now - epoch` recomputed fresh
/// every tick — all three are normalized to zero for comparison only.
fn heartbeats_equal_ignoring_time(a: &HeartbeatPayload, b: &HeartbeatPayload) -> bool {
    let normalize = |h: &HeartbeatPayload| {
        let mut h = h.clone();
        h.time = 0;
        h.version = 0;
        for entry in h.already_played_with.iter_mut() {
            entry.time_diff = 0.0;
        }
        h
    };
    normalize(a) == normalize(b)
}

/// Returns the names of the top-level fields that differ between two
/// normalized heartbeats. Used only for debug logging to discover which
/// field is churning and preventing dedup.
fn diff_heartbeat_fields(a: &HeartbeatPayload, b: &HeartbeatPayload) -> Vec<&'static str> {
    let normalize = |h: &HeartbeatPayload| {
        let mut h = h.clone();
        h.time = 0;
        h.version = 0;
        for entry in h.already_played_with.iter_mut() {
            entry.time_diff = 0.0;
        }
        h
    };
    let a = normalize(a);
    let b = normalize(b);
    let mut diffs = Vec::new();
    if a.state != b.state {
        diffs.push("state");
    }
    if a.r#type != b.r#type {
        diffs.push("type");
    }
    if a.mode != b.mode {
        diffs.push("mode");
    }
    if a.puuid != b.puuid {
        diffs.push("puuid");
    }
    if a.map != b.map {
        diffs.push("map");
    }
    if a.server != b.server {
        diffs.push("server");
    }
    if a.players != b.players {
        diffs.push("players");
    }
    if !Arc::ptr_eq(&a.rank_icons, &b.rank_icons) || a.rank_icons.len() != b.rank_icons.len() {
        diffs.push("rank_icons");
    }
    if a.session_id != b.session_id {
        diffs.push("session_id");
    }
    if a.already_played_with != b.already_played_with {
        diffs.push("already_played_with");
    }
    diffs
}
