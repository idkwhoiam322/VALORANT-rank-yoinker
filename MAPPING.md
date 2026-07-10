# Code Mapping: Python → Rust/Tauri

**Python repo root:** `VALORANT-rank-yoinker/`  
**Rust repo root:** `VRY_rewrite_tauri_fixed/tauri_rewrite/`

---

## 1. File-Level Mapping

### Backend

| Python File | Lines | Rust File | Lines | Coverage |
|-------------|-------|-----------|-------|----------|
| `main.py` | 1357 | `core/state_machine.rs` + `core/payload_builder.rs` | 286 + 749 | State machine + heartbeat building |
| `main.py` (main loop) | 387–1339 | `core/state_machine.rs:157–238` | 81 | Main polling loop |
| `main.py` (INGAME handler) | 544–867 | `core/payload_builder.rs:106–373` | 267 | INGAME heartbeat assembly |
| `main.py` (PREGAME handler) | 868–1123 | `core/payload_builder.rs:374–602` | 228 | PREGAME heartbeat assembly |
| `main.py` (MENUS handler) | 1125–1268 | `core/payload_builder.rs:603–731` | 128 | MENUS heartbeat assembly |
| `main.py` (helpers) | 52–103 | `core/payload_builder.rs:732–749` | 17 | `format_last_active()` |
| `src/Loadouts.py` | 295 | `services/loadouts.rs` | 252 | Skin/loadout/spray resolution |
| `src/rank.py` | 147 | `services/rank.rs` | 172 | MMR, peak rank, win rate |
| `src/content.py` | 163 | `api/content.rs` + `models/content.rs` | 211 + 311 | Content cache + season parsing helpers |
| `src/constants.py` | 259 | Scattered across multiple files | — | Constants split by domain |
| `src/server.py` | 60 | `lib.rs` (Tauri events) | 47 | IPC (Tauri events vs WebSocket) |
| `src/presences.py` | 118 | `services/presences.rs` | 183 | Presence tracking + party detection |
| `src/names.py` | 42 | `services/names.rs` | 116 | Name resolution |
| `src/player_stats.py` | 155 | `services/stats.rs` | 188 | HS%, KD, RR earned, last active |
| `src/stats.py` | 232 | `services/encounters.rs` | 302 | Encounter tracking |
| `src/requestsV.py` | 308 | `api/client.rs` + `api/auth.rs` | 275 + 148 | API client + auth |
| `src/websocket.py` | 161 | `api/websocket.rs` | 142 | Local WebSocket |
| `src/config.py` | 78 | `services/config.rs` | 239 | Config management |
| `src/logs.py` | 43 | `services/logging.rs` | 80 | Log file management |
| `src/errors.py` | 38 | `api/client.rs:ApiError` | (inline) | Error types |
| `src/states/menu.py` | 108 | `services/presences.rs` (party methods) | (inline) | Party logic |
| `src/states/coregame.py` | 58 | `core/payload_builder.rs` (inline) | (inline) | Inline in builder |
| `src/states/pregame.py` | 42 | `core/payload_builder.rs` (inline) | (inline) | Inline in builder |

### Not Ported (Console-Only)

| Python File | Lines | Reason |
|-------------|-------|--------|
| `src/colors.py` | 221 | ANSI terminal color – not needed for GUI |
| `src/table.py` | 227 | Rich console table – Tauri is GUI-only |
| `src/rpc.py` | 378 | Discord RPC – not implemented |
| `src/configurator.py` | 75 | Interactive config wizard – not implemented |
| `src/account_manager/*` | 593 | Account management – not implemented |
| `src/questions.py` | 93 | Config wizard questions – not needed |
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

### 2.4 Stats Processing

| Step | Python (`src/player_stats.py`) | Rust (`services/stats.rs`) |
|------|------|------|
| Fetch competitive updates | `GET /mmr/v1/players/{puuid}/competitiveupdates?startIndex=0&endIndex=1` | Same endpoint |
| Get match ID | `match_summary["MatchID"]` | `update.match_id` |
| Fetch match details | `GET /match-details/v1/matches/{match_id}` | Same endpoint |
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
| PD fallback | PUT `/name-service/v2/players` | Same |

