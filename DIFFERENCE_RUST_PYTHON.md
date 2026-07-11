# Feature Parity Diff: Rust/Tauri vs Python

**Last updated:** 2026-07-12  
**Python repo:** `VALORANT-rank-yoinker` (38 files, ~5391 lines)  
**Rust repo:** `VRY_rewrite_tauri_fixed/tauri_rewrite` (31 .rs files + JS/CSS, ~5450 lines total)  
**Python version:** 2.99  
**Rust version:** 3.0.0

---

## Legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Implemented correctly / identical behavior |
| ⚠️ | Implemented but differs from Python behavior |
| ❌ | Not implemented |
| ➕ | Rust has this; Python does not |

---

## 1. Core Architecture

| Feature | Python | Rust/Tauri | Status |
|---------|--------|-----------|--------|
| Process model | Standalone console app (main.py) + optional pywebview GUI | Tauri app (Rust backend + embedded Chromium WebView2) | Architectural |
| IPC | HTTP+WebSocket server (websocket_server lib, port 1100) | Tauri event system (`app.emit("heartbeat")` / `listen`) | ✅ |
| State machine | Main loop in main.py (1357 lines) polling every `cooldown` seconds | `state_machine.rs` `run_main_loop()` (286 lines), same polling | ✅ |
| Config format | `config.json` with `DEFAULT_CONFIG` dict, `@apply_defaults` decorator | `config.json` with `AppConfig` struct + serde defaults | ✅ |
| Rate limiting | Token-bucket via deque: Pd=8/s, Glz=5/s, Local=20/s, Custom=5/s | Rolling window via VecDeque: same limits | ✅ |
| Error handling | `Error` class with `PortError`/`LockfileError` typed exceptions | `ApiError` enum with `Http`/`RateLimited`/`BadClaims`/`NotFound`/`ServerError`/`Lockfile`/`Auth` variants | ✅ |
| Logging | `logs/log-N.txt`, timestamped entries, `Logging` class | `logging.rs` `Logger` struct, same format, in-memory buffer (500 entries) + `get_tail()` | ✅ |
| Runtime dependencies | 10+ Python packages (requests, websocket_server, pypresence, etc.) | 13 Rust crates (tauri, reqwest, tokio, serde, etc.) | Architectural |
| Bundle size | ~30MB (cx_Freeze) | ~12MB (Rust binary, WebView2 provided by OS) | Architectural |
| Python version | 3.9+ required | Not applicable (standalone .exe) | Architectural |
| Webview dependency | Optional (pywebview) | Required (WebView2 runtime) | Architectural |
| Pre-fetch static data | Fetches agents, maps, weapons, skins, sprays, flex, buddies, titles, cards, competitive tiers, seasons at startup | Same, via `fetch_all_content()` in `api/content.rs` | ✅ |
| Async runtime | Synchronous (`requests` library, blocking calls) | Async (`tokio`, `reqwest` async) | ✅ |
| Release optimization | N/A (interpreted) | **`opt-level = 0`** — all Rust compiler optimizations disabled! | ⚠️ Should be `opt-level = 2` |

---

## 2. Game State Detection

| Feature | Python | Rust/Tauri | Status |
|---------|--------|-----------|--------|
| Local API presences | `GET /chat/v4/presences` (polling) | Same, via `PresenceService::get_presences()` | ✅ |
| WebSocket presences | `Ws` class connects to `wss://127.0.0.1:port`, subscribes to presences + chat, handles real-time updates | `ValorantWs` struct exists but **not wired into main loop** — Rust uses polling only | ❌ No WS integration |
| Deceive detection | `is_deceive_running()` checks tasklist for Deceive.exe, falls back to GLZ API presences | Not implemented | ❌ |
| Presence decoding | Base64-decodes `private` field, handles both nested (`matchPresenceData.sessionLoopState`) and flat (`sessionLoopState`) | Same via `PresenceService::decode_private_presence()` | ✅ |
| State extraction | `extract_game_state()` from presence private data | Same via `PresenceService::extract_game_state()` | ✅ |
| Queue ID extraction | Extracts `queueId` from nested `matchPresenceData` or flat | Same via `PresenceService::extract_queue_id()` | ✅ |
| Party detection | `Menu.get_party_json()` / `get_party_members()` from presences' partyId | Same via `PresenceService::extract_party_id()` / `find_party_member_puuids()` | ✅ |
| Account level from presence | `extract_account_level()` from nested or flat | Same via `PresenceService::extract_account_level()` | ✅ |
| State transition: INGAME→other | Queues match result with retries (`queue_match_result_update` + `process_pending_match_results`) | Updates match result directly (`update_match_result` on state transition) | ⚠️ No retry queue |
| State transition: MENUS | Invalidates caches | Same: `rank.invalidate_cache()` + `stats.clear_cache()` | ✅ |
| Reconnect loop | On DISCONNECTED: re-reads lockfile, waits for presence, refreshes headers, creates new Ws | On main loop error: re-enters `try_initialize()` which re-reads lockfile + re-auths | ✅ |
| State transition: PREGAME→INGAME | Reuses cached rank+stats via `get_or_fetch_rank_and_stats()` | Re-fetches everything (no match player cache) | ❌ Extra API calls |

