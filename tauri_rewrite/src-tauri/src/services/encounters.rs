use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::models::heartbeat::EncounterEntry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterRecord {
    pub name: Option<String>,
    pub agent: Option<String>,
    pub map: Option<String>,
    pub rank: Option<u32>,
    pub rr: Option<i32>,
    pub match_id: Option<String>,
    pub epoch: Option<f64>,
    pub relation: Option<String>,
    pub team: Option<String>,
    pub my_team: Option<String>,
    pub result: Option<String>,
    pub score: Option<String>,
}

/// Internal data behind the single Mutex - holds both the per-puuid records
/// and a match_id → [puuids] index for O(1) lookups in update_match_result.
struct EncounterData {
    /// puuid → list of encounter records
    records: HashMap<String, Vec<EncounterRecord>>,
    /// match_id → puuids that have an encounter with this match_id
    match_index: HashMap<String, Vec<String>>,
}

impl EncounterData {
    fn from_records(records: HashMap<String, Vec<EncounterRecord>>) -> Self {
        let mut index: HashMap<String, Vec<String>> = HashMap::new();
        for (puuid, history) in &records {
            for entry in history {
                if let Some(ref mid) = entry.match_id {
                    index.entry(mid.clone()).or_default().push(puuid.clone());
                }
            }
        }
        Self {
            records,
            match_index: index,
        }
    }
}

/// Rolling 6-way win/loss/unknown tally for ally + enemy encounters.
/// Shared by `build_encounter_summary` and `get_all_summaries` so the
/// identical tally block lives in exactly one place.
#[derive(Default)]
struct Tally {
    ally_wins: usize,
    ally_losses: usize,
    ally_unknown: usize,
    enemy_wins: usize,
    enemy_losses: usize,
    enemy_unknown: usize,
}

impl Tally {
    fn add(&mut self, relation: &str, result: Option<&str>) {
        match (relation, result) {
            ("ally", Some("win")) => self.ally_wins += 1,
            ("ally", Some("loss")) => self.ally_losses += 1,
            ("ally", _) => self.ally_unknown += 1,
            ("enemy", Some("win")) => self.enemy_wins += 1,
            ("enemy", Some("loss")) => self.enemy_losses += 1,
            ("enemy", _) => self.enemy_unknown += 1,
            _ => {}
        }
    }
}

