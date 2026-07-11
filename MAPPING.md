# Code Mapping: Python → Rust/Tauri

**Python repo root:** `VALORANT-rank-yoinker/`  
**Rust repo root:** `VRY_rewrite_tauri_fixed/tauri_rewrite/`  
**Last verified:** 2026-07-12  
**Python version:** 2.99 (38 files, ~5391 lines)  
**Rust version:** 3.0.0 (31 .rs files + JS/CSS, ~5450 lines total)

---

## 1. File-Level Mapping

### Backend

| Python File | Lines | Rust File(s) | Rust Lines | Coverage |
|-------------|-------|--------------|------------|----------|
| `main.py` | 1357 | `core/state_machine.rs` + `core/payload_builder.rs` | 286 + 776 | State machine + heartbeat building |
| `main.py` (main loop) | 387–1339 | `core/state_machine.rs:157–315` | 158 | Main polling loop |
| `main.py` (INGAME handler) | 544–867 | `core/payload_builder.rs:180–391` | 211 | INGAME heartbeat assembly |
| `main.py` (PREGAME handler) | 868–1123 | `core/payload_builder.rs:393–632` | 239 | PREGAME heartbeat assembly |
| `main.py` (MENUS handler) | 1125–1268 | `core/payload_builder.rs:634–757` | 123 | MENUS heartbeat assembly |
| `main.py` (helpers) | 52–103 | `core/payload_builder.rs:759–776` | 17 | `format_last_active()` + `parse_server()` |
| `main.py` (match result queue) | 265–362 | `core/state_machine.rs:222–264` | 42 | Match result processing (Rust: no retry queue) |
| `main.py` (match player cache) | 208–263 | **Not implemented** | 0 | ⚠️ Match-local rank/stats cache (PREGAME→INGAME reuse) |
| `src/Loadouts.py` | 295 | `services/loadouts.rs` | 287 | Skin/loadout/spray resolution |
| `src/rank.py` | 147 | `services/rank.rs` | 172 | MMR, peak rank, win rate |
| `src/content.py` | 163 | `api/content.rs` + `models/content.rs` | 275 + 307 | Content cache + season parsing helpers |
| `src/constants.py` | 259 | Scattered across multiple files | — | Constants split by domain |
| `src/server.py` | 60 | `commands/config.rs` + `commands/system.rs` + `lib.rs` | 96 | IPC (Tauri events vs WebSocket) |
| `src/presences.py` | 118 | `services/presences.rs` | 183 | Presence tracking + party detection |
| `src/names.py` | 42 | `services/names.rs` | 115 | Name resolution |
| `src/player_stats.py` | 155 | `services/stats.rs` | 188 | HS%, KD, RR earned, last active |
| `src/stats.py` | 232 | `services/encounters.rs` | 311 | Encounter tracking |
| `src/requestsV.py` | 308 | `api/client.rs` + `api/auth.rs` | 303 + 147 | API client + auth |
| `src/websocket.py` | 161 | `api/websocket.rs` (exists but **not wired**) | 142 | WebSocket (presence/chat — Rust uses polling only) |
| `src/config.py` | 78 | `services/config.rs` | 203 | Config management |
| `src/logs.py` | 43 | `services/logging.rs` | 93 | Log file management |
| `src/errors.py` | 38 | `api/client.rs:ApiError` | (inline) | Error types |
| `src/states/menu.py` | 108 | `services/presences.rs` (party methods) | (inline) | Party logic |
| `src/states/coregame.py` | 58 | `core/payload_builder.rs` (inline) | (inline) | Inline in builder |
| `src/states/pregame.py` | 42 | `core/payload_builder.rs` (inline) | (inline) | Inline in builder |

### Commands (Rust-only, Tauri IPC handlers)

