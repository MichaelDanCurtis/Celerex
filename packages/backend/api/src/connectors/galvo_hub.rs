//! Galvo Hub mTLS A2A federation client.
//!
//! Celerex participates in Galvo's agent-to-agent federation as a first-class
//! org/agent: it registers with the Hub (CSR → issued client cert), invokes
//! remote agents through encrypted, Hub-relayed envelopes, and runs a `/push`
//! receiver so remote agents can call Celerex agents back.
//!
//! Crypto, envelope sealing, and the outbound relay flow are reused verbatim
//! from Galvo's own `galvo-federation` crate (X25519 ECDH + HKDF-SHA256 +
//! ChaCha20-Poly1305, Ed25519 sender sigs) — there is zero reimplemented
//! crypto here, so the wire format can't drift from the Hub. The one thing we
//! hand-roll is the `/push` WebSocket *challenge-response* handshake, because
//! the current Hub requires a signed-nonce hello that the crate's older
//! `WebSocketTransport` doesn't send.
//!
//! Wire contracts (verified against galvo-hub source):
//!   - `POST {hub_url}/rpc` JSON-RPC 2.0: hub.register / hub.set_org_tier /
//!     hub.lookup_agent / hub.request_cap_token / hub.relay
//!   - Registration is the bootstrap: it runs over the Hub's TLS listener,
//!     which `allow_unauthenticated()`s, so a CA-trusting client with no client
//!     cert can call it. The response carries the issued client cert.
//!   - Trust gating: new orgs are `Mint`; a target's default policy requires
//!     `Verified`, so a caller org must be elevated via hub.set_org_tier
//!     (bootstrap-gated) before request_cap_token succeeds.
//!   - `/push` (ws): Hub→`{"kind":"challenge","nonce_b64":..}`;
//!     client→`{"kind":"hello","agent_handle":..,"challenge_sig_b64":ed25519(nonce)}`;
//!     Hub→`{"kind":"inbound_relay","envelope_json":"<sealed envelope JSON>"}`.

use base64::{engine::general_purpose::STANDARD_NO_PAD as B64, Engine};
use futures_util::{SinkExt, StreamExt};
use galvo_federation::client::SendClient;
use galvo_federation::envelope::{verify_and_open, EnvelopeHeader, OpenParams, SealedEnvelope};
use galvo_federation::keys::{AgentKeys, Ed25519Keypair, X25519Keypair};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("hub rpc error: {0}")]
    Hub(String),
    #[error("malformed hub response: {0}")]
    Malformed(String),
    #[error("tls/cert: {0}")]
    Cert(String),
    #[error("csr: {0}")]
    Csr(String),
    #[error("key: {0}")]
    Key(String),
    #[error("send: {0}")]
    Send(String),
    #[error("envelope: {0}")]
    Envelope(String),
    #[error("websocket: {0}")]
    Ws(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Persisted federation identity for one Celerex agent. Written once on
/// registration and reloaded on subsequent boots so a Celerex instance keeps a
/// stable handle + keys across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubKeystore {
    pub handle: String, // "org/agent"
    pub org_id: String,
    pub hub_url: String, // https://host:port  (JSON-RPC)
    pub ed25519_secret_b64: String,
    pub ed25519_public_b64: String,
    pub x25519_secret_b64: String,
    pub x25519_public_b64: String,
    pub issued_cert_pem: String,
    pub client_key_pem: String,
    pub ca_cert_pem: String,
    pub hub_issuer_pub_b64: String,
}

impl HubKeystore {
    pub fn load(path: &std::path::Path) -> Result<Self, HubError> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    pub fn save(&self, path: &std::path::Path) -> Result<(), HubError> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
    fn ed25519(&self) -> Result<Ed25519Keypair, HubError> {
        Ed25519Keypair::from_secret_b64(&self.ed25519_secret_b64).map_err(|e| HubError::Key(e.to_string()))
    }
    fn x25519(&self) -> Result<X25519Keypair, HubError> {
        X25519Keypair::from_secret_b64(&self.x25519_secret_b64).map_err(|e| HubError::Key(e.to_string()))
    }
}

/// Generate a fresh CSR + its private key (PEM). Mirrors the CSR shape the Hub
/// CA expects (`CertificateSigningRequestParams::from_pem`).
fn generate_csr(cn: &str) -> Result<(String, String), HubError> {
    let kp = rcgen::KeyPair::generate().map_err(|e| HubError::Csr(e.to_string()))?;
    let mut params = rcgen::CertificateParams::default();
    let mut dn = rcgen::DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, cn);
    params.distinguished_name = dn;
    let csr = params.serialize_request(&kp).map_err(|e| HubError::Csr(e.to_string()))?;
    let csr_pem = csr.pem().map_err(|e| HubError::Csr(e.to_string()))?;
    Ok((csr_pem, kp.serialize_pem()))
}

