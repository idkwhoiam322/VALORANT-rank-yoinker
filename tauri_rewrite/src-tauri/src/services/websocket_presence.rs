use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::version::{TLS12, TLS13};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{connect_async_tls_with_config, Connector, tungstenite::Message};

use crate::models::presences::{GameState, Presence};
use crate::services::logging::Logger;

/// Custom certificate verifier that accepts any server certificate.
///
/// Used only for the local Riot WebSocket on 127.0.0.1, where Riot presents a
/// self-signed certificate. There is no MITM risk on localhost, but switching to
/// rustls here shrinks the TLS attack surface (no OpenSSL/SChannel via
/// native-tls). We keep `danger_accept_invalid_certs` semantics intentionally.
#[derive(Debug)]
struct AcceptInvalidServerCert;

impl ServerCertVerifier for AcceptInvalidServerCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA256,
        ]
    }
}

/// Full presence snapshot pushed by the Riot local WebSocket.
/// Carries both the game state and the entire presences array so
/// the heartbeat builder can reuse the data for mode/queue resolution
/// instead of a separate HTTP call.
pub struct WsPresenceEvent {
    pub state: GameState,
    pub presences: Vec<Presence>,
}

/// Receives game-state and presence snapshots via the Riot local
/// WebSocket.
///
/// Spawns a background task that subscribes to
/// `OnJsonApiEvent_chat_v4_presences` and pushes `WsPresenceEvent`
/// into a channel.  The main loop uses these for fast state detection
/// and for mode/queue resolution without a separate HTTP call.
pub struct ValorantWs {
    state_rx: mpsc::Receiver<WsPresenceEvent>,
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

        // Build a rustls TLS connector that accepts Riot's self-signed local cert.
        // Use builder_with_provider so we don't depend on a process-wide default
        // CryptoProvider being installed (rustls 0.23 requires exactly one provider
        // feature; here we pin the `ring` provider explicitly).
        let config = match ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&TLS13, &TLS12])
        {
            Ok(builder) => builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AcceptInvalidServerCert))
                .with_no_client_auth(),
            Err(e) => {
                logger.log(&format!("WS TLS config error: {e}"));
                return None;
            }
        };
        let connector = Connector::Rustls(Arc::new(config));

        // Spawn the WS background task.
        tokio::spawn(async move {
            ws_task(&url, connector, &password, &puuid, tx, logger).await;
        });

        Some(Self { state_rx: rx })
    }

    /// Receive the next presence event (non-blocking callers should
    /// use `tokio::select!` with a timeout as fallback).
    pub async fn recv(&mut self) -> Option<WsPresenceEvent> {
        self.state_rx.recv().await
    }

    /// Attempt to receive the next presence event without blocking.
    /// Returns the event if one is immediately available in the buffer,
    /// or an error if the channel is empty or closed.
    pub fn try_recv(&mut self) -> Result<WsPresenceEvent, mpsc::error::TryRecvError> {
        self.state_rx.try_recv()
    }
}

// ---------------------------------------------------------------------------
// Background task - connect → subscribe → read → reconnect on failure
// ---------------------------------------------------------------------------

async fn ws_task(
    url: &str,
    connector: Connector,
    password: &str,
    puuid: &str,
    tx: mpsc::Sender<WsPresenceEvent>,
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
            .insert(reqwest::header::AUTHORIZATION, auth.parse().unwrap());
        let mut stream = match connect_async_tls_with_config(request, None, false, Some(connector.clone())).await
        {
            Ok((ws, _)) => {
                let was_reconnect = backoff > 1;
                backoff = 1;
                logger.log(&format!("WS presences connected{}", if was_reconnect { " (reconnected)" } else { "" }));
                ws
            }
            Err(e) => {
                logger.log(&format!("WS connect failed: {e} - retry in {backoff}s"));
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
            logger.log(&format!("WS subscribe failed: {e} - reconnecting"));
            continue;
        }

        // Read loop.
        loop {
            let msg = tokio::select! {
                msg = stream.next() => msg,
                _ = tokio::time::sleep(Duration::from_secs(300)) => {
                    logger.log("WS idle timeout - reconnecting");
                    break;
                }
            };

            match msg {
                Some(Ok(Message::Text(text))) => {
                    if let Some(event) =
                        parse_presence_event(&text, puuid)
                    {
                        if event.state.as_str() != last_state {
                            last_state = event.state.as_str().to_string();
                            logger.log(&format!("WS state: {:?}", event.state));
                        }
                        if tx.send(event).await.is_err() {
                            return; // channel closed
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    logger.log("WS closed - reconnecting");
                    break;
                }
                Some(Err(e)) => {
                    logger.log(&format!("WS error: {e} - reconnecting"));
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

fn parse_presence_event(
    text: &str,
    puuid: &str,
) -> Option<WsPresenceEvent> {
    let event: serde_json::Value = serde_json::from_str(text).ok()?;

    // Try multiple envelope formats to reach the presence array.
    let raw_presences = event
        // [seq, name, {"data": {"presences": [...]}}]
        .get(2)
        .and_then(|d| d.get("data"))
        .or_else(|| event.get("data"))
        .and_then(|d| d.as_object())
        .or_else(|| event.as_object())
        .and_then(|o| o.get("presences"))
        .and_then(|v| v.as_array())?;

    // Parse the raw JSON presences into Presence structs.
    let presences: Vec<Presence> = raw_presences
        .iter()
        .filter_map(|p| serde_json::from_value::<Presence>(p.clone()).ok())
        .collect();

    // Find own presence.
    let own = raw_presences.iter().find(|p| {
        p.get("puuid").and_then(|v| v.as_str()) == Some(puuid)
            && p.get("product").and_then(|v| v.as_str()) == Some("valorant")
    })?;

    // Decode base64 private field (shared helper so the decode behaviour
    // matches the REST presence path exactly).
    let private_b64 = own.get("private")?.as_str()?;
    let private: serde_json::Value =
        crate::services::presences::PresenceService::decode_private_presence_json(private_b64)?;

    // Extract sessionLoopState (nested or flat).
    let state_str = private
        .get("matchPresenceData")
        .and_then(|mpd| mpd.get("sessionLoopState"))
        .and_then(|v| v.as_str())
        .or_else(|| private.get("sessionLoopState").and_then(|v| v.as_str()))?;

    Some(WsPresenceEvent {
        state: GameState::from_str(state_str),
        presences,
    })
}
