use serde_json::Value;

/// Extract the first string value from `v` across multiple possible field names.
/// Returns `None` if none of the keys hold a string.
pub fn first_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(s) = v.get(*key).and_then(|v| v.as_str()) {
            return Some(s);
        }
    }
    None
}

/// Determine the winning team's ID from match details.
/// Riot's V3/V4 API can use different field names and shapes.
pub fn get_winning_team(match_data: &Value) -> Option<String> {
    match_data["matchInfo"]["winningTeam"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            match_data["matchInfo"]["WinningTeam"]
                .as_str()
                .map(|s| s.to_string())
        })
        .or_else(|| {
            match_data["teams"].as_array().and_then(|teams| {
                teams
                    .iter()
                    .find(|t| t["won"].as_bool() == Some(true))
                    .and_then(|t| {
                        t["teamId"]
                            .as_str()
                            .or_else(|| t["teamID"].as_str())
                            .or_else(|| t["TeamID"].as_str())
                            .map(|s| s.to_string())
                    })
            })
        })
}

/// Compute the match score string (e.g. "13-5") from match details.
pub fn get_match_score(match_data: &Value) -> Option<String> {
    let teams = match_data["teams"].as_array()?;
    if teams.len() < 2 {
        return None;
    }
    let t0 = teams[0]["roundsWon"]
        .as_i64()
        .or_else(|| teams[0]["RoundsWon"].as_i64())
        .unwrap_or(0);
    let t1 = teams[1]["roundsWon"]
        .as_i64()
        .or_else(|| teams[1]["RoundsWon"].as_i64())
        .unwrap_or(0);
    Some(format!("{t0}-{t1}"))
}
