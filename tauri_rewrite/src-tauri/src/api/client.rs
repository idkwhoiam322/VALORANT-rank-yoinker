use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::{Client, ClientBuilder, Response};
use thiserror::Error;

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
    pd_url: Mutex<String>,
    glz_url: Mutex<String>,
    rate_limiters: Mutex<[RateLimiter; 4]>,
    local_password: Mutex<String>,
    local_port: Mutex<u16>,
}

impl ApiClient {
    pub fn new(pd_url: String, glz_url: String) -> Self {
        let client = ClientBuilder::new()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(15))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            pd_url: Mutex::new(pd_url),
            glz_url: Mutex::new(glz_url),
            rate_limiters: Mutex::new([
                RateLimiter::new(8),  // Pd
                RateLimiter::new(5),  // Glz
                RateLimiter::new(20), // Local
                RateLimiter::new(5),  // Custom
            ]),
            local_password: Mutex::new(String::new()),
            local_port: Mutex::new(0),
        }
    }

    pub fn update_urls(&self, pd_url: String, glz_url: String) {
        *self.pd_url.lock().unwrap() = pd_url;
        *self.glz_url.lock().unwrap() = glz_url;
    }

    pub fn set_local_auth(&self, password: String, port: u16) {
        *self.local_password.lock().unwrap() = password;
        *self.local_port.lock().unwrap() = port;
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
        let (wait,) = {
            let mut limiters = self.rate_limiters.lock().unwrap();
            let idx = Self::limiter_index(url_type);
            let wait = limiters[idx].check_rate();
            limiters[idx].record_request();
            (wait,)
        };
        if let Some(delay) = wait {
            tokio::time::sleep(delay).await;
        }

        let url = self.url_for(url_type, endpoint);
        let http_method = method.unwrap_or_else(|| {
            match body {
                Some(_) => reqwest::Method::POST,
                None => reqwest::Method::GET,
            }
        });
        let mut req = self.client.request(http_method, &url);

        if url_type == UrlType::Local {
            let password = self.local_password.lock().unwrap().clone();
            let auth_header = format!(
                "Basic {}",
                base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    format!("riot:{}", password)
                )
            );
            req = req.header("Authorization", auth_header);
        } else {
            for (key, value) in headers {
                req = req.header(key.as_str(), value.as_str());
            }
        }

        if let Some(b) = body {
            req = req.json(&b);
        }

        let response = req.send().await.map_err(ApiError::Http)?;

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
            let (wait,) = {
                let mut limiters = self.rate_limiters.lock().unwrap();
                let idx = Self::limiter_index(UrlType::Custom);
                let wait = limiters[idx].check_rate();
                limiters[idx].record_request();
                (wait,)
            };
            if let Some(delay) = wait {
                tokio::time::sleep(delay).await;
            }
        }

        let url = format!("https://valorant-api.com/v1/{}", endpoint);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "VRY/1.0")
            .send()
            .await
            .map_err(ApiError::Http)?;
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