/// A CA-trusting reqwest client with NO client identity — used for the
/// pre-issuance bootstrap (`hub.register`) and admin/lookup RPCs.
fn ca_trusting_client(ca_pem: &str) -> Result<reqwest::Client, HubError> {
    let root = reqwest::Certificate::from_pem(ca_pem.as_bytes())?;
    reqwest::Client::builder()
        .use_rustls_tls()
        .add_root_certificate(root)
        .build()
        .map_err(HubError::Http)
}

/// POST a JSON-RPC call and unwrap `result`, surfacing `error` as `HubError::Hub`.
async fn rpc(client: &reqwest::Client, hub_url: &str, method: &str, params: Value) -> Result<Value, HubError> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let resp: Value = client
        .post(format!("{hub_url}/rpc"))
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    if let Some(err) = resp.get("error").filter(|v| !v.is_null()) {
        return Err(HubError::Hub(err.to_string()));
    }
    resp.get("result")
        .cloned()
        .ok_or_else(|| HubError::Malformed(format!("{method}: missing result")))
}

/// Where the federation keystore is persisted (env `GALVO_HUB_KEYSTORE`,
/// default `./galvo-hub-keys.json`). A single Celerex instance keeps one
/// federation identity here across restarts.
pub fn keystore_path() -> std::path::PathBuf {
    std::env::var("GALVO_HUB_KEYSTORE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("./galvo-hub-keys.json"))
}

pub struct RegisterRequest {
    pub hub_url: String,
    pub bootstrap_token: String,
    pub ca_cert_pem: String,
    pub org_id: String,
    pub org_name: String,
    pub agent_slug: String,
    pub agent_description: String,
    pub capabilities: Vec<String>,
}

/// Register Celerex as an org/agent with the Hub. Generates fresh A2A keys + a
/// CSR, exchanges them for an issued client cert, and returns a persistable
/// keystore.
pub async fn register(req: RegisterRequest) -> Result<HubKeystore, HubError> {
    let keys = AgentKeys::generate();
    let cn = format!("{}.{}.celerex.local", req.agent_slug, req.org_id);
    let (csr_pem, client_key_pem) = generate_csr(&cn)?;

    let client = ca_trusting_client(&req.ca_cert_pem)?;
    let result = rpc(
        &client,
        &req.hub_url,
        "hub.register",
        json!({
            "bootstrap_token": req.bootstrap_token,
            "org_id": req.org_id,
            "org_name": req.org_name,
            "agent_slug": req.agent_slug,
            "agent_description": req.agent_description,
            "agent_capabilities": req.capabilities,
            "csr_pem": csr_pem,
            "ed25519_pub_b64": keys.ed25519_public_b64,
            "x25519_pub_b64": keys.x25519_public_b64,
        }),
    )
    .await?;

    let issued_cert_pem = result["issued_cert_pem"]
        .as_str()
        .ok_or_else(|| HubError::Malformed("register: no issued_cert_pem".into()))?
        .to_string();
    let ca_cert_pem = result["ca_cert_pem"]
        .as_str()
        .map(str::to_string)
        .unwrap_or(req.ca_cert_pem);
    let hub_issuer_pub_b64 = result["hub_issuer_pub_b64"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(HubKeystore {
        handle: format!("{}/{}", req.org_id, req.agent_slug),
        org_id: req.org_id,
        hub_url: req.hub_url,
        ed25519_secret_b64: keys.ed25519_secret_b64,
        ed25519_public_b64: keys.ed25519_public_b64,
        x25519_secret_b64: keys.x25519_secret_b64,
        x25519_public_b64: keys.x25519_public_b64,
        issued_cert_pem,
        client_key_pem,
        ca_cert_pem,
        hub_issuer_pub_b64,
    })
}

/// Elevate an org's trust tier (bootstrap-gated). Needed so a Mint-tier caller
/// can clear a target's default `Verified` policy.
pub async fn set_org_tier(
    hub_url: &str,
    ca_cert_pem: &str,
    bootstrap_token: &str,
    org_id: &str,
    tier: &str,
) -> Result<(), HubError> {
    let client = ca_trusting_client(ca_cert_pem)?;
    rpc(
        &client,
        hub_url,
        "hub.set_org_tier",
        json!({ "bootstrap_token": bootstrap_token, "org_id": org_id, "tier": tier }),
    )
    .await?;
    Ok(())
}

/// A registered Celerex federation agent: can invoke remote agents and run a
/// `/push` receiver.
pub struct GalvoHubClient {
    ks: HubKeystore,
}

impl GalvoHubClient {
    pub fn new(ks: HubKeystore) -> Self {
        Self { ks }
    }

    pub fn handle(&self) -> &str {
        &self.ks.handle
    }

    fn send_client(&self) -> Result<SendClient, HubError> {
        SendClient::new(
            self.ks.hub_url.clone(),
            self.ks.ca_cert_pem.as_bytes(),
            self.ks.issued_cert_pem.as_bytes(),
            self.ks.client_key_pem.as_bytes(),
            self.ks.handle.clone(),
            self.ks.ed25519()?,
        )
        .map_err(|e| HubError::Send(e.to_string()))
    }

    /// Invoke a remote agent: lookup → cap token → seal → relay. Returns the
    /// sealed envelope that was relayed (for audit/correlation).
    pub async fn invoke(&self, to_handle: &str, payload: &[u8]) -> Result<SealedEnvelope, HubError> {
        let send = self.send_client()?;
        let task_id = format!("task-{}", uuid::Uuid::new_v4().simple());
        let msg_id = format!("msg-{}", uuid::Uuid::new_v4().simple()); // 36 chars ≥ 12
        send.send(to_handle, &task_id, &msg_id, payload)
            .await
            .map_err(|e| HubError::Send(e.to_string()))
    }

    /// Look up a peer's Ed25519 public key (to verify inbound sender sigs).
    async fn lookup_ed25519(&self, handle: &str) -> Result<String, HubError> {
        let client = ca_trusting_client(&self.ks.ca_cert_pem)?;
        let result = rpc(&client, &self.ks.hub_url, "hub.lookup_agent", json!({ "handle": handle })).await?;
        result["keys"]["ed25519_pub"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| HubError::Malformed("lookup_agent: no keys.ed25519_pub".into()))
    }

    /// Connect to the Hub's `/push` channel, complete the challenge-response
    /// handshake, then decrypt each relayed envelope and hand the plaintext +
    /// header to `on_message`. Runs until the socket closes or errors.
    pub async fn run_receiver<F>(&self, ws_url: &str, mut on_message: F) -> Result<(), HubError>
    where
        F: FnMut(EnvelopeHeader, Vec<u8>) + Send,
    {
        let (ws, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| HubError::Ws(e.to_string()))?;
        let (mut write, mut read) = ws.split();
        let recipient_x = self.ks.x25519()?;
        let ed = self.ks.ed25519()?;

        while let Some(frame) = read.next().await {
            let msg = frame.map_err(|e| HubError::Ws(e.to_string()))?;
            match msg {
                Message::Text(text) => {
                    let v: Value = serde_json::from_str(&text)?;
                    match v["kind"].as_str() {
                        Some("challenge") => {
                            let nonce_b64 = v["nonce_b64"]
                                .as_str()
                                .ok_or_else(|| HubError::Ws("challenge missing nonce_b64".into()))?;
                            let nonce = B64
                                .decode(nonce_b64)
                                .map_err(|e| HubError::Ws(format!("bad nonce b64: {e}")))?;
                            let sig_b64 = ed.sign_b64(&nonce);
                            let hello = json!({
                                "kind": "hello",
                                "agent_handle": self.ks.handle,
                                "challenge_sig_b64": sig_b64,
                            });
                            write
                                .send(Message::Text(hello.to_string().into()))
                                .await
                                .map_err(|e| HubError::Ws(e.to_string()))?;
                        }
                        Some("inbound_relay") => {
                            let env_json = v["envelope_json"]
                                .as_str()
                                .ok_or_else(|| HubError::Ws("inbound_relay missing envelope_json".into()))?;
                            let env: SealedEnvelope = serde_json::from_str(env_json)?;
                            let header = env.header.clone();
                            let sender_pub = self.lookup_ed25519(&header.from).await?;
                            let pt = verify_and_open(OpenParams {
                                envelope: &env,
                                recipient_x25519: &recipient_x,
                                sender_ed25519_pub_b64: &sender_pub,
                            })
                            .map_err(|e| HubError::Envelope(e.to_string()))?;
                            on_message(header, pt);
                        }
                        Some("error") => {
                            return Err(HubError::Ws(format!(
                                "hub closed push: {}",
                                v["reason"].as_str().unwrap_or("unknown")
                            )));
                        }
                        _ => {}
                    }
                }
                Message::Ping(p) => {
                    write
                        .send(Message::Pong(p))
                        .await
                        .map_err(|e| HubError::Ws(e.to_string()))?;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_csr_produces_pem_and_key() {
        let (csr, key) = generate_csr("celerex.flow.local").unwrap();
        assert!(csr.contains("BEGIN CERTIFICATE REQUEST"));
        assert!(key.contains("BEGIN PRIVATE KEY"));
        // Parseability by the Hub CA is proven for real by the in-process
        // federation integration test (tests/galvo_hub_federation.rs).
    }

    #[test]
    fn keystore_round_trips_keys() {
        let keys = AgentKeys::generate();
        let ks = HubKeystore {
            handle: "celerex/flow".into(),
            org_id: "celerex".into(),
            hub_url: "https://127.0.0.1:8443".into(),
            ed25519_secret_b64: keys.ed25519_secret_b64.clone(),
            ed25519_public_b64: keys.ed25519_public_b64.clone(),
            x25519_secret_b64: keys.x25519_secret_b64.clone(),
            x25519_public_b64: keys.x25519_public_b64.clone(),
            issued_cert_pem: "x".into(),
            client_key_pem: "y".into(),
            ca_cert_pem: "z".into(),
            hub_issuer_pub_b64: "w".into(),
        };
        // Secrets reconstruct to the same public keys.
        assert_eq!(ks.ed25519().unwrap().public_b64(), keys.ed25519_public_b64);
        assert_eq!(ks.x25519().unwrap().public_b64(), keys.x25519_public_b64);
    }
}