---

## 3. Loadout / Skin Resolution

| Feature | Python | Rust/Tauri | Status |
|---------|--------|-----------|--------|
| API source | valorant-api.com + Riot GLZ | valorant-api.com + Riot GLZ | ✅ |
| Socket UUIDs | `sockets` dict with 5 named UUIDs: skin, skin_level, skin_chroma, skin_buddy, **skin_buddy_level** | Hardcoded UUIDs: `bcef87d6`, `e7c63390`, `3ad1b2b2`, `77258665` — **no buddy_level socket** | ⚠️ Missing skin_buddy_level |
| Skin display name | Raw `displayName` from API | Same | ✅ |
| Chroma resolution | Iterates weapon skin chromas, finds matching UUID | Same | ✅ |
| Icon fallback chain | chroma displayIcon → chroma fullRender → skin displayIcon → level 0 displayIcon | Same chain (chroma.display_icon → chroma.full_render → skin.display_icon → level[0].display_icon → weapon.display_icon) | ✅ |
| Buddy resolution | Matches buddy UUID from valo-api buddies data | Same via `content.buddies` | ✅ |
| Skin tier color | Resolves from `contentTierUuid` via `tierDict`, sent to frontend as RGB | Not sent to frontend | ❌ |
| Weapon preview list | Tracks selected weapon's skin display name (`weapon_lists`) | Same (`weapon_lists` HashMap) | ✅ |
| PlayerCard/Title name | Resolved from `displayName` of player cards / titles | Same via `player_card_name` / `title_name` | ✅ |
| Expression/spray resolution | Matches `AssetID` against sprays and flex APIs | Same, combines spray + flex in `content.sprays` / `content.flex` | ✅ |
| `fullTransparentIcon` fallback | Falls back to `displayIcon` when `fullTransparentIcon` is None | Same: `full_transparent_icon.or_else(display_icon)` | ✅ |

---

## 4. Rank / Competitive

| Feature | Python | Rust/Tauri | Status |
|---------|--------|-----------|--------|
| MMR fetch | `GET /mmr/v1/players/{puuid}` from PD | Same, `RankService::fetch_rank()` | ✅ |
| Rank extraction | `CompetitiveTier`, `RankedRating`, `LeaderboardRank` from current season | Same via `SeasonalInfo.competitive_tier` / `.ranked_rating` / `.leaderboard_rank` | ✅ |
| Peak rank source | WinsByTier keys only (ignores `PeakRank` from API) | WinsByTier only (same) | ✅ |
| Peak rank adjustment | +3 for pre-Ascendant seasons (tiers > 20) | Same `is_before_ascendant()` check + +3 | ✅ |
| Pre-Ascendant season list | 17 UUIDs in `before_ascendant_seasons` (constants.py) | 17 UUIDs in `BEFORE_ASCENDANT_SEASONS` (api/content.rs) | ✅ |
| Peak rank act/episode | `Content.get_act_episode_from_act_id()` | Same via `ContentCache::get_act_episode_from_act_id()` | ✅ |
| Season name parsing | `has_letter_and_number()`, `roman_to_int()`, `parse_season_number()` | Same helper functions | ✅ |
| Previous season rank | `get_previous_season_id()` finds previous act by EndTime==StartTime | Same logic in `fetch_seasons()` | ✅ |
| Win rate | `wins / total_games * 100` | Same: `(wins as f64 / games as f64 * 100.0) as u32` | ✅ |
| Rank caching | `requestMap` dict with TTL per session | `Mutex<HashMap<String, (PlayerRank, Instant)>>` with 300s TTL | ✅ |
| Cache invalidation | `invalidate_cached_responses()` on MENUS | `invalidate_cache()` on MENUS | ✅ |
| Previous rank via separate fetch | Calls `get_rank(puuid, previousSeasonID)` — separate HTTP call | Same pattern via `get_previous_rank()` | ⚠️ Both make redundant 2nd call; could be extracted from same MMR response |

