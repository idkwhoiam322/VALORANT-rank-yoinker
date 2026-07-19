use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub name: String,
    pub pid: u32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

// Entitlements is held in memory only and turned into request headers via
// `build_headers`. It is intentionally NOT Serialize/Deserialize: the token and
// access_token are secrets and must never be (de)serialized to JSON/logs.
#[derive(Debug, Clone)]
pub struct Entitlements {
    pub access_token: SecretString,
    pub token: SecretString,
    pub subject: String,
}

impl Entitlements {
    pub fn build_headers(&self, client_version: &str) -> Vec<(String, String)> {
        vec![
            (
                "Authorization".into(),
                format!("Bearer {}", self.access_token.expose_secret()),
            ),
            (
                "X-Riot-Entitlements-JWT".into(),
                self.token.expose_secret().to_string(),
            ),
            ("X-Riot-ClientPlatform".into(), CLIENT_PLATFORM.into()),
            ("X-Riot-ClientVersion".into(), client_version.into()),
            ("User-Agent".into(), USER_AGENT.into()),
        ]
    }
}

const CLIENT_PLATFORM: &str = "ew0KCSJwbGF0Zm9ybVR5cGUiOiAiUEMiLA0KCSJwbGF0Zm9ybU9TIjogIldpbmRvd3MiLA0KCSJwbGF0Zm9ybU9TVmVyc2lvbiI6ICIxMC4wLjE5MDQyLjEuMjU2LjY0Yml0IiwNCgkicGxhdGZvcm1DaGlwc2V0IjogIlVua25vd24iDQp9";

const USER_AGENT: &str = "ShooterGame/13 Windows/10.0.19043.1.256.64bit";

#[derive(Debug, Clone)]
pub struct Region {
    pub pd: String,
    pub glz: String,
    pub shard: String,
}

impl Region {
    pub fn from_logs(pd_region: &str, glz_host: &str, glz_shard: &str) -> Self {
        Self {
            pd: pd_region.to_string(),
            glz: format!("glz-{}.{}", glz_host, glz_shard),
            shard: glz_shard.to_string(),
        }
    }

    pub fn pd_url(&self) -> String {
        format!("https://pd.{}.a.pvp.net", self.pd)
    }

    pub fn glz_url(&self) -> String {
        format!("https://{}.a.pvp.net", self.glz)
    }
}
