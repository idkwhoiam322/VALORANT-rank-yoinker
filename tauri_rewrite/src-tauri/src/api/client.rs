use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;
use reqwest::{Client, ClientBuilder, Method, Response};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

use crate::api::endpoints;
use crate::models::auth::Entitlements;
use crate::services::logging::Logger;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Rate limited")]
    RateLimited,
    #[error("Bad claims - need reauth")]
    BadClaims,
    #[error("Not found")]
    NotFound,
    #[error("Server error: {0}")]
    ServerError(String),
    #[error("Lockfile error: {0}")]
    Lockfile(String),
    #[error("Auth error: {0}")]
    Auth(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlType {
    Pd,
    Glz,
    Local,
    Custom,
}

struct RateLimiter {
    history: VecDeque<Instant>,
    max_per_second: usize,
}

impl RateLimiter {
    fn new(max_per_second: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_per_second),
            max_per_second,
        }
    }

    fn check_rate(&mut self) -> Option<Duration> {
        let now = Instant::now();
        while let Some(&t) = self.history.front() {
            if now.duration_since(t) > Duration::from_secs(1) {
                self.history.pop_front();
            } else {
                break;
            }
        }
        if self.history.len() >= self.max_per_second {
            if let Some(&oldest) = self.history.front() {
                let wait = Duration::from_secs(1).saturating_sub(now.duration_since(oldest));
                return Some(wait);
            }
        }
        None
    }

    fn record_request(&mut self) {
        self.history.push_back(Instant::now());
    }
}

pub struct ApiClient {
    client: Client,
    local_client: Client,
    pd_base: Mutex<Arc<str>>,
    glz_base: Mutex<Arc<str>>,
    rate_limiters: [Mutex<RateLimiter>; 4],
    rate_cooldown: [Mutex<Option<Instant>>; 4],
    local_password: Mutex<SecretString>,
    local_auth_header: Mutex<Option<SecretString>>,
    local_base: Mutex<Arc<str>>,
    logger: Mutex<Option<Arc<Logger>>>,
    entitlements: Arc<Mutex<Option<Entitlements>>>,
    client_version: Mutex<String>,
    local_api_dead: AtomicBool,
}

impl ApiClient {
    pub fn new(pd_url: String, glz_url: String) -> Self {
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("Failed to create HTTP client");

        let local_client = ClientBuilder::new()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(15))
            .build()
            .expect("Failed to create local HTTP client");