---

## 5. Stats (HS%, KD, RR Earned, Last Active)

| Feature | Python | Rust/Tauri | Status |
|---------|--------|-----------|--------|
| Competitive updates | `GET /mmr/v1/players/{puuid}/competitiveupdates?startIndex=0&endIndex=1&queue=competitive` | Same endpoint | ✅ |
| Match details | `GET /match-details/v1/matches/{match_id}` from **Pd** | Same, from **Pd** (`UrlType::Pd`) | ✅ |
| Match details error handling | Returns `{}` on failure, continues with summary-only stats | Returns `None`, `process_match_data` handles `Option` gracefully | ✅ |
| KD calculation | kills / deaths, 2 decimal places, handles 0 deaths | Same: `format!("{:.2}", k / d)`, handles 0 deaths | ✅ |
| HS% calculation | round(headshots / (legshots+bodyshots+headshots) * 100) | Same integer rounding | ✅ |
| RR earned | `match_summary.RankedRatingEarned` | Same from `CompetitiveUpdate.ranked_rating_earned` | ✅ |
| AFK penalty | `match_summary.AFKPenalty` | Same from `CompetitiveUpdate.afk_penalty` | ✅ |
| Last active epoch | `(MatchStartTime + gameLengthMillis) / 1000` — match END time | Same | ✅ |
| `gameLengthMillis` fallback | From match_details `matchInfo.gameLengthMillis`, defaults to 0 | Same via `match_info.game_length_millis.unwrap_or(0)` | ✅ |
| `MatchStartTime` fallback | `match_summary.MatchStartTime OR match_info.gameStartMillis` | Same: `summary.match_start_time OR match_info.game_start_millis` | ✅ |
| Match details cache | `match_details_cache` dict per session | `Mutex<HashMap<String, MatchDetailsResponse>>` | ✅ |
| Stats cache clearing | `clear_runtime_cache()` on MENUS | `clear_cache()` on MENUS | ✅ |
| **Conditional fetch** | `_should_fetch_comp_stats()` checks table flags — **skips 2 API calls if all stat columns hidden** | **Always fetches** — ignores config flags | ❌ Extra API calls |
| Match result tracking | `Stats.update_match_result()` updates stored encounter entries | Same via `EncounterService::update_match_result()` | ✅ |
| Match result retry queue | `queue_match_result_update` + `process_pending_match_results` (retries up to 5 times) | Not implemented (updated directly on INGAME→other transition, single attempt) | ❌ Less robust |
| Match player cache | `get_or_fetch_rank_and_stats()` caches per match_id for PREGAME→INGAME reuse | Not implemented | ❌ Extra API calls |

---

## 6. Encounter Tracking

| Feature | Python | Rust/Tauri | Status |
|---------|--------|-----------|--------|
| Storage location | `%APPDATA%/vry/stats.json` | `root/stats/encounters.json` | ⚠️ Different path |
| Data model | Per-puuid array with name, agent, map, rank, rr, match_id, epoch, relation, team, my_team, result, score | Same fields in `EncounterRecord` | ✅ |
| Deduplication | Merge by match_id (preserves existing result/score) | Same: find by match_id, merge non-null fields | ✅ |
| Match result update | Queue-based: `queue_match_result_update` with retries + max attempts | Direct `update_match_result` on state transition | ❌ No retry mechanism |
| Encounter summaries | `build_encounter_summary()` returns ally/enemy W/L counts + latest encounter | Same in `EncounterService::build_encounter_summary()` | ✅ |
| All summaries (MENUS) | N/A (not used in Python frontend) | `get_all_summaries()` for MENUS state — `alreadyPlayedWith` in heartbeat | ➕ Rust-only feature |
| Match result details | Checks multiple key casing variants, falls back to comparing scores | Handles `matchInfo.winningTeam` + `teams[].won` bool + multiple casing variants | ✅ |
| Score extraction | From teams array's roundsWon fields | Same with multiple casing variants | ✅ |
| `get_all_summaries` sorting | N/A | Sorts by most recent first | ➕ |
| File I/O strategy | Writes on every `save_data()` call | Writes on every `save_encounter()` + `update_match_result()` call | ⚠️ Both write synchronously |

