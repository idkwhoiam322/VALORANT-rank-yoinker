use std::path::PathBuf;
use std::time::Duration;

use secrecy::SecretString;

use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;
use crate::models::auth::{Entitlements, Lockfile, Region};

const LOCKFILE_PATH: &str = r"Riot Games\Riot Client\Config\lockfile";
const LOG_PATH: &str = r"VALORANT\Saved\Logs\ShooterGame.log";

pub(crate) fn get_lockfile_path() -> PathBuf {
    let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        std::env::var("APPDATA").unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local".into())
    });
    PathBuf::from(localappdata).join(LOCKFILE_PATH)
}

pub(crate) fn get_log_path() -> PathBuf {
    let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        std::env::var("APPDATA").unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local".into())
    });
    PathBuf::from(localappdata).join(LOG_PATH)
}

/// Path to `RiotClientInstalls.json`, which records the installed Riot Client
/// executable location. Mirrors the Python launcher
/// (`account_config.py:get_riot_client_path`).
fn get_riot_client_installs_path() -> PathBuf {
    let allusers = std::env::var("ALLUSERSPROFILE").unwrap_or_else(|_| r"C:\ProgramData".into());
    PathBuf::from(allusers).join(r"Riot Games\RiotClientInstalls.json")
}

/// Resolve the Riot Client executable path.
///
/// Reads `RiotClientInstalls.json` (a JSON dict of client key -> install path)
/// and returns the first value whose path exists; falls back to the common
/// fixed install locations. Returns `None` only if nothing is found.
fn get_riot_client_install_path() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = {
        let installs = get_riot_client_installs_path();
        let mut list = Vec::new();
        if let Ok(text) = std::fs::read_to_string(&installs) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(obj) = json.as_object() {
                    for (_k, v) in obj {
                        if let Some(s) = v.as_str() {
                            list.push(PathBuf::from(s));
                        }
                    }
                }
            }
        }
        list
    };

    for path in &candidates {
        if path.exists() {
            return Some(path.clone());
        }
    }

    let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let fallbacks = [
        PathBuf::from(localappdata).join(r"Riot Games\Riot Client\RiotClientServices.exe"),
        PathBuf::from(r"C:\Riot Games\Riot Client\RiotClientServices.exe"),
    ];
    for path in &fallbacks {
        if path.exists() {
            return Some(path.clone());
        }
    }
    None
}

/// Launch the Riot Client only (no `--launch-product` flag, so VALORANT is
/// not auto-started). The Riot Client alone creates the lockfile we wait on,
/// which is all vRY needs to read ranks/presence. Uses `cmd /c start` so the
/// child is detached from vRY — no extra crate required.
///
/// The presence of the lockfile is the only signal we use to decide whether to
/// launch. A backgrounded / signed-out RC keeps its lockfile, so we never
/// relaunch on top of an existing instance (avoids duplicate processes).
fn launch_riot_client() {
    if let Some(path) = get_riot_client_install_path() {
        let path_str = path.to_string_lossy().to_string();
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", &path_str])
            .spawn();
    }
}