| Rust File | Lines | Purpose | Python Equivalent |
|-----------|-------|---------|-------------------|
| `commands/config.rs` | 47 | `get_config`, `set_config`, `get_gui_log_tail`, `get_heartbeat_log` | `server.py` `send_payload()` + `config.py` |
| `commands/system.rs` | 41 | `get_version`, `restart_application`, `get_status` | `main.py` `Requests.get_headers(refresh=True)` |

### Not Ported (Console-Only or Unimplemented)

| Python File | Lines | Reason |
|-------------|-------|--------|
| `src/colors.py` | 221 | ANSI terminal color – not needed for GUI |
| `src/table.py` | 227 | Rich console table – Tauri is GUI-only |
| `src/rpc.py` | 378 | Discord RPC – not implemented |
| `src/configurator.py` | 75 | Interactive config wizard – not implemented |
| `src/questions.py` | 93 | Config wizard questions – not needed |
| `src/account_manager/*` | 593 | Account management – not implemented |
| `src/os_info.py` | 11 | OS detection – not needed |

### Frontend

| Python File | Rust File | Notes |
|-------------|-----------|-------|
| `docs/vry_gui.html` | `frontend/index.html` | Nearly identical HTML |
| `docs/vry_gui.js` | `frontend/vry_gui.js` | Tauri events replace WebSocket; also more compact |
| `docs/vry_gui.css` | `frontend/vry_gui.css` | Nearly identical CSS |
| `docs/matchLoadouts.css` | `frontend/style.css` | Identical CSS (base styles) |
| `docs/html2canvas.min.js` | `frontend/html2canvas.min.js` | Same library file |
| `docs/matchLoadouts.html` | Not ported | Standalone viewer |
| `docs/matchLoadouts.js` | Not ported | Standalone viewer |
| `docs/app.js` | Not ported | Release download counter |

---

## 2. Function-Level Mapping

### 2.1 Peak Rank Algorithm

| Step | Python (`src/rank.py:77–94`) | Rust (`services/rank.rs:93–129`) |
|------|------|------|
| Get seasons dict | `r["QueueSkills"]["competitive"]["SeasonalInfoBySeasonID"]` | `mmr.queue_skills.competitive.seasonal_info_by_season_id` |
| Iterate seasons | `for season in seasons:` | `for (sid, sinfo) in seasons` |
| Skip null WinsByTier | `if seasons[season]["WinsByTier"] is not None:` | `if let Some(ref wbt) = sinfo.wins_by_tier` |
| Iterate tier keys | `for winByTier in seasons[season]["WinsByTier"]:` | `for tier_str in wbt.keys()` |
| Parse tier | `int(winByTier)` | `tier_str.parse::<u32>()` |
| Before-asc adj | `if season in self.ranks_before and int(winByTier) > 20: winByTier = int(winByTier) + 3` | `if is_before_ascendant(sid) && tier_val > 20 { tier_val += 3; }` |
| Compare | `if int(winByTier) > max_rank:` | `if tier_val > max_rank` |
| Store max | `max_rank = int(winByTier); max_rank_season = season` | `max_rank = tier_val; max_season_id = sid.clone()` |

### 2.2 Act/Episode Parsing

#### Helper Functions (identical logic)

| Helper | Python (`src/content.py`) | Rust (`models/content.rs`) |
|--------|------|------|
| `has_letter_and_number()` | Lines 84–86 | Lines 257–259 |
| `roman_to_int()` | Lines 88–104 | Lines 261–281 |
| `parse_season_number()` | Lines 106–126 | Lines 285–311 |

#### Main Function

| Step | Python (`src/content.py:68–80`) | Rust (`models/content.rs:227–254`) |
|------|------|------|
| Init trailing_episode | `self.content["Seasons"][0]` (first season) | `None` |
| Iterate | `for season in self.content["Seasons"]:` | `for season in &self.seasons {` |
| Track episode | `if season["Type"] == "episode": trailing_episode = season` | `if season.season_type == Some("episode") { trailing_episode = Some(season); }` |
| Match act_id | `if season["ID"] == act_id:` | `if season.id.eq_ignore_ascii_case(act_id)` |
| Return | `{"act": ..., "episode": ...}` | `(act, episode)` tuple |