---

## 7. Name Resolution

| Feature | Python | Rust/Tauri | Status |
|---------|--------|-----------|--------|
| Local API | POST `/player-account/lookup/v2/namesets-for-puuids` with `{"puuids": [...]}` | Same via `UrlType::Local` | ✅ |
| PD fallback | PUT `/name-service/v2/players` with PUUID array | Same via `UrlType::Pd`, `fetch_put_json_with_body` | ✅ |
| Name format | `"GameName#TagLine"` | Same | ✅ |
| Key casing handling | Tries `gameName`, `game_name`, `GameName` for local API; `Subject`, `GameName`, `TagLine` for PD | Same multi-casing fallback (serde alias + manual `.or_else()`) | ✅ |
| Error handling | Returns empty string on failure | Returns `ApiError` on failure, caller unwraps with default | ⚠️ Different error propagation |
| **Name caching** | **No cache** (fetches every heartbeat) | **No cache** (same) | ⚠️ Both re-fetch every state transition |

---

## 8. Frontend (Web GUI)

| Feature | Python | Rust/Tauri | Status |
|---------|--------|-----------|--------|
| Connection method | WebSocket to `ws://host:port/` | Tauri events (`heartbeat`, `state_change`, `backend_ready`) | Architectural |
| HTML/CSS files | `docs/vry_gui.html` + `docs/vry_gui.css` + `docs/matchLoadouts.css` | Same files (copied into Tauri `public/`) | ✅ |
| JS engine | `docs/vry_gui.js` (~1295 lines) | Same `vry_gui.js` (~1015 lines) | ⚠️ Rust frontend more compact |
| Element IDs | 40+ IDs | 44 IDs (matches Python + extras) | ✅ |
| Weapon columns | 4-column layout | Same | ✅ |
| Preview weapons | Vandal, Phantom, Melee | Same | ✅ |
| Rank names/colors | Same arrays | Same | ✅ |
| Stat bar | 8 chips: Win Rate, Rank, RR, Leaderboard, Peak Rank, Last Act, Level, Last Active | Same 8 chips | ✅ |
| Expression wheel | 4-slot radial layout | Same | ✅ |
| Player card preview | clip-path polygon shape | Same | ✅ |
| Screenshot | html2canvas → clipboard | Same | ✅ |
| Right-click copy | on playerCardPreview, selectedName, selectedCardTitle | Same | ✅ |
| Native tooltips | `title` attribute | Same | ✅ |
| Loading overlay | `#loadingOverlay` with log tail polling | Same via IPC `get_gui_log_tail` | ✅ |
| Refresh/Restart | `restart_application` action via WebSocket | `tauriInvoke("restart_application")` via `commands::system::restart_application` | ✅ |
| Player sorting | Self → party → team → rank desc → name | Same | ✅ |
| `user-select` | Removed `user-select: text` on `#selectedName` only | Added `* { user-select: none; }` globally | ⚠️ More aggressive |
| Version info on connect | Server sends `{"type": "version", "core": version}` | Not sent (frontend can call `get_version` command) | ⚠️ Different mechanism |

---

## 9. Config Flags Comparison