/// Sort a set of encounter records by epoch (most recent first) and de-duplicate
/// by `match_id`, keeping the most recent record for each match. Shared by both
/// `build_encounter_summary` and `get_all_summaries` so the sort+dedup logic
/// lives in exactly one place (previously duplicated in both functions).
fn dedup_sorted(mut records: Vec<&EncounterRecord>) -> Vec<&EncounterRecord> {
    records.sort_by(|a, b| {
        b.epoch
            .partial_cmp(&a.epoch)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen = std::collections::HashSet::new();
    records.retain(|e| seen.insert(e.match_id.as_deref().unwrap_or("").to_string()));
    records
}

pub(crate) struct EncounterService {
    stats_path: PathBuf,
    data: Mutex<EncounterData>,
}

impl EncounterService {
    pub(crate) fn new(root: PathBuf) -> Self {
        let stats_dir = root.join("stats");
        let stats_path = stats_dir.join("encounters.json");
        let _ = fs::create_dir_all(&stats_dir);

        let records = if stats_path.exists() {
            fs::read_to_string(&stats_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            HashMap::new()
        };

        Self {
            stats_path,
            data: Mutex::new(EncounterData::from_records(records)),
        }
    }

    pub(crate) fn save_encounter(&self, puuid: &str, record: EncounterRecord) {
        let mut data = self.data.lock().expect("encounter data");
        let match_id = record.match_id.clone();

        // Insert or merge the record; track whether a new match_id entry was added
        let is_new_match = {
            let history = data.records.entry(puuid.to_string()).or_default();

            if let Some(ref mid) = match_id {
                if let Some(existing) = history
                    .iter_mut()
                    .find(|e| e.match_id.as_deref() == Some(mid))
                {
                    // Merge: update fields if new values are non-null.
                    // relation/team/my_team are immutable for a given match_id,
                    // so they are only seeded on first insert; a later partial
                    // save (e.g. pregame before the self player's team is known)
                    // must not flip an already-correct relation for this match.
                    if record.name.is_some() {
                        existing.name = record.name.clone();
                    }
                    if record.agent.is_some() {
                        existing.agent = record.agent.clone();
                    }
                    if record.map.is_some() {
                        existing.map = record.map.clone();
                    }
                    if record.rank.is_some() {
                        existing.rank = record.rank;
                    }
                    if record.rr.is_some() {
                        existing.rr = record.rr;
                    }
                    if existing.relation.is_none() && record.relation.is_some() {
                        existing.relation = record.relation.clone();
                    }
                    if existing.team.is_none() && record.team.is_some() {
                        existing.team = record.team.clone();
                    }
                    if existing.my_team.is_none() && record.my_team.is_some() {
                        existing.my_team = record.my_team.clone();
                    }
                    if record.result.is_some() {
                        existing.result = record.result.clone();
                    }
                    if record.score.is_some() {
                        existing.score = record.score.clone();
                    }
                    if record.epoch.is_some() {
                        existing.epoch = record.epoch;
                    }
                    false // merged, not a new entry
                } else {
                    history.push(record);
                    true // new entry with a match_id
                }
            } else {
                history.push(record);
                false // no match_id
            }
        };

        // Update the match index outside the `history` borrow
        if is_new_match {
            if let Some(ref mid) = match_id {
                data.match_index
                    .entry(mid.clone())
                    .or_default()
                    .push(puuid.to_string());
            }
        }

        self.save_to_disk(&data.records);
    }

    pub(crate) fn update_match_result(
        &self,
        match_id: &str,
        my_team: &str,
        winning_team: &str,
        score: Option<String>,
    ) -> bool {
        let my_result = if my_team == winning_team {
            "win"
        } else {
            "loss"
        };
        let enemy_result = if my_team == winning_team {
            "loss"
        } else {
            "win"
        };

        let mut data = self.data.lock().expect("encounter data");
        let mut changed = false;

        // Use the match_index to find only relevant puuids (O(1) instead of scanning all puuids)
        if let Some(puuids) = data.match_index.get(match_id).cloned() {
            for puuid in &puuids {
                if let Some(history) = data.records.get_mut(puuid) {
                    for entry in history.iter_mut() {
                        if entry.match_id.as_deref() != Some(match_id) {
                            continue;
                        }
                        let result = if entry.relation.as_deref() == Some("enemy") {
                            enemy_result
                        } else {
                            my_result
                        };
                        if entry.result.as_deref() != Some(result) {
                            entry.result = Some(result.into());
                            changed = true;
                        }
                        if score.is_some() {
                            entry.score = score.clone();
                        }
                    }
                }
            }
        }

        if changed {
            self.save_to_disk(&data.records);
        }
        changed
    }

    pub(crate) fn build_encounter_summary(
        &self,
        puuid: &str,
        current_match_id: &str,
        fallback_name: &str,
        fallback_relation: Option<&str>,
        current_agent: Option<&str>,
        current_map: Option<&str>,
    ) -> Option<EncounterEntry> {
        // Clone the relevant history out of the lock first; the sort + dedup
        // below is O(n log n) and must not run while the mutex is held.
        let history = {
            let data = self.data.lock().expect("encounter data");
            data.records.get(puuid).cloned()
        }?;

        let previous: Vec<EncounterRecord> = history
            .into_iter()
            .filter(|e| e.match_id.as_deref() != Some(current_match_id))
            .collect();

        if previous.is_empty() {
            return None;
        }

        // Sort by epoch descending and de-duplicate by match_id (keep most recent).
        let previous_refs: Vec<&EncounterRecord> = previous.iter().collect();
        let deduped = dedup_sorted(previous_refs);

        let latest = deduped[0];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        let mut tally = Tally::default();
        for entry in &deduped {
            tally.add(
                entry.relation.as_deref().unwrap_or(""),
                entry.result.as_deref(),
            );
        }
        let ally_wins = tally.ally_wins;
        let ally_losses = tally.ally_losses;
        let ally_unknown = tally.ally_unknown;
        let enemy_wins = tally.enemy_wins;
        let enemy_losses = tally.enemy_losses;
        let enemy_unknown = tally.enemy_unknown;

        let time_diff = latest.epoch.map(|e| now - e).unwrap_or(0.0);
        let latest_relation = latest
            .relation
            .as_deref()
            .unwrap_or(fallback_relation.unwrap_or(""));
        let latest_name = latest
            .name
            .as_deref()
            .filter(|n| *n != "#")
            .unwrap_or(fallback_name);

        Some(EncounterEntry {
            times: deduped.len(),
            name: latest_name.to_string(),
            agent: current_agent
                .map(String::from)
                .or_else(|| latest.agent.clone())
                .unwrap_or_else(|| "Unknown".into()),
            map: current_map
                .map(String::from)
                .or_else(|| latest.map.clone())
                .unwrap_or_else(|| "Unknown".into()),
            last_agent: latest.agent.clone(),
            last_map: latest.map.clone(),
            relation: latest_relation.to_string(),
            relation_name: if latest_relation == "ally" {
                "teammate"
            } else {
                "enemy"
            }
            .into(),
            time_diff: time_diff.max(0.0),
            ally_wins,
            ally_losses,
            ally_unknown,
            ally_count: ally_wins + ally_losses + ally_unknown,
            enemy_wins,
            enemy_losses,
            enemy_unknown,
            enemy_count: enemy_wins + enemy_losses + enemy_unknown,
        })
    }

    pub fn get_all_summaries(
        &self,
        exclude_puuid: &str,
    ) -> Vec<crate::models::heartbeat::EncounterEntry> {
        // Snapshot the whole record map out of the lock; all per-player
        // sort/dedup work below runs on the owned copy so the mutex is only
        // held for the (cheap) clone, not the O(n log n) processing.
        let records = self.data.lock().expect("encounter data").records.clone();
        let mut results = Vec::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        for (puuid, history) in records.iter() {
            if puuid == exclude_puuid {
                continue;
            }

            if history.is_empty() {
                continue;
            }

            // Collect all records not matching current match (none excluded here)
            let records: Vec<&EncounterRecord> = history.iter().collect();

            // Sort by epoch descending and de-duplicate by match_id (keep most recent).
            let deduped = dedup_sorted(records);

            if deduped.is_empty() {
                continue;
            }

            let latest = deduped[0];

            let mut tally = Tally::default();
            for entry in &deduped {
                tally.add(
                    entry.relation.as_deref().unwrap_or(""),
                    entry.result.as_deref(),
                );
            }
            let ally_wins = tally.ally_wins;
            let ally_losses = tally.ally_losses;
            let ally_unknown = tally.ally_unknown;
            let enemy_wins = tally.enemy_wins;
            let enemy_losses = tally.enemy_losses;
            let enemy_unknown = tally.enemy_unknown;

            let time_diff = latest.epoch.map(|e| now - e).unwrap_or(0.0);
            let latest_name = latest.name.as_deref().unwrap_or("Unknown");

            let last_agent = latest.agent.clone();
            let last_map = latest.map.clone();
            results.push(crate::models::heartbeat::EncounterEntry {
                times: deduped.len(),
                name: latest_name.to_string(),
                agent: last_agent.clone().unwrap_or_else(|| "Unknown".into()),
                map: last_map.clone().unwrap_or_else(|| "Unknown".into()),
                last_agent,
                last_map,
                relation: latest.relation.clone().unwrap_or_default(),
                relation_name: if latest.relation.as_deref() == Some("ally") {
                    "teammate"
                } else {
                    "enemy"
                }
                .into(),
                time_diff: time_diff.max(0.0),
                ally_wins,
                ally_losses,
                ally_unknown,
                ally_count: ally_wins + ally_losses + ally_unknown,
                enemy_wins,
                enemy_losses,
                enemy_unknown,
                enemy_count: enemy_wins + enemy_losses + enemy_unknown,
            });
        }

        // Sort by most recent first (smaller time_diff = more recent)
        results.sort_by(|a, b| {
            a.time_diff
                .partial_cmp(&b.time_diff)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    fn save_to_disk(&self, data: &HashMap<String, Vec<EncounterRecord>>) {
        let json = match serde_json::to_string_pretty(data) {
            Ok(j) => j,
            Err(_) => return,
        };
        let path = self.stats_path.clone();
        // Offload the file write to a blocking thread so the async runtime
        // is not stalled by disk I/O. JSON serialization (CPU-only) already
        // happened above while the caller's mutex lock is still held.
        tokio::task::spawn_blocking(move || {
            let _ = std::fs::write(&path, &json);
        });
    }
}
