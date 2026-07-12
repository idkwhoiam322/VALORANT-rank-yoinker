use std::path::PathBuf;
use std::time::Duration;

use crate::models::auth::{Entitlements, Lockfile, Region};
use crate::api::client::{ApiClient, ApiError, UrlType};
use crate::api::endpoints;

const LOCKFILE_PATH: &str = r"Riot Games\Riot Client\Config\lockfile";
const LOG_PATH: &str = r"VALORANT\Saved\Logs\ShooterGame.log";

pub fn get_lockfile_path() -> PathBuf {
    let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        std::env::var("APPDATA").unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local".into())
    });
    PathBuf::from(localappdata).join(LOCKFILE_PATH)
}

pub fn get_log_path() -> PathBuf {
    let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        std::env::var("APPDATA").unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local".into())
    });
    PathBuf::from(localappdata).join(LOG_PATH)
}

pub fn parse_lockfile(path: &PathBuf) -> Result<Lockfile, ApiError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ApiError::Lockfile(format!("Cannot read lockfile: {}", e)))?;

    let parts: Vec<&str> = content.trim().split(':').collect();
    if parts.len() < 5 {
        return Err(ApiError::Lockfile(format!("Invalid lockfile format: {}", content)));
    }

    Ok(Lockfile {
        name: parts[0].to_string(),
        pid: parts[1].parse().map_err(|_| ApiError::Lockfile("Invalid PID".into()))?,
        port: parts[2].parse().map_err(|_| ApiError::Lockfile("Invalid port".into()))?,
        password: parts[3].to_string(),
        protocol: parts[4].to_string(),
    })
}

pub fn parse_region_from_logs(path: &PathBuf) -> Result<Region, ApiError> {
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
        _ => Err(ApiError::Auth("Could not determine region from logs".into())),
    }
}

pub fn parse_client_version(path: &PathBuf) -> Result<String, ApiError> {
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

pub async fn authenticate(client: &ApiClient, lockfile: &Lockfile) -> Result<(Entitlements, String), ApiError> {
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

            if json.get("message").and_then(|m| m.as_str()) == Some("Entitlements token is not ready yet") {
                if retries >= 5 {
                    return Err(ApiError::Auth(
                        "Entitlements token not ready after retries. Please sign in to Riot Client and restart vRY.".into()
                    ));
                }
                retries += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            break json;
        }
    };

    // If we get here and json doesn't have accessToken, it's likely an error response
    if json.get("accessToken").is_none() {
        return Err(ApiError::Auth("Entitlements token not available. Please sign in to Riot Client and restart vRY.".into()));
    }

    let entitlements = Entitlements {
        access_token: json["accessToken"]
            .as_str()
            .ok_or_else(|| ApiError::Auth("Missing accessToken".into()))?
            .to_string(),
        token: json["token"]
            .as_str()
            .ok_or_else(|| ApiError::Auth("Missing token".into()))?
            .to_string(),
        subject: json["subject"]
            .as_str()
            .ok_or_else(|| ApiError::Auth("Missing subject".into()))?
            .to_string(),
    };

    let log_path = get_log_path();
    let client_version = parse_client_version(&log_path).unwrap_or_else(|_| "unknown".into());

    Ok((entitlements, client_version))
}