### 2.3 Loadout Resolution

| Step | Python (`src/Loadouts.py`) | Rust (`services/loadouts.rs`) |
|------|------|------|
| Socket UUIDs | From `sockets` dict in constants | Hardcoded: `bcef87d6-...`, `e7c63390-...`, `3ad1b2b2-...`, `77258665-...` |
| Skin ID extraction | `items[weapon_uuid]["Sockets"]["bcef87d6-..."]["Item"]["ID"]` | `sockets.get("bcef87d6-...").and_then(\|s\| s.item).and_then(\|i\| i.id)` |
| Skin name | `skin["displayName"]` | `skin.display_name.clone()` (no strip) |
| Chroma resolution | Iterate `weapon["skins"][skin_uuid]["chromas"]` by UUID | Same in Rust |
| Chroma icon chain | `displayIcon` → `fullRender` → skin `displayIcon` → level 0 `displayIcon` | Same chain |
| Buddy resolution | Match UUID in `valoApiBuddies` | `content.buddies.get(&bid.to_lowercase())` |
| PlayerCardName | `PCard.get("displayName", "")` | `card.display_name.clone()` |
| TitleName | `title.get("displayName", title["titleText"])` | `title_obj.display_name.clone()` |
| Spray resolution | Check sprays dict, then flex dict | Same in Rust |
| Skin buddy level socket | Python includes `dd3bf334-...` UUID | **Not in Rust** (only 4 of 5 sockets) |

### 2.4 Stats Processing

| Step | Python (`src/player_stats.py`) | Rust (`services/stats.rs`) |
|------|------|------|
| Conditional fetch check | `_should_fetch_comp_stats()` checks table flags | **Not implemented** — always fetches |
| Fetch competitive updates | `GET /mmr/v1/players/{puuid}/competitiveupdates?startIndex=0&endIndex=1` | Same endpoint |
| Get match ID | `match_summary["MatchID"]` | `update.match_id` |
| Fetch match details (`"pd"` URL type) | `GET /match-details/v1/matches/{match_id}` via `"pd"` | Same, `UrlType::Pd` |
| Calc KD | `kills / deaths` | Same |
| Calc HS% | `headshots / (legshots+bodyshots+headshots) * 100` | Same |
| Get RR earned | From competitive update summary | `update.ranked_rating_earned` |
| Get AFK penalty | From competitive update summary | `update.afk_penalty` |
| Calc last active | `(MatchStartTime + gameLengthMillis) / 1000` with `gameStartMillis` fallback | Same |

### 2.5 Encounter Tracking

| Step | Python (`src/stats.py`) | Rust (`services/encounters.rs`) |
|------|------|------|
| Save encounter | `save_data()`: normalizes history, dedup by match_id | `save_encounter()`: same dedup logic |
| Build summary | `build_encounter_summary()`: count ally/enemy wins/losses | Same in `build_encounter_summary()` |
| Update result | `update_match_result()`: sets result by relation | Same in `update_match_result()` |
| All summaries | N/A (no frontend use in Python) | `get_all_summaries()` for MENUS heartbeat |

### 2.6 Presence Tracking

| Step | Python (`src/presences.py`) | Rust (`services/presences.rs`) |
|------|------|------|
| Fetch presences | `GET /chat/v4/presences` from local API | Same endpoint |
| Find own | Match by puuid + product=="valorant" | Same |
| Decode private | base64 decode `presence['private']` JSON | Same in `decode_private_presence()` |
| Extract state | `sessionLoopState` from nested or flat | Same in `extract_game_state()` |
| Extract queue ID | `queueId` from nested or flat | Same in `extract_queue_id()` |
| Extract party ID | `partyId` from nested or flat | Same in `extract_party_id()` |
| Extract account level | `accountLevel` from nested or flat | Same in `extract_account_level()` |
| Find party members | Match puuids by partyId | Same in `find_party_member_puuids()` |