### 2.8 Client API

| Feature | Python (`src/requestsV.py`) | Rust (`api/client.rs`) |
|---------|------|------|
| Rate limiting | `_throttle()` rolling 1s window | `RateLimiter` with `check_rate()` + `record_request()` |
| Auth headers | `get_headers()` → Authorization, X-Riot-Entitlements-JWT, etc. | `build_headers()` → same 5 headers |
| Lockfile | `get_lockfile()` → parses `:` delimited file | `parse_lockfile()` → same |
| Region parsing | `get_region()` → scans ShooterGame.log | `parse_region_from_logs()` → same |
| Version parsing | `get_current_version()` → "CI server version:" | `parse_client_version()` → same |
| Authenticate | POST `/entitlements/v1/token` → accessToken, token, subject | Same in `authenticate()` |
| Error handling | `BAD_CLAIMS` retry, 429 backoff, RPC_ERROR retry | Same error handling in `fetch_with_method()` |

---

## 3. Data Model Mapping

### Backend → Frontend Fields

| Python Payload Field | Rust `HeartbeatPayload` Field | Frontend Usage |
|---------------------|------------------------------|----------------|
| `type` | (not sent, Tauri event type) | Event routing |
| `puuid` | `puuid: String` | Self identification |
| `state` | `state: String` | State chip, headers |
| `mode` | `mode: Option<String>` | Meta chip |
| `map` | `map: Option<String>` | Meta chip |
| `server` | `server: Option<String>` | Meta chip |
| `time` | `time: i64` | "Updated" timestamp |
| `rankIcons` | `rank_icons: Vec<Option<String>>` | Rank icon URLs |
| `players` | `players: HashMap<String, PlayerHeartbeat>` | Player data |
| `alreadyPlayedWith` | `already_played_with: Vec<EncounterEntry>` | Encounter table |

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
| `earnedRR` | `earned_rr: Option<String>` | Not displayed |

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

| Python Config Key | Rust `AppConfig` Field | Default |
|-------------------|----------------------|---------|
| `cooldown` | `cooldown: u64` | 10 |
| `port` | `port: u16` | 1100 |
| `weapon` | `weapon: String` | "Vandal" |
| `chat_limit` | `chat_limit: u32` | 5 |
| `table.skin` | `table.skin: bool` | true |
| `table.rr` | `table.rr: bool` | true |
| `table.earned_rr` | `table.earned_rr: bool` | false |
| `table.peakrank` | `table.peakrank: bool` | true |
| `table.previousrank` | `table.previousrank: bool` | false |
| `table.leaderboard` | `table.leaderboard: bool` | true |
| `table.headshot_percent` | `table.headshot_percent: bool` | false |
| `table.winrate` | `table.winrate: bool` | true |
| `table.kd` | `table.kd: bool` | false |
| `table.level` | `table.level: bool` | true |
| `table.last_active` | `table.last_active: bool` | true |
| `flags.last_played` | `flags.last_played: bool` | true |
| `flags.auto_hide_leaderboard` | `flags.auto_hide_leaderboard: bool` | true |
| `flags.pre_cls` | `flags.pre_cls: bool` | true |
| `flags.game_chat` | `flags.game_chat: bool` | true |
| `flags.peak_rank_act` | `flags.peak_rank_act: bool` | true |
| `flags.discord_rpc` | `flags.discord_rpc: bool` | true |
| `flags.aggregate_rank_rr` | `flags.aggregate_rank_rr: bool` | true |
| `flags.server_id` | `flags.server_id: bool` | true |
| `flags.short_ranks` | `flags.short_ranks: bool` | false |
| `flags.truncate_skins` | `flags.truncate_skins: bool` | false |
| `flags.truncate_names` | `flags.truncate_names: bool` | false |
| `flags.starting_side` | `flags.starting_side: bool` | false |