/// Block until the Riot Client lockfile is present and parseable, launching the
/// Riot Client once if it is absent.
///
/// The lockfile's mere existence is a *free* on-disk `Path::exists()` check
/// (no API call, negligible CPU/disk), so polling it is safe to repeat. We use
/// `budget` as the outer timeout; inside it we poll existence every ~1s so we
/// react promptly once RC starts, then attempt to parse the lockfile.
///
/// Note: existence does NOT mean the local API is ready (e.g. a backgrounded RC
/// whose endpoints are not yet bound). Callers must still attempt the real
/// auth/connect and treat failure as "not ready yet", exactly like Python's
/// `get_headers()` retry-on-ConnectionError.
pub(crate) async fn ensure_lockfile_ready(budget: Duration) -> Option<Lockfile> {
    let path = get_lockfile_path();
    if path.exists() {
        if let Ok(lf) = parse_lockfile(&path) {
            return Some(lf);
        }
    }

    // Lockfile absent (RC fully closed) — launch once, then wait for it.
    launch_riot_client();

    let start = std::time::Instant::now();
    loop {
        if path.exists() {
            if let Ok(lf) = parse_lockfile(&path) {
                return Some(lf);
            }
        }
        if start.elapsed() >= budget {
            return None;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub(crate) fn parse_lockfile(path: &PathBuf) -> Result<Lockfile, ApiError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ApiError::Lockfile(format!("Cannot read lockfile: {}", e)))?;

    let parts: Vec<&str> = content.trim().split(':').collect();
    if parts.len() < 5 {
        return Err(ApiError::Lockfile(format!(
            "Invalid lockfile format: {}",
            content
        )));
    }

    Ok(Lockfile {
        name: parts[0].to_string(),
        pid: parts[1]
            .parse()
            .map_err(|_| ApiError::Lockfile("Invalid PID".into()))?,
        port: parts[2]
            .parse()
            .map_err(|_| ApiError::Lockfile("Invalid port".into()))?,
        password: parts[3].to_string(),
        protocol: parts[4].to_string(),
    })
}

pub(crate) fn parse_region_from_logs(path: &PathBuf) -> Result<Region, ApiError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ApiError::Auth(format!("Cannot read log file: {}", e)))?;

    let mut pd_region: Option<String> = None;
    let mut glz_host: Option<String> = None;
    let mut glz_shard: Option<String> = None;

    for line in content.lines().rev() {
        if line.contains(".a.pvp.net/account-xp/v1/") && pd_region.is_none() {
            if let Some(part) = line.split(".a.pvp.net/account-xp/v1/").next() {
                if let Some(region) = part.split('.').last() {
                    pd_region = Some(region.to_string());
                }
            }
        }
        if line.contains("https://glz") && glz_host.is_none() {
            if let Some(rest) = line.split("https://glz-").nth(1) {
                let parts: Vec<&str> = rest.split('.').collect();
                if parts.len() >= 2 {
                    glz_host = Some(parts[0].to_string());
                    glz_shard = Some(parts[1].to_string());
                }
            }
        }
        if pd_region.is_some() && glz_host.is_some() {
            break;
        }
    }

    match (pd_region, glz_host, glz_shard) {
        (Some(pd), Some(gh), Some(gs)) => {
            if pd == "pbe" {
                return Ok(Region::from_logs("na", "na-1", "na"));
            }
            Ok(Region::from_logs(&pd, &gh, &gs))
        }
        _ => Err(ApiError::Auth(
            "Could not determine region from logs".into(),
        )),
    }
}

fn parse_client_version(path: &PathBuf) -> Result<String, ApiError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ApiError::Auth(format!("Cannot read log file for version: {}", e)))?;

    for line in content.lines().rev() {
        if line.contains("CI server version:") {
            if let Some(version) = line.split("CI server version: ").nth(1) {
                return Ok(version.trim().to_string());
            }
        }
    }

    Ok("unknown".into())
}

pub(crate) async fn authenticate(
    client: &ApiClient,
    lockfile: &Lockfile,
) -> Result<(Entitlements, String), ApiError> {
    let port = lockfile.port;
    client.set_local_auth(lockfile.password.clone(), port);

    let json = {
        let mut retries = 0;
        loop {
            let response = client
                .fetch(UrlType::Local, endpoints::LOCAL_ENTITLEMENTS, &[], None)
                .await?;

            let status = response.status();
            let text = response.text().await.map_err(ApiError::Http)?;

            // Check if response indicates error (user not signed in)
            if status.is_client_error() {
                return Err(ApiError::Auth(format!(
                    "Riot client returned error (status {}): {}. Please sign in to Riot Client and restart vRY.",
                    status,
                    text
                )));
            }

            let json: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| ApiError::Auth(format!("JSON parse error: {}", e)))?;

            if json.get("message").and_then(|m| m.as_str())
                == Some("Entitlements token is not ready yet")
            {
                if retries >= 5 {
                    client.app_log(&format!(
                        "[AUTH] entitlements token not ready after {} retries - giving up",
                        retries
                    ));
                    return Err(ApiError::Auth(
                        "Entitlements token not ready after retries. Please sign in to Riot Client and restart vRY.".into()
                    ));
                }
                retries += 1;
                client.app_log(&format!(
                    "[AUTH] entitlements token not ready (attempt {}/5) - retrying in 1s",
                    retries
                ));
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            break json;
        }
    };

    // If we get here and json doesn't have accessToken, it's likely an error response
    if json.get("accessToken").is_none() {
        return Err(ApiError::Auth(
            "Entitlements token not available. Please sign in to Riot Client and restart vRY."
                .into(),
        ));
    }

    let entitlements = Entitlements {
        access_token: SecretString::from(
            json["accessToken"]
                .as_str()
                .ok_or_else(|| ApiError::Auth("Missing accessToken".into()))?
                .to_string(),
        ),
        token: SecretString::from(
            json["token"]
                .as_str()
                .ok_or_else(|| ApiError::Auth("Missing token".into()))?
                .to_string(),
        ),
        subject: json["subject"]
            .as_str()
            .ok_or_else(|| ApiError::Auth("Missing subject".into()))?
            .to_string(),
    };

    let log_path = get_log_path();
    let client_version = parse_client_version(&log_path).unwrap_or_else(|_| "unknown".into());

    Ok((entitlements, client_version))
}