        Self {
            client,
            local_client,
            pd_base: Mutex::new(pd_url.into()),
            glz_base: Mutex::new(glz_url.into()),
            rate_limiters: [
                Mutex::new(RateLimiter::new(8)),  // Pd
                Mutex::new(RateLimiter::new(5)),  // Glz
                Mutex::new(RateLimiter::new(20)), // Local
                Mutex::new(RateLimiter::new(5)),  // Custom
            ],
            rate_cooldown: [
                Mutex::new(None), // Pd
                Mutex::new(None), // Glz
                Mutex::new(None), // Local
                Mutex::new(None), // Custom
            ],
            local_password: Mutex::new(SecretString::from(String::new())),
            local_auth_header: Mutex::new(None),
            local_base: Mutex::new("https://127.0.0.1:0".into()),
            logger: Mutex::new(None),
            entitlements: Arc::new(Mutex::new(None)),
            client_version: Mutex::new(String::new()),
            local_api_dead: AtomicBool::new(false),
        }
    }

    pub fn set_logger(&self, logger: Arc<Logger>) {
        *self.logger.lock().unwrap() = Some(logger);
    }

    pub fn update_urls(&self, pd_url: String, glz_url: String) {
        *self.pd_base.lock().unwrap() = pd_url.into();
        *self.glz_base.lock().unwrap() = glz_url.into();
    }

    pub fn set_local_auth(&self, password: String, port: u16) {
        let header = format!(
            "Basic {}",
            base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                format!("riot:{password}")
            )
        );
        *self.local_password.lock().unwrap() = SecretString::from(password);
        *self.local_auth_header.lock().unwrap() = Some(SecretString::from(header));
        *self.local_base.lock().unwrap() = format!("https://127.0.0.1:{}", port).into();
    }

    pub fn get_local_password(&self) -> SecretString {
        self.local_password.lock().unwrap().clone()
    }

    pub fn entitlements_arc(&self) -> Arc<Mutex<Option<Entitlements>>> {
        self.entitlements.clone()
    }

    pub fn set_client_version(&self, version: &str) {
        *self.client_version.lock().unwrap() = version.to_string();
    }

    pub fn get_client_version(&self) -> String {
        self.client_version.lock().unwrap().clone()
    }

    pub(crate) fn is_local_api_dead(&self) -> bool {
        self.local_api_dead.load(Ordering::Relaxed)
    }

    pub(crate) fn clear_local_api_dead(&self) {
        self.local_api_dead.store(false, Ordering::Relaxed);
    }

    pub fn get_entitlements(&self) -> Option<Entitlements> {
        self.entitlements.lock().unwrap().clone()
    }

    /// Refresh both entitlements and client_version from local Riot client.
    /// Retries up to 3 times with 1s delay between attempts.
    pub(crate) async fn refresh_entitlements_with_retry(&self) -> Result<(), ApiError> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_secs(1);

        for attempt in 0..MAX_RETRIES {
            self.app_log(&format!(
                "[AUTH] refreshing entitlements + version from local Riot client (attempt {}/{})",
                attempt + 1,
                MAX_RETRIES
            ));

            // Refresh client_version from logs first
            if let Err(e) = self.refresh_client_version().await {
                self.app_log(&format!("[AUTH] failed to refresh client_version: {e}"));
            }

            // Then refresh entitlements
            match self.fetch_local_entitlements().await {
                Ok(fresh) => {
                    *self.entitlements.lock().unwrap() = Some(fresh);
                    self.app_log("[AUTH] entitlements refreshed successfully");
                    return Ok(());
                }
                Err(e) => {
                    self.app_log(&format!("[AUTH] refresh failed: {e}"));
                    if attempt + 1 < MAX_RETRIES {
                        tokio::time::sleep(RETRY_DELAY).await;
                    }
                }
            }
        }
        Err(ApiError::Auth(
            "Entitlements refresh failed after retries".into(),
        ))
    }

    /// Refresh client_version by parsing ShooterGame.log
    async fn refresh_client_version(&self) -> Result<(), ApiError> {
        use std::fs;
        use std::path::PathBuf;

        let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            std::env::var("APPDATA").unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local".into())
        });
        let log_path = PathBuf::from(localappdata).join(r"VALORANT\Saved\Logs\ShooterGame.log");

        let content = fs::read_to_string(&log_path)
            .map_err(|e| ApiError::Auth(format!("Cannot read log file for version: {}", e)))?;

        for line in content.lines().rev() {
            if line.contains("CI server version:") {
                if let Some(version) = line.split("CI server version: ").nth(1) {
                    let version = version.trim().to_string();
                    *self.client_version.lock().unwrap() = version.clone();
                    self.app_log(&format!("[AUTH] client_version refreshed: {version}"));
                    return Ok(());
                }
            }
        }
        Err(ApiError::Auth(
            "Could not determine client version from logs".into(),
        ))
    }

    /// Fetch fresh entitlements from local Riot client endpoint
    async fn fetch_local_entitlements(&self) -> Result<Entitlements, ApiError> {
        let response = self
            .fetch(UrlType::Local, endpoints::LOCAL_ENTITLEMENTS, &[], None)
            .await?;
        let status = response.status();
        let text = response.text().await.map_err(ApiError::Http)?;

        if status.is_client_error() {
            return Err(ApiError::Auth(format!(
                "Riot client returned error: {}",
                text
            )));
        }

        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| ApiError::Auth(format!("JSON parse error: {}", e)))?;

        if json.get("accessToken").is_none() {
            return Err(ApiError::Auth("Entitlements token not available".into()));
        }

        Ok(Entitlements {
            access_token: json["accessToken"]
                .as_str()
                .ok_or_else(|| ApiError::Auth("Missing accessToken".into()))?
                .to_string()
                .into(),
            token: json["token"]
                .as_str()
                .ok_or_else(|| ApiError::Auth("Missing token".into()))?
                .to_string()
                .into(),
            subject: json["subject"]
                .as_str()
                .ok_or_else(|| ApiError::Auth("Missing subject".into()))?
                .to_string(),
        })
    }

    pub(crate) fn app_log(&self, msg: &str) {
        if let Some(logger) = self.logger.lock().unwrap().as_ref() {
            logger.log(msg);
        }
    }

    /// Centralized cache-hit logging - mirrors `execute_request` so all cache
    /// hits go to both the `log` crate (env_logger) and the app backend log.
    /// `kind` identifies the cache (e.g. "rank", "stats", "match details",
    /// "match player", "names"). `key` is a short PUUID/match_id fragment.
    /// `ttl_remaining` optionally reports time left in the cache entry.
    pub(crate) fn cache_hit(&self, kind: &str, key: &str, ttl_remaining: Option<u64>) {
        let line = match ttl_remaining {
            Some(t) => format!("[CACHE] {kind} hit for {key} ({t}s remaining)"),
            None => format!("[CACHE] {kind} hit for {key}"),
        };
        log::info!("{}", line);
        self.app_log(&line);
    }

    /// Determine delay before retrying a 429 response.
    /// Uses `Retry-After` header if present and non-zero; otherwise falls back to
    /// exponential backoff (5s * 2^attempt, max 60s). A `Retry-After: 0` (or missing)
    /// header no longer means "retry immediately" — it would otherwise busy-loop while
    /// the server is still throttling.
    fn retry_after_delay(response: &Response, attempt: usize) -> Duration {
        let header_secs = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        if let Some(secs) = header_secs {
            if secs > 0 {
                return Duration::from_secs(secs);
            }
        }
        Duration::from_secs((5u64 * 2u64.pow(attempt as u32)).min(60))
    }

    /// Unified HTTP request execution with request/response logging.
    /// All public fetch methods route through this to ensure consistent logging.
    async fn execute_request(
        &self,
        method: Method,
        url: &str,
        headers: &HeaderMap,
        body: Option<&serde_json::Value>,
        url_type: UrlType,
    ) -> Result<Response, ApiError> {
        let start = Instant::now();
        let log_line = format!("[API] -> {} {}", method, url);
        log::info!("{}", log_line);
        self.app_log(&log_line);

        let req_client = match url_type {
            UrlType::Local => &self.local_client,
            UrlType::Pd | UrlType::Glz | UrlType::Custom => &self.client,
        };
        let mut req = req_client.request(method.clone(), url);
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req.send().await.map_err(ApiError::Http)?;
        let elapsed = start.elapsed();
        let log_line = format!(
            "[API] <- {} {} {} ({:?})",
            resp.status().as_u16(),
            method,
            url,
            elapsed
        );
        log::info!("{}", log_line);
        self.app_log(&log_line);

        Ok(resp)
    }

    fn limiter_index(url_type: UrlType) -> usize {
        match url_type {
            UrlType::Pd => 0,
            UrlType::Glz => 1,
            UrlType::Local => 2,
            UrlType::Custom => 3,
        }
    }

    /// Remaining cooldown for `url_type`, if a 429 with `Retry-After` was seen
    /// recently. `Local` (localhost Riot client) is never cooled down.
    fn cooldown_remaining(&self, url_type: UrlType) -> Option<Duration> {
        if url_type == UrlType::Local {
            return None;
        }
        let guard = self.rate_cooldown[Self::limiter_index(url_type)]
            .lock()
            .unwrap();
        match *guard {
            Some(expiry) => {
                let rem = expiry.saturating_duration_since(Instant::now());
                if rem.is_zero() {
                    None
                } else {
                    Some(rem)
                }
            }
            None => None,
        }
    }

    /// Record a cooldown for `url_type` so all subsequent calls to that host back
    /// off together. `Local` and zero/negative durations are ignored. Extends any
    /// existing cooldown (max of old and new expiry) rather than overwriting, so a
    /// shorter backoff from a later 429 can't shrink an already-established window.
    fn set_cooldown(&self, url_type: UrlType, dur: Duration) {
        if url_type == UrlType::Local || dur.is_zero() {
            return;
        }
        let new_expiry = Instant::now() + dur;
        let mut guard = self.rate_cooldown[Self::limiter_index(url_type)]
            .lock()
            .unwrap();
        *guard = Some(guard.map_or(new_expiry, |e| e.max(new_expiry)));
    }

    fn url_for(&self, url_type: UrlType, endpoint: &str) -> String {
        // Clone the Arc (a cheap atomic refcount bump) while the lock is held
        // for that instant only, then build the final URL string after the
        // guard has already been dropped, so the mutex is never held across
        // the string formatting work.
        match url_type {
            UrlType::Pd => {
                let base = self.pd_base.lock().unwrap().clone();
                format!("{}{}", base, endpoint)
            }
            UrlType::Glz => {
                let base = self.glz_base.lock().unwrap().clone();
                format!("{}{}", base, endpoint)
            }
            UrlType::Local => {
                let base = self.local_base.lock().unwrap().clone();
                format!("{}{}", base, endpoint)
            }
            UrlType::Custom => endpoint.to_string(),
        }
    }

    pub async fn fetch(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
        body: Option<serde_json::Value>,
    ) -> Result<Response, ApiError> {
        self.fetch_with_method(url_type, endpoint, headers, body, None)
            .await
    }

    pub async fn fetch_with_method(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
        body: Option<serde_json::Value>,
        method: Option<reqwest::Method>,
    ) -> Result<Response, ApiError> {
        const MAX_429_RETRIES: usize = 5;

        let url = self.url_for(url_type, endpoint);
        let http_method = method.unwrap_or_else(|| match body {
            Some(_) => Method::POST,
            None => Method::GET,
        });

        let mut header_map = HeaderMap::new();
        if url_type == UrlType::Local {
            let auth = {
                let guard = self.local_auth_header.lock().unwrap();
                if let Some(h) = guard.as_ref() {
                    h.expose_secret().to_string()
                } else {
                    drop(guard);
                    let pw = self
                        .local_password
                        .lock()
                        .unwrap()
                        .clone()
                        .expose_secret()
                        .to_string();
                    format!(
                        "Basic {}",
                        base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            format!("riot:{pw}")
                        )
                    )
                }
            };
            header_map.insert(
                "Authorization",
                match auth.parse::<reqwest::header::HeaderValue>() {
                    Ok(v) => v,
                    Err(_) => return Err(ApiError::Auth("Invalid local auth header".into())),
                },
            );
        } else {
            for (key, value) in headers {
                let Ok(name) = key.as_str().parse::<reqwest::header::HeaderName>() else {
                    continue;
                };
                let Ok(val) = value.parse::<reqwest::header::HeaderValue>() else {
                    continue;
                };
                header_map.insert(name, val);
            }
        }

        for attempt in 0..MAX_429_RETRIES {
            let idx = Self::limiter_index(url_type);
            // Shared per-host cooldown: once any call to this host is told to back
            // off (429 + Retry-After), every subsequent call to the same host waits
            // out the remainder together instead of each rediscovering the limit.
            if let Some(rem) = self.cooldown_remaining(url_type) {
                self.app_log(&format!(
                    "[API] rate limited ({url_type:?}) - cooling down {rem:?}"
                ));
                tokio::time::sleep(rem).await;
            }
            let wait = {
                let mut limiter = self.rate_limiters[idx].lock().unwrap();
                limiter.check_rate()
            };
            if let Some(delay) = wait {
                self.app_log(&format!(
                    "[API] rate limited ({url_type:?}) - sleeping {delay:?}"
                ));
                tokio::time::sleep(delay).await;
            }

            let response = self
                .execute_request(
                    http_method.clone(),
                    &url,
                    &header_map,
                    body.as_ref(),
                    url_type,
                )
                .await?;

            if response.status().as_u16() == 404 {
                return Err(ApiError::NotFound);
            }
            if response.status().as_u16() == 429 {
                let delay = Self::retry_after_delay(&response, attempt);
                self.set_cooldown(url_type, delay);
                self.app_log(&format!("[API] 429 Too Many Requests ({url_type:?} {endpoint}) - retry {}/{} sleeping {:?}", attempt + 1, MAX_429_RETRIES, delay));
                tokio::time::sleep(delay).await;
                continue;
            }
            if url_type == UrlType::Local && response.status().as_u16() == 503 {
                self.local_api_dead.store(true, Ordering::Relaxed);
                return Err(ApiError::ServerError(
                    response.text().await.unwrap_or_default(),
                ));
            }
            if response.status().is_server_error() {
                return Err(ApiError::ServerError(
                    response.text().await.unwrap_or_default(),
                ));
            }

            // Only count successful requests against the per-second limiter; 429s
            // would otherwise inflate the window and tighten throttling further.
            let mut limiter = self.rate_limiters[idx].lock().unwrap();
            limiter.record_request();
            return Ok(response);
        }

        Err(ApiError::RateLimited)
    }

    pub async fn fetch_json<T: serde::de::DeserializeOwned>(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
    ) -> Result<T, ApiError> {
        self.fetch_json_with_reauth(url_type, endpoint, headers, None, None)
            .await
    }

    pub async fn fetch_json_with_body<T: serde::de::DeserializeOwned>(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
        body: serde_json::Value,
    ) -> Result<T, ApiError> {
        self.fetch_json_with_reauth(url_type, endpoint, headers, Some(&body), None)
            .await
    }

    /// Internal fetch with centralized automatic re-auth on BAD_CLAIMS.
    /// On first BadClaims, refreshes entitlements + client_version from local Riot client,
    /// rebuilds headers from refreshed values, and retries once.
    async fn fetch_json_with_reauth<T: serde::de::DeserializeOwned>(
        &self,
        url_type: UrlType,
        endpoint: &str,
        initial_headers: &[(String, String)],
        body: Option<&serde_json::Value>,
        method: Option<Method>,
    ) -> Result<T, ApiError> {
        let mut headers = initial_headers.to_vec();
        let mut did_refresh = false;

        loop {
            let resp = self
                .fetch_with_headers(url_type, endpoint, &headers, body, method.clone())
                .await?;
            let text = resp.text().await.map_err(ApiError::Http)?;

            // Check for BAD_CLAIMS in response
            if text.contains("BAD_CLAIMS") {
                if did_refresh {
                    // Already refreshed once, don't retry again
                    return Err(ApiError::BadClaims);
                }
                did_refresh = true;
                self.app_log(&format!(
                    "[API] {endpoint}: BAD_CLAIMS received - refreshing entitlements + version"
                ));

                // Refresh both entitlements and client_version
                if let Err(e) = self.refresh_entitlements_with_retry().await {
                    self.app_log(&format!("[API] re-auth failed: {e}"));
                    return Err(ApiError::BadClaims);
                }

                // Rebuild headers from freshly refreshed entitlements + client_version
                if let Some(fresh_entitlements) = self.get_entitlements() {
                    let cv = self.get_client_version();
                    headers = fresh_entitlements.build_headers(&cv);
                    self.app_log(
                        "[API] entitlements + version refreshed - retrying with fresh headers",
                    );
                    continue;
                } else {
                    return Err(ApiError::Auth(
                        "No entitlements available after refresh".into(),
                    ));
                }
            }

            return serde_json::from_str(&text).map_err(|e| {
                ApiError::ServerError(format!(
                    "JSON parse error: {} - body: {}",
                    e,
                    text.chars().take(200).collect::<String>()
                ))
            });
        }
    }

    /// Fetch with pre-built headers and optional body/method
    async fn fetch_with_headers(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
        body: Option<&serde_json::Value>,
        method: Option<Method>,
    ) -> Result<Response, ApiError> {
        const MAX_429_RETRIES: usize = 5;

        let url = self.url_for(url_type, endpoint);
        let http_method = method.unwrap_or_else(|| match body {
            Some(_) => Method::POST,
            None => Method::GET,
        });

        let mut header_map = HeaderMap::new();
        if url_type == UrlType::Local {
            let auth = {
                let guard = self.local_auth_header.lock().unwrap();
                if let Some(h) = guard.as_ref() {
                    h.expose_secret().to_string()
                } else {
                    drop(guard);
                    let pw = self
                        .local_password
                        .lock()
                        .unwrap()
                        .clone()
                        .expose_secret()
                        .to_string();
                    format!(
                        "Basic {}",
                        base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            format!("riot:{pw}")
                        )
                    )
                }
            };
            header_map.insert(
                "Authorization",
                match auth.parse::<reqwest::header::HeaderValue>() {
                    Ok(v) => v,
                    Err(_) => return Err(ApiError::Auth("Invalid local auth header".into())),
                },
            );
        } else {
            for (key, value) in headers {
                let Ok(name) = key.as_str().parse::<reqwest::header::HeaderName>() else {
                    continue;
                };
                let Ok(val) = value.parse::<reqwest::header::HeaderValue>() else {
                    continue;
                };
                header_map.insert(name, val);
            }
        }

        for attempt in 0..MAX_429_RETRIES {
            let idx = Self::limiter_index(url_type);
            // Shared per-host cooldown: once any call to this host is told to back
            // off (429 + Retry-After), every subsequent call to the same host waits
            // out the remainder together instead of each rediscovering the limit.
            if let Some(rem) = self.cooldown_remaining(url_type) {
                self.app_log(&format!(
                    "[API] rate limited ({url_type:?}) - cooling down {rem:?}"
                ));
                tokio::time::sleep(rem).await;
            }
            let wait = {
                let mut limiter = self.rate_limiters[idx].lock().unwrap();
                limiter.check_rate()
            };
            if let Some(delay) = wait {
                self.app_log(&format!(
                    "[API] rate limited ({url_type:?}) - sleeping {delay:?}"
                ));
                tokio::time::sleep(delay).await;
            }

            let response = self
                .execute_request(http_method.clone(), &url, &header_map, body, url_type)
                .await?;

            if response.status().as_u16() == 404 {
                return Err(ApiError::NotFound);
            }
            if response.status().as_u16() == 429 {
                let delay = Self::retry_after_delay(&response, attempt);
                self.set_cooldown(url_type, delay);
                self.app_log(&format!("[API] 429 Too Many Requests ({url_type:?} {endpoint}) - retry {}/{} sleeping {:?}", attempt + 1, MAX_429_RETRIES, delay));
                tokio::time::sleep(delay).await;
                continue;
            }
            if url_type == UrlType::Local && response.status().as_u16() == 503 {
                self.local_api_dead.store(true, Ordering::Relaxed);
                return Err(ApiError::ServerError(
                    response.text().await.unwrap_or_default(),
                ));
            }
            if response.status().is_server_error() {
                return Err(ApiError::ServerError(
                    response.text().await.unwrap_or_default(),
                ));
            }

            // Only count successful requests against the per-second limiter; 429s
            // would otherwise inflate the window and tighten throttling further.
            let mut limiter = self.rate_limiters[idx].lock().unwrap();
            limiter.record_request();
            return Ok(response);
        }

        Err(ApiError::RateLimited)
    }

    /// Fetch JSON with configurable retry, validation, and automatic re-auth.
    /// Retries up to `max_retries` times with `delay` between attempts.
    /// Uses centralized reauth in `fetch_json` which handles BAD_CLAIMS
    /// by refreshing entitlements + client_version and retrying once.
    /// The `validate` closure determines if the response is acceptable;
    /// returns `Err` only after all retries are exhausted.
    pub async fn fetch_json_retry(
        &self,
        url_type: UrlType,
        endpoint: &str,
        entitlements: &Entitlements,
        client_version: &str,
        max_retries: u32,
        delay: Duration,
        validate: impl Fn(&serde_json::Value) -> bool,
    ) -> Result<serde_json::Value, ApiError> {
        let mut last_error = None;
        let mut current_entitlements = entitlements.clone();
        let mut current_cv = client_version.to_string();

        for attempt in 0..max_retries {
            let headers = current_entitlements.build_headers(&current_cv);
            let label = format!("{endpoint} (attempt {}/{})", attempt + 1, max_retries);

            match self
                .fetch_json::<serde_json::Value>(url_type, endpoint, &headers)
                .await
            {
                Ok(json) => {
                    if validate(&json) {
                        return Ok(json);
                    }
                    self.app_log(&format!(
                        "[API] retry {label}: validation failed - retrying"
                    ));
                }
                Err(e) => {
                    if matches!(&e, ApiError::NotFound) {
                        return Err(e);
                    }
                    self.app_log(&format!("[API] retry {label}: {e}"));
                    last_error = Some(e);
                }
            }

            // On BadClaims, the centralized reauth in fetch_json already tried once.
            // Rebuild headers from potentially refreshed entitlements for next attempt.
            if let Some(fresh) = self.get_entitlements() {
                current_entitlements = fresh;
                current_cv = self.get_client_version();
            }

            if attempt + 1 < max_retries {
                tokio::time::sleep(delay).await;
            }
        }

        self.app_log(&format!(
            "[API] retry exhausted: {endpoint} after {max_retries} attempts"
        ));
        Err(last_error.unwrap_or(ApiError::ServerError("max retries exhausted".into())))
    }

    /// Typed variant of `fetch_json_retry`: validates the raw JSON, then
    /// deserializes to `T`.  Saves callers from having to call
    /// `serde_json::from_value` themselves.
    pub async fn fetch_json_retry_typed<T: serde::de::DeserializeOwned>(
        &self,
        url_type: UrlType,
        endpoint: &str,
        entitlements: &Entitlements,
        client_version: &str,
        max_retries: u32,
        delay: Duration,
        validate: impl Fn(&serde_json::Value) -> bool,
    ) -> Result<T, ApiError> {
        let json = self
            .fetch_json_retry(
                url_type,
                endpoint,
                entitlements,
                client_version,
                max_retries,
                delay,
                validate,
            )
            .await?;
        serde_json::from_value(json).map_err(|e| {
            ApiError::ServerError(format!("JSON parse error: {} (endpoint: {})", e, endpoint))
        })
    }

    /// Retry + validation variant for requests with a body (PUT/POST).
    /// Same retry/validate semantics as `fetch_json_retry_typed` but passes
    /// `body` and `method` through to `fetch_json_with_reauth`.
    pub async fn fetch_json_retry_with_body<T: serde::de::DeserializeOwned>(
        &self,
        url_type: UrlType,
        endpoint: &str,
        entitlements: &Entitlements,
        client_version: &str,
        body: serde_json::Value,
        method: Option<Method>,
        max_retries: u32,
        delay: Duration,
        validate: impl Fn(&serde_json::Value) -> bool,
    ) -> Result<T, ApiError> {
        let mut last_error = None;
        let mut current_entitlements = entitlements.clone();
        let mut current_cv = client_version.to_string();

        for attempt in 0..max_retries {
            let headers = current_entitlements.build_headers(&current_cv);
            let label = format!("{endpoint} (attempt {}/{})", attempt + 1, max_retries);

            match self
                .fetch_json_with_reauth::<serde_json::Value>(
                    url_type,
                    endpoint,
                    &headers,
                    Some(&body),
                    method.clone(),
                )
                .await
            {
                Ok(json) => {
                    if validate(&json) {
                        match serde_json::from_value::<T>(json) {
                            Ok(v) => return Ok(v),
                            Err(e) => {
                                self.app_log(&format!(
                                    "[API] retry {label}: JSON parse error: {e}"
                                ));
                                last_error =
                                    Some(ApiError::ServerError(format!("JSON parse error: {e}")));
                            }
                        }
                    } else {
                        self.app_log(&format!(
                            "[API] retry {label}: validation failed - retrying"
                        ));
                    }
                }
                Err(e) => {
                    if matches!(&e, ApiError::NotFound) {
                        return Err(e);
                    }
                    self.app_log(&format!("[API] retry {label}: {e}"));
                    last_error = Some(e);
                }
            }

            if let Some(fresh) = self.get_entitlements() {
                current_entitlements = fresh;
                current_cv = self.get_client_version();
            }

            if attempt + 1 < max_retries {
                tokio::time::sleep(delay).await;
            }
        }

        self.app_log(&format!(
            "[API] retry exhausted: {endpoint} after {max_retries} attempts"
        ));
        Err(last_error.unwrap_or(ApiError::ServerError("max retries exhausted".into())))
    }

    pub async fn fetch_valorant_api<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
    ) -> Result<T, ApiError> {
        const MAX_429_RETRIES: usize = 5;

        let url = format!("{}/{}", endpoints::VALORANT_API_BASE, endpoint);
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", "VRY/1.0".parse().unwrap());

        for attempt in 0..MAX_429_RETRIES {
            // Use Custom rate limiter slot (5 req/s) for valorant-api.com.
            // Shared per-host cooldown so a 429 on one ValAPI call backs off all
            // subsequent ValAPI calls together.
            let idx = Self::limiter_index(UrlType::Custom);
            if let Some(rem) = self.cooldown_remaining(UrlType::Custom) {
                self.app_log(&format!(
                    "[API] rate limited (ValAPI) - cooling down {rem:?}"
                ));
                tokio::time::sleep(rem).await;
            }
            {
                let wait = {
                    let mut limiter = self.rate_limiters[idx].lock().unwrap();
                    limiter.check_rate()
                };
                if let Some(delay) = wait {
                    self.app_log(&format!("[API] rate limited (ValAPI) - sleeping {delay:?}"));
                    tokio::time::sleep(delay).await;
                }
            }

            let resp = self
                .execute_request(Method::GET, &url, &headers, None, UrlType::Custom)
                .await?;
            let status = resp.status();

            if status.as_u16() == 429 {
                let delay = Self::retry_after_delay(&resp, attempt);
                self.set_cooldown(UrlType::Custom, delay);
                self.app_log(&format!(
                    "[API] 429 Too Many Requests (ValAPI {endpoint}) - retry {}/{} sleeping {:?}",
                    attempt + 1,
                    MAX_429_RETRIES,
                    delay
                ));
                tokio::time::sleep(delay).await;
                continue;
            }

            let text = resp.text().await.map_err(ApiError::Http)?;

            if !status.is_success() {
                return Err(ApiError::ServerError(format!(
                    "ValAPI HTTP {} -> {}: {}",
                    status.as_u16(),
                    endpoint,
                    text.chars().take(200).collect::<String>()
                )));
            }

            // Only count successful requests against the per-second limiter.
            let mut limiter = self.rate_limiters[idx].lock().unwrap();
            limiter.record_request();
            return serde_json::from_str(&text).map_err(|e| {
                ApiError::ServerError(format!(
                    "ValAPI JSON parse error: {} - body: {}",
                    e,
                    text.chars().take(200).collect::<String>()
                ))
            });
        }

        Err(ApiError::RateLimited)
    }
}

/// Produce a stable, non-reversible short identifier for logging so that
/// PUUIDs / match_ids are never written to logs in a recoverable form.
///
/// Uses FNV-1a over the full id and renders 16 hex chars. The same input always
/// maps to the same output, so log lines for one player/match stay correlatable,
/// but the original identifier cannot be recovered from the hash.
pub(crate) fn anon_id(id: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in id.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