| Config Flag | Python default | Rust default | Status |
|-------------|---------------|--------------|--------|
| `cooldown` | 10 | 10 | ✅ |
| `port` | 1100 | 1100 | ✅ |
| `weapon` | Vandal | Vandal | ✅ |
| `chat_limit` | 5 | 5 | ✅ |
| `table.skin` | true | true | ✅ |
| `table.rr` | true | true | ✅ |
| `table.earned_rr` | false | false | ✅ |
| `table.peakrank` | true | true | ✅ |
| `table.previousrank` | false | false | ✅ |
| `table.leaderboard` | true | true | ✅ |
| `table.headshot_percent` | false | false | ✅ |
| `table.winrate` | true | true | ✅ |
| `table.kd` | false | false | ✅ |
| `table.level` | true | true | ✅ |
| `table.last_active` | true | true | ✅ |
| `flags.discord_rpc` | false | false | ✅ |
| `flags.aggregate_rank_rr` | true | true | ✅ |
| `flags.peak_rank_act` | true | true | ✅ |
| `flags.auto_hide_leaderboard` | true | true | ✅ |
| `flags.game_chat` | false | false | ✅ |
| `flags.last_played` | true | true | ✅ |
| `flags.pre_cls` | false | false | ✅ |
| `flags.server_id` | false | false | ✅ |
| `flags.short_ranks` | false | false | ✅ |
| `flags.truncate_skins` | true | true | ✅ |
| `flags.truncate_names` | false | false | ✅ |
| `flags.starting_side` | true | true | ✅ |

> **Note:** While all config flags have identical names and defaults, **8 flags are stored but never acted upon** in the Rust backend or frontend. See Section 9a below.

---

## 9a. Config Flag Implementation Gaps

| Flag | Python Uses It For | Rust Implements It? | Impact |
|------|--------------------|---------------------|--------|
| `short_ranks` | Switches `NUMBERTORANKS`/`SHORT_NUMBERTORANKS` in `main.py:393-396` | ❌ Stored, never referenced | Low — frontend could apply its own short names |
| `aggregate_rank_rr` | Appends RR to rank string `main.py:747-750` | ❌ Stored, never referenced | Low — frontend displays both fields separately |
| `peak_rank_act` | Toggles peak rank act display `main.py:764-765` | ❌ Stored, never referenced | Low — `peak_rank_act` always sent in payload |
| `last_played` | Prints encounter summary to console `main.py:1323-1327` | ❌ Not applicable (GUI-only) | N/A |
| `auto_hide_leaderboard` | Hides LB column when no leaderboard rank `main.py:1292-1295` | ❌ Stored, never referenced | Low — frontend always shows column |
| `truncate_skins` | Cuts long skin names for table display | ❌ Stored, never referenced | Low — frontend shows full names |
| `truncate_names` | Cuts long player names for table display | ❌ Stored, never referenced | Low — frontend shows full names |
| `starting_side` | Shows DEF/ATK indicator in title `main.py:1285-1287` | ❌ Stored, never referenced | Low — not displayed |

---

## 10. Missing Python Features in Rust

### Backend Features

| Feature | Python File | Lines | Rust Status | Impact |
|---------|-----------|-------|-------------|--------|
| Account management (switch/add/remove accounts) | `account_manager/account_manager.py`, `account_config.py`, `account_auth.py` | 593 | ❌ | Users must sign in manually via Riot client |
| Discord Rich Presence | `src/rpc.py` | 378 | ❌ | No Discord status integration |
| Interactive config wizard | `src/configurator.py` | 75 | ❌ | Config only editable via JSON file or Tauri commands |
| In-game chat display | `src/websocket.py` `handle()` chat messages | 161 | ❌ | No chat overlay |
| ANSI terminal table | `src/table.py` | 227 | ❌ | Tauri is GUI-only |
| Version update check | `requestsV.py:check_version()` | ~40 | ❌ | Users must manually update |
| Deceive detection | `requestsV.py:is_deceive_running()` + GLZ fallback | ~10 | ❌ | Deceive users may have issues |
| `matchLoadout` standalone payload | `main.py` (multiple places) | ~30 | ❌ | Only heartbeat payload type |
| `hide_names` / `hide_levels` | `constants.py` + main.py logic | ~10 | ❌ | Names always shown |
| Installation auto-update | `requestsV.py:copy_run_update_script()` | ~30 | ❌ | No auto-update mechanism |
| WebSocket presence integration | `src/websocket.py` `Ws` class (realtime) | 161 | ❌ | Uses polling only (2s loop) |
| **Match player cache (PREGAME→INGAME)** | `main.py:239-263` | 25 | ❌ | ~15 extra API calls per transition |
| **Conditional stats fetching** | `player_stats.py:28-32` | 5 | ❌ | 2 extra API calls per player when stats hidden |

### Frontend Features

