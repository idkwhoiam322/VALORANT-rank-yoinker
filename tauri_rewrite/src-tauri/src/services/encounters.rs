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
        Self { records, match_index: index }
    }
}

pub struct EncounterService {
    stats_path: PathBuf,
    data: Mutex<EncounterData>,
}

impl EncounterService {
    pub fn new(root: PathBuf) -> Self {
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

    pub fn save_encounter(&self, puuid: &str, record: EncounterRecord) {
        let mut data = self.data.lock().unwrap();
        let match_id = record.match_id.clone();

        // Insert or merge the record; track whether a new match_id entry was added
        let is_new_match = {
            let history = data.records.entry(puuid.to_string()).or_default();

            if let Some(ref mid) = match_id {
                if let Some(existing) = history.iter_mut().find(|e| e.match_id.as_deref() == Some(mid)) {
                    // Merge: update fields if new values are non-null
                    if record.name.is_some() { existing.name = record.name.clone(); }
                    if record.agent.is_some() { existing.agent = record.agent.clone(); }
                    if record.map.is_some() { existing.map = record.map.clone(); }
                    if record.rank.is_some() { existing.rank = record.rank; }
                    if record.rr.is_some() { existing.rr = record.rr; }
                    if record.relation.is_some() { existing.relation = record.relation.clone(); }
                    if record.team.is_some() { existing.team = record.team.clone(); }
                    if record.my_team.is_some() { existing.my_team = record.my_team.clone(); }
                    if record.result.is_some() { existing.result = record.result.clone(); }
                    if record.score.is_some() { existing.score = record.score.clone(); }
                    if record.epoch.is_some() { existing.epoch = record.epoch; }
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
                data.match_index.entry(mid.clone()).or_default().push(puuid.to_string());
            }
        }

        self.save_to_disk(&data.records);
    }

    pub fn update_match_result(
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

        let mut data = self.data.lock().unwrap();
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

    pub fn build_encounter_summary(
        &self,
        puuid: &str,
        current_match_id: &str,
        fallback_name: &str,
        fallback_relation: Option<&str>,
        current_agent: Option<&str>,
        current_map: Option<&str>,
    ) -> Option<EncounterEntry> {
        let data = self.data.lock().unwrap();
        let history = data.records.get(puuid)?;

        let mut previous: Vec<&EncounterRecord> = history
            .iter()
            .filter(|e| e.match_id.as_deref() != Some(current_match_id))
            .collect();

        if previous.is_empty() {
            return None;
        }

        // Sort by epoch descending so the most recent record is first
        previous.sort_by(|a, b| b.epoch.partial_cmp(&a.epoch).unwrap_or(std::cmp::Ordering::Equal));

        // Deduplicate by match_id (keep first = most recent after sort)
        let mut seen = std::collections::HashSet::new();
        previous.retain(|e| {
            let key = e.match_id.as_deref().unwrap_or("");
            if seen.contains(key) {
                false
            } else {
                seen.insert(key.to_string());
                true
            }
        });

        let latest = previous[0];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        let mut ally_wins = 0usize;
        let mut ally_losses = 0usize;
        let mut ally_unknown = 0usize;
        let mut enemy_wins = 0usize;
        let mut enemy_losses = 0usize;
        let mut enemy_unknown = 0usize;

        for entry in &previous {
            let rel = entry.relation.as_deref().unwrap_or("");
            match (rel, entry.result.as_deref()) {
                ("ally", Some("win")) => ally_wins += 1,
                ("ally", Some("loss")) => ally_losses += 1,
                ("ally", _) => ally_unknown += 1,
                ("enemy", Some("win")) => enemy_wins += 1,
                ("enemy", Some("loss")) => enemy_losses += 1,
                ("enemy", _) => enemy_unknown += 1,
                _ => {}
            }
        }

        let time_diff = latest.epoch.map(|e| now - e).unwrap_or(0.0);
        let latest_relation = latest.relation.as_deref().unwrap_or(fallback_relation.unwrap_or(""));
        let latest_name = latest
            .name
            .as_deref()
            .filter(|n| *n != "#")
            .unwrap_or(fallback_name);

        Some(EncounterEntry {
            times: previous.len(),
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

    pub fn get_all_summaries(&self, exclude_puuid: &str) -> Vec<crate::models::heartbeat::EncounterEntry> {
        let data = self.data.lock().unwrap();
        let mut results = Vec::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        for (puuid, history) in data.records.iter() {
            if puuid == exclude_puuid {
                continue;
            }

            if history.is_empty() {
                continue;
            }

            // Collect all records not matching current match (none excluded here)
            let mut records: Vec<&EncounterRecord> = history.iter().collect();

            // Sort by epoch descending so the most recent is first
            records.sort_by(|a, b| b.epoch.partial_cmp(&a.epoch).unwrap_or(std::cmp::Ordering::Equal));

            // Deduplicate by match_id (keep first = most recent after sort)
            let mut seen = std::collections::HashSet::new();
            let deduped: Vec<&EncounterRecord> = records.into_iter().filter(|e| {
                let key = e.match_id.as_deref().unwrap_or("");
                if seen.contains(key) { false } else { seen.insert(key.to_string()); true }
            }).collect();

            if deduped.is_empty() {
                continue;
            }

            let latest = deduped[0];

            let mut ally_wins = 0usize;
            let mut ally_losses = 0usize;
            let mut ally_unknown = 0usize;
            let mut enemy_wins = 0usize;
            let mut enemy_losses = 0usize;
            let mut enemy_unknown = 0usize;

            for entry in &deduped {
                let rel = entry.relation.as_deref().unwrap_or("");
                match (rel, entry.result.as_deref()) {
                    ("ally", Some("win")) => ally_wins += 1,
                    ("ally", Some("loss")) => ally_losses += 1,
                    ("ally", _) => ally_unknown += 1,
                    ("enemy", Some("win")) => enemy_wins += 1,
                    ("enemy", Some("loss")) => enemy_losses += 1,
                    ("enemy", _) => enemy_unknown += 1,
                    _ => {}
                }
            }

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
                }.into(),
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
        results.sort_by(|a, b| a.time_diff.partial_cmp(&b.time_diff).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    fn save_to_disk(&self, data: &HashMap<String, Vec<EncounterRecord>>) {
        if let Ok(json) = serde_json::to_string_pretty(data) {
            let _ = fs::write(&self.stats_path, &json);
        }
    }
}