### 2.7 Name Resolution

| Step | Python (`src/names.py`) | Rust (`services/names.rs`) |
|------|------|------|
| Local API | POST `/player-account/lookup/v2/namesets-for-puuids` | Same |
| Parse names | `alias["GameName"] + "#" + alias["TagLine"]` | Same |
| PD fallback | PUT `/name-service/v2/players` with PUUID array | Same |
| **Caching** | **No cache** (fetches every heartbeat) | **No cache** (same — both re-fetch every state transition) |

### 2.8 Match Player Cache (Python-only, not in Rust)

| Step | Python (`main.py:239–263`) | Rust |
|------|---------------------------|------|
| Cache init | `reset_match_player_cache(match_id)` on new match | **Not implemented** |
| Cache lookup | `match_player_cache["players"].get(puuid)` | Always re-fetches rank+stats |
| Cache fill | Stores `(playerRank, previousPlayerRank, ppstats)` per match | N/A |
| TTL cleanup | 300s safety TTL per entry | N/A |

### 2.9 Client API

| Feature | Python (`src/requestsV.py`) | Rust (`api/client.rs`) |
|---------|------|------|
| Rate limiting | `_throttle()` rolling 1s window per URL type | `RateLimiter` with `check_rate()` + `record_request()` |
| Auth headers | `get_headers()` → Authorization, X-Riot-Entitlements-JWT, etc. | `build_headers()` → same 5 headers |
| Lockfile | `get_lockfile()` → parses `:` delimited file | `parse_lockfile()` → same |
| Region parsing | `get_region()` → scans ShooterGame.log | `parse_region_from_logs()` → same |
| Version parsing | `get_current_version()` → "CI server version:" | `parse_client_version()` → same |
| Authenticate | GET `/entitlements/v1/token` → accessToken, token, subject | Same in `authenticate()` |
| Error handling | `BAD_CLAIMS` retry, 429 backoff, RPC_ERROR retry | Same error handling in `fetch_with_method()` |
| **Deceive detection** | `is_deceive_running()` tasklist check | **Not implemented** |
| **Version check** | `check_version()` hits GitHub API | **Not implemented** |
| **Auto-update** | `copy_run_update_script()` | **Not implemented** |

---

## 3. Data Model Mapping

### Backend → Frontend Fields

| Python Payload Field | Rust `HeartbeatPayload` Field | Frontend Usage |
|---------------------|------------------------------|----------------|
| `type` | `r#type: String` ("heartbeat") | Event routing |
| `puuid` | `puuid: String` | Self identification |
| `state` | `state: String` | State chip, headers |
| `mode` | `mode: Option<String>` | Meta chip |
| `map` | `map: Option<String>` | Meta chip |
| `server` | `server: Option<String>` | Meta chip |
| `time` | `time: i64` | "Updated" timestamp |
| `rankIcons` | `rank_icons: Arc<Vec<Option<String>>>` | Rank icon URLs |
| `players` | `players: HashMap<String, PlayerHeartbeat>` | Player data |
| `alreadyPlayedWith` | `already_played_with: Vec<EncounterEntry>` | Encounter table |
| `version` | **Rust-only** `version: u64` | Not used by frontend |

### Per-Player Fields