| Feature | Python Frontend | Rust Frontend | Status |
|---------|----------------|---------------|--------|
| `matchLoadouts.html` standalone viewer | Has its own HTML+JS | ❌ | Only `vry_gui.html` |
| Skin RGB/tier color display | Resolves contentTierUuid → color | ❌ | Not sent in payload |
| `short_ranks` rendering | Abbreviates rank names | ❌ | Not implemented despite config flag |

---

## 11. Behavioral Differences

| Behavior | Python | Rust/Tauri | Details |
|----------|--------|-----------|---------|
| Party number MENUS | partyNumber=1 for self, 0 for others | partyNumber=1 for ALL players | ⚠️ Minor (frontend ignores 0 anyway) |
| Party number INGAME | partyNumber=1 for self+party, 0 for others | partyNumber=0 for ALL players | ⚠️ Minor |
| **Conditional stats fetching** | Skips 2 API calls per player if stat columns hidden | **Always fetches** | ⚠️ **Extra API calls** |
| **Match player cache** | Caches rank+stats per match_id (PREGAME→INGAME reuse) | **Always re-fetches** | ⚠️ **Extra API calls** (~15 per transition) |
| Match result update reliability | Queue-based with retries (handles transient failures) | Direct on state transition (no retry) | ⚠️ Less robust |
| Deceive-compatible presence | Falls back to GLZ HTTP presences if Deceive detected | No Deceive detection | ⚠️ Deceive users see DISCONNECTED |
| WebSocket presences | Real-time via persistent WebSocket | Polled every 2s via `/chat/v4/presences` HTTP | ⚠️ Slower state detection |
| Config wizard | Interactive CLI (InquirerPy) | JSON file + Tauri commands | ⚠️ Different UX |
| Client version parsing | From Riot client process query | From ShooterGame.log (`CI server version:` line) | ⚠️ Different source |
| Short rank names | Uses `SHORT_NUMBERTORANKS` array | Config flag exists but unused | ⚠️ Inactive config |
| Skin content tier color | Sent as RGB in payload | Not sent | ⚠️ Missing frontend feature |
| `matchLoadout` payload | Separate payload type with detailed per-weapon data | Only heartbeat payload (weapons inside player entries) | ⚠️ Different payload structure |
| **`version` field in heartbeat** | Not sent | `version: u64` (monotonic counter) | ➕ Rust-only |
| **`earnedRR` field in heartbeat** | Not sent | `earned_rr: Option<String>` (INGAME self only) | ➕ Rust-only |
| Python `2.x.x` version scheme | `version = "2.99"` | `"3.0.0"` | ⚠️ Version bump indicates rewrite |
| Agent image URL | From valapi displayIcon | Constructed URL `https://media.valorant-api.com/agents/{cid}/displayicon.png` | ⚠️ Different URL but same result |
| Release build optimization | N/A (interpreted) | **`opt-level = 0`** — all compiler optimizations disabled | ⚠️ **Major perf issue** — should be `opt-level = 2` |
| RwLock scope in main loop | No equivalent (synchronous) | RwLock held across entire heartbeat assembly (20+ HTTP calls) | ⚠️ **UI blocking** during long heartbeats |

---

## 12. Lines of Code Comparison

| Category | Python | Rust |
|----------|--------|------|
| API client | 308 (requestsV.py) | 303 (api/client.rs) + 147 (api/auth.rs) |
| State machine / main loop | 1357 (main.py) | 286 (core/state_machine.rs) + 776 (core/payload_builder.rs) |
| Service layer | 155 player_stats.py + 147 rank.py + 42 names.py + 118 presences.py + 232 stats.py + 295 Loadouts.py | 188 stats.rs + 172 rank.rs + 115 names.rs + 183 presences.rs + 287 loadouts.rs + 311 encounters.rs + 203 config.rs + 93 logging.rs |
| Data models | ~200 (inline in various files) | 211 mmr.rs + 176 loadout.rs + 307 content.rs + 59 auth.rs + 49 presences.rs + 36 match_data.rs + 95 heartbeat.rs |
| Frontend JS | 1295 (vry_gui.js) | 1015 (vry_gui.js) |
| Frontend HTML/CSS | ~800 | ~800 |
| **Total** | **~5391** | **~3800 (Rust) + ~1815 (JS/CSS) = ~5615** |

---

