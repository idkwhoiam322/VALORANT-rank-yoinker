use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{connect_async_tls_with_config, Connector, tungstenite::Message};

use crate::models::presences::GameState;
use crate::services::logging::Logger;

/// Receives game-state changes via the Riot local WebSocket.
///
/// Spawns a background task that subscribes to
/// `OnJsonApiEvent_chat_v4_presences` and pushes `GameState` transitions
/// into a channel.  The main loop polls this channel instead of making
/// repeated HTTP calls to `/chat/v4/presences`.
pub struct ValorantWs {
    state_rx: mpsc::Receiver<GameState>,
}

impl ValorantWs {
    /// Try to connect to the Riot local WebSocket and subscribe to presence
    /// events.  Returns `None` when the connection cannot be established
    /// (the caller falls back to polling).
    pub async fn connect(
        port: u16,
        password: &str,
        puuid: &str,
        logger: Arc<Logger>,
    ) -> Option<Self> {
        let (tx, rx) = mpsc::channel(32);
        let puuid = puuid.to_string();
        let url = format!("wss://127.0.0.1:{port}");
        let password = password.to_string();

        // Build a TLS connector that accepts the self-signed Riot certificate.
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .ok()?;
        let connector = Connector::NativeTls(tls);

        // Spawn the WS background task.
        tokio::spawn(async move {
            ws_task(&url, connector, &password, &puuid, tx, logger).await;
        });

        Some(Self { state_rx: rx })
    }

    /// Receive the next game-state change (non-blocking callers should
    /// use `tokio::select!` with a timeout as fallback).
    pub async fn recv(&mut self) -> Option<GameState> {
        self.state_rx.recv().await
    }
}

// ---------------------------------------------------------------------------
// Background task — connect → subscribe → read → reconnect on failure
// ---------------------------------------------------------------------------

async fn ws_task(
    url: &str,
    connector: Connector,
    password: &str,
    puuid: &str,
    tx: mpsc::Sender<GameState>,
    logger: Arc<Logger>,
) {
    let mut backoff = 1u64;
    let mut last_state = String::new();

    // Build the WebSocket upgrade request with Basic auth.
    let auth = format!(
        "Basic {}",
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("riot:{password}")
        )
    );

    loop {
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert(http::header::AUTHORIZATION, auth.parse().unwrap());
        let mut stream = match connect_async_tls_with_config(request, None, false, Some(connector.clone())).await
        {
            Ok((ws, _)) => {
                let was_reconnect = backoff > 1;
                backoff = 1;
                logger.log(&format!("WS presences connected{}", if was_reconnect { " (reconnected)" } else { "" }));
                ws
            }
            Err(e) => {
                logger.log(&format!("WS connect failed: {e} — retry in {backoff}s"));
                if tx.is_closed() {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(32);
                continue;
            }
        };

        // Subscribe to presence events (Riot local WebSocket protocol).
        // The integer prefix `5` is required by the Riot WS protocol.
        if let Err(e) = stream
            .send(Message::Text(r#"[5, "OnJsonApiEvent_chat_v4_presences"]"#.into()))
            .await
        {
            logger.log(&format!("WS subscribe failed: {e} — reconnecting"));
            continue;
        }

        // Read loop.
        loop {
            let msg = tokio::select! {
                msg = stream.next() => msg,
                _ = tokio::time::sleep(Duration::from_secs(60)) => {
                    logger.log("WS idle timeout — reconnecting");
                    break;
                }
            };

            match msg {
                Some(Ok(Message::Text(text))) => {
                    if let Some(new_state) =
                        parse_state_from_event(&text, puuid, &logger)
                    {
                        if new_state.as_str() != last_state {
                            last_state = new_state.as_str().to_string();
                            logger.log(&format!("WS state: {:?}", new_state));
                            if tx.send(new_state).await.is_err() {
                                return; // channel closed
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    logger.log("WS closed — reconnecting");
                    break;
                }
                Some(Err(e)) => {
                    logger.log(&format!("WS error: {e} — reconnecting"));
                    break;
                }
                _ => {}
            }
        }

        if tx.is_closed() {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Event parsing
// ---------------------------------------------------------------------------

fn parse_state_from_event(
    text: &str,
    puuid: &str,
    _logger: &Arc<Logger>,
) -> Option<GameState> {
    let event: serde_json::Value = serde_json::from_str(text).ok()?;

    // Try multiple envelope formats to reach the presence array.
    let presences = event
        // [seq, name, {"data": {"presences": [...]}}]
        .get(2)
        .and_then(|d| d.get("data"))
        .or_else(|| event.get("data"))
        .and_then(|d| d.as_object())
        .or_else(|| event.as_object())
        .and_then(|o| o.get("presences"))
        .and_then(|v| v.as_array())?;

    // Find own presence.
    let own = presences.iter().find(|p| {
        p.get("puuid").and_then(|v| v.as_str()) == Some(puuid)
            && p.get("product").and_then(|v| v.as_str()) == Some("valorant")
    })?;

    // Decode base64 private field.
    let private_b64 = own.get("private")?.as_str()?;
    let private_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        private_b64,
    )
    .ok()?;
    let private: serde_json::Value = serde_json::from_slice(&private_bytes).ok()?;

    // Extract sessionLoopState (nested or flat).
    let state_str = private
        .get("matchPresenceData")
        .and_then(|mpd| mpd.get("sessionLoopState"))
        .and_then(|v| v.as_str())
        .or_else(|| private.get("sessionLoopState").and_then(|v| v.as_str()))?;

    Some(GameState::from_str(state_str))
}
