use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::{Client, ClientBuilder, Method, Response};
use reqwest::header::HeaderMap;
use thiserror::Error;

use crate::api::endpoints;
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

fn check_bad_claims(text: &str) -> Result<(), ApiError> {
    if text.contains("BAD_CLAIMS") {
        return Err(ApiError::BadClaims);
    }
    Ok(())
}

pub struct ApiClient {
    client: Client,
    local_client: Client,
    pd_url: Mutex<String>,
    glz_url: Mutex<String>,
    rate_limiters: [Mutex<RateLimiter>; 4],
    local_password: Mutex<String>,
    local_port: Mutex<u16>,
    logger: Mutex<Option<Arc<Logger>>>,
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
            pd_url: Mutex::new(pd_url),
            glz_url: Mutex::new(glz_url),
            rate_limiters: [
                Mutex::new(RateLimiter::new(8)),  // Pd
                Mutex::new(RateLimiter::new(5)),  // Glz
                Mutex::new(RateLimiter::new(20)), // Local
                Mutex::new(RateLimiter::new(5)),  // Custom
            ],
            local_password: Mutex::new(String::new()),
            local_port: Mutex::new(0),
            logger: Mutex::new(None),
        }

    }

    pub fn set_logger(&self, logger: Arc<Logger>) {
        *self.logger.lock().unwrap() = Some(logger);
    }

    pub fn update_urls(&self, pd_url: String, glz_url: String) {
        *self.pd_url.lock().unwrap() = pd_url;
        *self.glz_url.lock().unwrap() = glz_url;
    }

    pub fn set_local_auth(&self, password: String, port: u16) {
        *self.local_password.lock().unwrap() = password;
        *self.local_port.lock().unwrap() = port;
    }

    pub fn get_local_password(&self) -> String {
        self.local_password.lock().unwrap().clone()
    }

    fn app_log(&self, msg: &str) {
        if let Some(logger) = self.logger.lock().unwrap().as_ref() {
            logger.log(msg);
        }
    }

    /// Centralized cache-hit logging — mirrors `execute_request` so all cache
    /// hits go to both the `log` crate (env_logger) and the app backend log.
    /// `kind` identifies the cache (e.g. "rank", "stats", "match details",
    /// "match player", "names"). `key` is a short PUUID/match_id fragment.
    /// `ttl_remaining` optionally reports time left in the cache entry.
    pub(crate) fn cache_hit(
        &self,
        kind: &str,
        key: &str,
        ttl_remaining: Option<u64>,
    ) {
        let line = match ttl_remaining {
            Some(t) => format!("[CACHE] {kind} hit for {key} ({t}s remaining)"),
            None => format!("[CACHE] {kind} hit for {key}"),
        };
        log::info!("{}", line);
        self.app_log(&line);
    }

    /// Unified HTTP request execution with request/response logging.
    /// All public fetch methods route through this to ensure consistent logging.
    async fn execute_request(
        &self,
        method: Method,
        url: &str,
        headers: &HeaderMap,
        body: Option<serde_json::Value>,
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

    fn url_for(&self, url_type: UrlType, endpoint: &str) -> String {
        match url_type {
            UrlType::Pd => format!("{}{}", self.pd_url.lock().unwrap(), endpoint),
            UrlType::Glz => format!("{}{}", self.glz_url.lock().unwrap(), endpoint),
            UrlType::Local => {
                let port = *self.local_port.lock().unwrap();
                format!("https://127.0.0.1:{}{}", port, endpoint)
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
        self.fetch_with_method(url_type, endpoint, headers, body, None).await
    }

    pub async fn fetch_with_method(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
        body: Option<serde_json::Value>,
        method: Option<reqwest::Method>,
    ) -> Result<Response, ApiError> {
        let idx = Self::limiter_index(url_type);
        let wait = {
            let mut limiter = self.rate_limiters[idx].lock().unwrap();
            let wait = limiter.check_rate();
            limiter.record_request();
            wait
        };
        if let Some(delay) = wait {
            tokio::time::sleep(delay).await;
        }

        let url = self.url_for(url_type, endpoint);
        let http_method = method.unwrap_or_else(|| {
            match body {
                Some(_) => Method::POST,
                None => Method::GET,
            }
        });

        let mut header_map = HeaderMap::new();
        if url_type == UrlType::Local {
            let password = self.local_password.lock().unwrap().clone();
            let auth = format!(
                "Basic {}",
                base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    format!("riot:{}", password)
                )
            );
            header_map.insert("Authorization", match auth.parse::<reqwest::header::HeaderValue>() {
                Ok(v) => v,
                Err(_) => return Err(ApiError::Auth("Invalid local auth header".into())),
            });
        } else {
            for (key, value) in headers {
                let Ok(name) = key.as_str().parse::<reqwest::header::HeaderName>() else { continue; };
                let Ok(val) = value.parse::<reqwest::header::HeaderValue>() else { continue; };
                header_map.insert(name, val);
            }
        }

        let response = self.execute_request(http_method, &url, &header_map, body, url_type).await?;

        if response.status().as_u16() == 404 {
            return Err(ApiError::NotFound);
        }
        if response.status().as_u16() == 429 {
            tokio::time::sleep(Duration::from_secs(5)).await;
            return Err(ApiError::RateLimited);
        }
        if response.status().is_server_error() {
            return Err(ApiError::ServerError(response.text().await.unwrap_or_default()));
        }

        Ok(response)
    }

    pub async fn fetch_json<T: serde::de::DeserializeOwned>(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
    ) -> Result<T, ApiError> {
        let resp = self.fetch(url_type, endpoint, headers, None).await?;
        let text = resp.text().await.map_err(ApiError::Http)?;
        check_bad_claims(&text)?;
        serde_json::from_str(&text).map_err(|e| {
            ApiError::ServerError(format!("JSON parse error: {} - body: {}", e, text.chars().take(200).collect::<String>()))
        })
    }

    pub async fn fetch_json_with_body<T: serde::de::DeserializeOwned>(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
        body: serde_json::Value,
    ) -> Result<T, ApiError> {
        let resp = self.fetch(url_type, endpoint, headers, Some(body)).await?;
        let text = resp.text().await.map_err(ApiError::Http)?;
        check_bad_claims(&text)?;
        serde_json::from_str(&text).map_err(|e| {
            ApiError::ServerError(format!("JSON parse error: {} - body: {}", e, text.chars().take(200).collect::<String>()))
        })
    }

    /// Fetch JSON with configurable retry and validation.
    /// Retries up to `max_retries` times with `delay` between attempts.
    /// The `validate` closure determines if the response is acceptable;
    /// returns `Err` only after all retries are exhausted.
    pub async fn fetch_json_retry(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
        max_retries: u32,
        delay: Duration,
        validate: impl Fn(&serde_json::Value) -> bool,
    ) -> Result<serde_json::Value, ApiError> {
        let mut last_error = None;
        for attempt in 0..max_retries {
            let label = format!("{endpoint} (attempt {}/{})", attempt + 1, max_retries);
            match self.fetch_json::<serde_json::Value>(url_type, endpoint, headers).await {
                Ok(json) => {
                    if validate(&json) {
                        return Ok(json);
                    }
                    self.app_log(&format!("[API] retry {label}: validation failed — retrying"));
                }
                Err(e) => {
                    self.app_log(&format!("[API] retry {label}: {e}"));
                    last_error = Some(e);
                }
            }
            if attempt + 1 < max_retries {
                tokio::time::sleep(delay).await;
            }
        }
        self.app_log(&format!("[API] retry exhausted: {endpoint} after {max_retries} attempts"));
        Err(last_error.unwrap_or(ApiError::ServerError("max retries exhausted".into())))
    }

    pub async fn fetch_put_json_with_body<T: serde::de::DeserializeOwned>(
        &self,
        url_type: UrlType,
        endpoint: &str,
        headers: &[(String, String)],
        body: serde_json::Value,
    ) -> Result<T, ApiError> {
        let resp = self.fetch_with_method(url_type, endpoint, headers, Some(body), Some(reqwest::Method::PUT)).await?;
        let text = resp.text().await.map_err(ApiError::Http)?;
        check_bad_claims(&text)?;
        serde_json::from_str(&text).map_err(|e| {
            ApiError::ServerError(format!("JSON parse error: {} - body: {}", e, text.chars().take(200).collect::<String>()))
        })
    }

    pub async fn fetch_valorant_api<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
    ) -> Result<T, ApiError> {
        // Use Custom rate limiter slot (5 req/s) for valorant-api.com
        {
            let idx = Self::limiter_index(UrlType::Custom);
            let wait = {
                let mut limiter = self.rate_limiters[idx].lock().unwrap();
                let wait = limiter.check_rate();
                limiter.record_request();
                wait
            };
            if let Some(delay) = wait {
                tokio::time::sleep(delay).await;
            }
        }

        let url = format!("{}/{}", endpoints::VALORANT_API_BASE, endpoint);
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", "VRY/1.0".parse().unwrap());
        let resp = self.execute_request(Method::GET, &url, &headers, None, UrlType::Custom).await?;
        let status = resp.status();
        let text = resp.text().await.map_err(ApiError::Http)?;

        if status.as_u16() == 429 {
            return Err(ApiError::RateLimited);
        }
        if !status.is_success() {
            return Err(ApiError::ServerError(format!(
                "ValAPI HTTP {} -> {}: {}",
                status.as_u16(),
                endpoint,
                text.chars().take(200).collect::<String>()
            )));
        }

        serde_json::from_str(&text).map_err(|e| {
            ApiError::ServerError(format!("ValAPI JSON parse error: {} - body: {}", e, text.chars().take(200).collect::<String>()))
        })
    }
}