## 13. Files Present in Python but Absent in Rust

| File | Lines | Purpose |
|------|-------|---------|
| `account_manager/account_manager.py` | 229 | Account switching UI |
| `account_manager/account_config.py` | 187 | Account persistence |
| `account_manager/account_auth.py` | 177 | Riot authentication for account switching |
| `custom/gui/desktop_app.py` | 401 | pywebview desktop launcher |
| `custom/gui/gui_logs.py` | 65 | GUI-specific logging |
| `custom/gui/webui.py` | 28 | Web UI link printer |
| `src/colors.py` | 221 | Terminal color utilities |
| `src/configurator.py` | 75 | Interactive config wizard |
| `src/rpc.py` | 378 | Discord Rich Presence |
| `src/table.py` | 227 | Terminal table rendering |
| `src/websocket.py` | 161 | WebSocket chat + presence (real-time) |
| `src/server.py` | 60 | WebSocket server for frontend IPC |
| `src/questions.py` | 93 | InquirerPy questions for config |
| `scripts/instalock.py` | 70 | Agent instalocker tool |
| `scripts/api_dumper.py` | 229 | API debug dumper |
| `setup.py` | 34 | cx_Freeze build config |

---

## 14. Performance & Build Profile

| Metric | Python | Rust |
|--------|--------|------|
| Binary size | ~30MB (cx_Freeze) | ~12MB (WebView2 provided by OS) |
| Release optimization | N/A (interpreted) | **`opt-level = 0`** — should be `2` or `"s"` |
| Async I/O | No (synchronous `requests`) | Yes (tokio + reqwest) |
| HTTP concurrency | Sequential per-player | **Currently sequential per-player** (should be parallel with `join_all`) |
| Static content fetch | Sequential with 200ms delays | Sequential with same delays (should be parallel) |
| Lock contention | N/A (single-threaded) | RwLock held across 20+ HTTP calls — blocks write commands |
| Memory — rank cache | Plain dict per session (cleared on MENUS) | `Mutex<HashMap>` with 300s TTL — unbounded growth |
| Memory — match details cache | Dict per session | `Mutex<HashMap>` — **unbounded growth** (memory leak risk) |
| Memory — encounters | Dict per session, persisted | `Mutex<HashMap>` persisted — unbounded growth |

---

## 15. Payload Field Differences

| Field | Python sends? | Rust sends? | Notes |
|-------|-------------|-------------|-------|
| `type` | Implicit (WebSocket message type) | Explicit `"type": "heartbeat"` | Different IPC mechanism |
| `version` (monotonic counter) | ❌ | ✅ | Rust-only heartbeat versioning |
| `earnedRR` | ❌ | ✅ (INGAME self) | Not in Python payload |
| `agentImgLink` source | valapi `displayIcon` | Constructed UUID URL | Visually identical |

---

## 16. Summary

- ✅ **Matched features**: Loadout/skin resolution, rank/peak rank, stats (KD, HS%, RR, lastActive), names, encounter tracking, web GUI layout, right-click copy, native tooltips, player sorting, config flags, rate limiting.
- ⚠️ **Minor behavioral diffs**: No buddy_level socket UUID, party number always 1 in MENUS, no Deceive detection, polling-based (not WS) presences, no retry queue for match results, missing short_ranks rendering, no skin tier color.
- ⚠️ **Performance regressions**: `opt-level = 0` in release profile, RwLock held across main loop, per-player sequential HTTP calls, no names cache, no match player cache, no conditional stats fetching.
- ➕ **Rust-only features**: `version` monotonic heartbeat counter, `earnedRR` field, `get_all_summaries()` for MENUS, `get_heartbeat_log` command.
- ❌ **Missing backend**: Account management (593 lines), Discord RPC (378), config wizard (75), chat display (161), table rendering (227), WebSocket presences (161), version checker, Deceive detection, `hide_names`/`hide_levels`, `matchLoadout` payload type, auto-update.
- ❌ **Missing frontend**: Standalone matchLoadouts viewer, skin tier color display, version info on connect.
- **Key fixes applied**: Match-details endpoint URL type (same as Python — no fix needed), `last_active` epoch calculation (added `gameLengthMillis`), encounter match result fallback for winning team, icon fallback for sprays/flex, `PlayerCardName`/`TitleName` fields.