| Python Field | Rust `PlayerHeartbeat` Field | Frontend Usage |
|-------------|------------------------------|----------------|
| `puuid` | `puuid: String` | Identification |
| `name` | `name: Option<String>` | Display, TRN/VTL links |
| `partyNumber` | `party_number: u32` | Party grouping |
| `agent` | `agent: Option<String>` | Agent name |
| `agentImgLink` | `agent_img_link: Option<String>` | Agent avatar URL |
| `rank` | `rank: u32` | Rank name/color/icon |
| `peakRank` | `peak_rank: u32` | Peak rank |
| `peakRankAct` | `peak_rank_act: Option<String>` | Peak rank act |
| `previousRank` | `previous_rank: u32` | Last act rank |
| `rr` | `rr: i32` | Rank rating |
| `leaderboard` | `leaderboard: i32` | Leaderboard pos |
| `winPercentage` | `win_percentage: Option<String>` | Win rate |
| `kd` | `kd: String` | Not displayed in vry_gui |
| `headshotPercentage` | `headshot_percentage: String` | Not displayed |
| `lastActive` | `last_active: Option<String>` | Last active chip |
| `level` | `level: Option<u32>` | Player level |
| `team` | `team: Option<String>` | Team assignment |
| `title` | `title: Option<String>` | Player title text |
| `playerCard` | `player_card: Option<String>` | Player card URL |
| `playerCardName` | `player_card_name: Option<String>` | Card name tooltip |
| `titleName` | `title_name: Option<String>` | Title name |
| `sprays` | `sprays: Option<HashMap<String, SprayEntry>>` | Expression wheel |
| `weapons` | `weapons: Option<HashMap<String, WeaponEntry>>` | Weapon inventory |
| `earnedRR` | `earned_rr: Option<String>` | **Rust-only** — not in Python payload |
| `agentImgLink` source | From valapi `displayIcon` | Rust: constructed URL `https://media.valorant-api.com/agents/{cid}/displayicon.png` |

---

## 4. Frontend Architecture Comparison

| Python (`docs/vry_gui.js`) | Rust (`frontend/vry_gui.js`) |
|------|------|
| WebSocket `ws://host:port/` | Tauri IPC: `tauriListen("heartbeat", ...)` |
| `socket.onmessage` → `setPayload()` | `setupTauriListeners()` → `setPayload()` |
| `sendAction("restart_application")` | `tauriInvoke("restart_application")` |
| `pywebview.api.get_gui_log_tail()` | `tauriInvoke("get_gui_log_tail")` |
| Reconnect with 2.5s timer | No reconnect (Tauri events are reliable) |
| `localStorage("vry.testPage.cache")` | `localStorage("vry-rust.cache")` |
| `state.payload.players` (object) | Same structure |
| `state.selectedPuuid` | Same state field |
| `state.lastGameState` | Same state field |
| Right-click: `[card, name, title].forEach(...)` | Same pattern |
| `renderStateTransition()` | Same function |
| `takeScreenshot()` (html2canvas) | Same function |
| Event wiring in IIFE bottom | Same pattern |

---

## 5. Config Cross-Reference

### Config Flags — Implementation Status

| Config Flag | Python Default | Rust Default | Python Uses It | Rust Implements It? |
|-------------|---------------|--------------|----------------|---------------------|
| `cooldown` | 10 | 10 | Poll interval | ✅ Same |
| `port` | 1100 | 1100 | WebSocket port | ✅ Stored but unused (Tauri IPC, no WS) |
| `weapon` | Vandal | Vandal | Preview weapon | ✅ Same |
| `chat_limit` | 5 | 5 | Chat display | ✅ Stored (chat not implemented) |
| `table.skin` | true | true | Column display | ✅ Frontend uses |
| `table.rr` | true | true | Column display | ✅ Frontend uses |
| `table.earned_rr` | false | false | Column display | ✅ Frontend uses |
| `table.peakrank` | true | true | Column display | ✅ Frontend uses |
| `table.previousrank` | false | false | Column display | ✅ Frontend uses |
| `table.leaderboard` | true | true | Column display | ✅ Frontend uses |
| `table.headshot_percent` | false | false | Column display | ✅ Frontend uses |
| `table.winrate` | true | true | Column display | ✅ Frontend uses |
| `table.kd` | false | false | Column display | ✅ Frontend uses |
| `table.level` | true | true | Column display | ✅ Frontend uses |
| `table.last_active` | true | true | Column display | ✅ Frontend uses |
| `flags.discord_rpc` | false | false | Discord RPC init | ❌ Not implemented (no RPC at all) |
| `flags.aggregate_rank_rr` | true | true | Appends RR to rank string | ❌ Stored, never referenced |
| `flags.peak_rank_act` | true | true | Toggles peak rank act display | ❌ Stored, never referenced |
| `flags.auto_hide_leaderboard` | true | true | Hides LB column when no one has rank | ❌ Stored, never referenced |
| `flags.game_chat` | false | false | Chat WebSocket | ❌ Not implemented |
| `flags.last_played` | true | true | Prints encounter summary to console | ❌ N/A (GUI-only) |
| `flags.pre_cls` | false | false | Clears console before print | ❌ N/A (GUI-only) |
| `flags.server_id` | false | false | Server ID in title bar | ❌ Stored, never referenced |
| `flags.short_ranks` | false | false | Abbreviated rank names | ❌ Stored, never referenced |
| `flags.truncate_skins` | true | true | Cuts long skin names | ❌ Stored, never referenced |
| `flags.truncate_names` | false | false | Cuts long player names | ❌ Stored, never referenced |
| `flags.starting_side` | true | true | Shows DEF/ATK indicator | ❌ Stored, never referenced |

---

## 6. Tauri Commands (Rust IPC)

| Command | Rust File | Purpose | Python Equivalent |
|---------|-----------|---------|-------------------|
| `get_config` | `commands/config.rs:10` | Returns `AppConfig` as JSON | `config.py` read |
| `set_config` | `commands/config.rs:18` | Updates config, saves to disk | `config.py` write |
| `get_gui_log_tail` | `commands/config.rs:29` | Returns last 50 log lines | `logs.py` read (via pywebview) |
| `get_heartbeat_log` | `commands/config.rs:37` | Returns heartbeat JSONL file | Not in Python |
| `get_version` | `commands/system.rs:9` | Returns version string | `constants.py` version var |
| `restart_application` | `commands/system.rs:14` | Resets backend state, reconnects | `Requests.get_headers(refresh=True)` |
| `get_status` | `commands/system.rs:32` | Returns connection status JSON | Not in Python |

---

## 7. Key Mapping Notices

### Fixed in Rust vs Python

| Fix | Python Issue | Rust |
|-----|-------------|------|
| Last active epoch | Mixed `MatchStartTime` / `gameStartMillis` sources | Both sources tried, adds `gameLengthMillis` |
| Encounter score/result | Multi-casing iteration for team fields | `.or_else()` chaining handles same variants |
| Spray icon fallback | `fullTransparentIcon` check | Same `full_transparent_icon.or_else(display_icon)` |

### Regressions / Gaps in Rust

| Gap | Impact | Suggested Fix |
|-----|--------|---------------|
| No conditional stats fetching | +2 API calls per player when stat columns hidden | Port `_should_fetch_comp_stats()` |
| No match player cache | +~15 API calls per PREGAME→INGAME transition | Port `get_or_fetch_rank_and_stats()` |
| No names cache | 1-2 API calls per heartbeat | Add TTL cache matching `RankService` pattern |
| No retry queue for match results | Single attempt, no retry on transient failure | Port `process_pending_match_results()` |
| No Deceive detection | Deceive users stuck in DISCONNECTED | Port `is_deceive_running()` |
| Missing buddy_level socket | Missing weapon buddy level display | Add `dd3bf334-...` socket UUID |
| 8 config flags stored but unimplemented | Frontend cosmetic features inactive | Wire flags into payload/frontend |
| `opt-level = 0` in release profile | All Rust performance benefits negated | Change to `opt-level = 2` |
| RwLock held across entire main loop | UI commands block during heartbeat assembly | Narrow read-lock scope |
