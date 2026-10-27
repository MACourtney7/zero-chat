//! HTTP/WebSocket route handlers, HMAC-SHA256 verification, and single-use invite burning
//! for the stateless Cloudflare Worker & Hibernating Durable Object signaling bootstrap.

use std::cell::RefCell;
use std::fmt;

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use worker::{Headers, Request, RequestInit, Response, Result, State, WebSocketPair};
use zero_protocol::crypto::verify_invite_token;
use zero_protocol::{from_json_bytes, to_json_string, DropPayload};

use crate::durable_object::{
    EphemeralRoomState, HibernatingSocket, HostAttachment, HOST_WEBSOCKET_TAG,
};

/// Maximum permitted HTTP body size (64 KiB) for `POST /drop/:do_id/:invite_uuid`.
pub const MAX_DROP_BODY_BYTES: usize = 65_536;

/// Target-independent HTTP response representation returned by protocol handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Signaling protocol errors mapped deterministically to HTTP status codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    MissingUpgradeHeader,
    InvalidHostSecret,
    HostSocketUnavailable,
    InvalidDropPayload(String),
    InviteUuidMismatch,
    ForbiddenHmac,
    InviteAlreadyBurned,
    BurnCapacityExceeded,
    PayloadTooLarge,
    SocketError(String),
}

impl NetworkError {
    /// Returns the canonical HTTP status code for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::MissingUpgradeHeader => 426,
            Self::InvalidHostSecret => 400,
            Self::HostSocketUnavailable => 404,
            Self::InvalidDropPayload(_) => 400,
            Self::InviteUuidMismatch => 400,
            Self::ForbiddenHmac => 403,
            Self::InviteAlreadyBurned => 410,
            Self::BurnCapacityExceeded => 429,
            Self::PayloadTooLarge => 413,
            Self::SocketError(_) => 500,
        }
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUpgradeHeader => write!(f, "Expected Upgrade: websocket header"),
            Self::InvalidHostSecret => {
                write!(
                    f,
                    "Invalid or missing 32-byte host_secret (expected hex or base64)"
                )
            }
            Self::HostSocketUnavailable => {
                write!(f, "No active hibernating host WebSocket for room")
            }
            Self::InvalidDropPayload(msg) => write!(f, "Invalid DropPayload: {msg}"),
            Self::InviteUuidMismatch => {
                write!(f, "Path invite_uuid does not match DropPayload.invite_uuid")
            }
            Self::ForbiddenHmac => write!(f, "403 Forbidden: Invalid HMAC invite token"),
            Self::InviteAlreadyBurned => {
                write!(f, "410 Gone: Invite UUID has already been burned")
            }
            Self::BurnCapacityExceeded => {
                write!(
                    f,
                    "429 Too Many Requests: Room invite burn capacity exceeded"
                )
            }
            Self::PayloadTooLarge => write!(f, "413 Payload Too Large"),
            Self::SocketError(msg) => write!(f, "WebSocket error: {msg}"),
        }
    }
}

impl std::error::Error for NetworkError {}

/// Extracts a bearer token (or raw token) from an optional `Authorization` header value.
pub fn extract_bearer_token(auth_header: Option<&str>) -> Option<&str> {
    let trimmed = auth_header?.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("bearer") {
        return None;
    }

    if let Some(prefix) = trimmed.get(..7) {
        if prefix.eq_ignore_ascii_case("bearer ") {
            let remainder = trimmed[7..].trim();
            return if remainder.is_empty() {
                None
            } else {
                Some(remainder)
            };
        }
    }

    Some(trimmed)
}

/// Parses a 32-byte `host_secret` from either a 64-character hex string or a Base64/Base64URL string.
/// Strips an optional `"Bearer "` prefix if present.
pub fn parse_host_secret(raw: &str) -> std::result::Result<[u8; 32], NetworkError> {
    let cleaned = extract_bearer_token(Some(raw)).ok_or(NetworkError::InvalidHostSecret)?;

    if cleaned.len() == 64 {
        if let Ok(bytes) = hex::decode(cleaned) {
            if bytes.len() == 32 {
                let mut secret = [0u8; 32];
                secret.copy_from_slice(&bytes);
                return Ok(secret);
            }
        }
    }

    let engines = [URL_SAFE_NO_PAD, URL_SAFE, STANDARD_NO_PAD, STANDARD];
    for engine in engines {
        if let Ok(bytes) = engine.decode(cleaned) {
            if bytes.len() == 32 {
                let mut secret = [0u8; 32];
                secret.copy_from_slice(&bytes);
                return Ok(secret);
            }
        }
    }

    Err(NetworkError::InvalidHostSecret)
}

/// Validates a host WebSocket upgrade request (`/host/:do_id`), parses the 32-byte `host_secret`,
/// and attaches `HostAttachment { host_secret }` to the hibernating socket.
pub fn process_host_upgrade<S: HibernatingSocket>(
    upgrade_header: Option<&str>,
    secret_str: Option<&str>,
    socket: &mut S,
) -> std::result::Result<HttpResponse, NetworkError> {
    match upgrade_header {
        Some(val) if val.trim().eq_ignore_ascii_case("websocket") => {}
        _ => return Err(NetworkError::MissingUpgradeHeader),
    }

    let raw_secret = secret_str.ok_or(NetworkError::InvalidHostSecret)?;
    let host_secret = parse_host_secret(raw_secret)?;

    socket
        .attach_host_secret(&HostAttachment { host_secret })
        .map_err(NetworkError::SocketError)?;

    Ok(HttpResponse {
        status: 101,
        body: String::new(),
    })
}

/// Processes a joiner's dead-drop `POST /drop/:do_id/:invite_uuid`:
/// 1. Validates payload size, JSON structure, and `invite_uuid` consistency.
/// 2. Recovers `HostAttachment` from the hibernating host WebSocket.
/// 3. Verifies the HMAC-SHA256 invite token in constant time (returning `403 Forbidden` on failure).
/// 4. Checks and burns `invite_uuid` in `EphemeralRoomState` (returning `410 Gone` on replay).
/// 5. Pipes the serialized `DropPayload` directly down the host's hibernating WebSocket.
pub fn process_dead_drop<S: HibernatingSocket>(
    path_invite_uuid: &str,
    auth_header: Option<&str>,
    body_bytes: &[u8],
    room_state: &mut EphemeralRoomState,
    host_sockets: &mut [S],
) -> std::result::Result<HttpResponse, NetworkError> {
    if body_bytes.len() > MAX_DROP_BODY_BYTES {
        return Err(NetworkError::PayloadTooLarge);
    }

    if path_invite_uuid.trim().is_empty() {
        return Err(NetworkError::InvalidDropPayload(
            "Empty invite UUID".to_string(),
        ));
    }

    let payload: DropPayload =
        from_json_bytes(body_bytes).map_err(|e| NetworkError::InvalidDropPayload(e.to_string()))?;

    if payload.invite_uuid != path_invite_uuid {
        return Err(NetworkError::InviteUuidMismatch);
    }

    let token = match extract_bearer_token(auth_header) {
        Some(header_token) => {
            if !payload.token.is_empty() && payload.token != header_token {
                return Err(NetworkError::ForbiddenHmac);
            }
            header_token
        }
        None => {
            let trimmed = payload.token.trim();
            if trimmed.is_empty() {
                return Err(NetworkError::ForbiddenHmac);
            }
            trimmed
        }
    };

    let mut recovered_attachment: Option<HostAttachment> = None;
    for sock in host_sockets.iter() {
        if let Some(att) = sock
            .recover_host_secret()
            .map_err(NetworkError::SocketError)?
        {
            recovered_attachment = Some(att);
            break;
        }
    }

    let attachment = recovered_attachment.ok_or(NetworkError::HostSocketUnavailable)?;

    // REQ-003.3: Constant-time HMAC verification BEFORE checking or mutating burn state.
    verify_invite_token(&attachment.host_secret, path_invite_uuid, token)
        .map_err(|_| NetworkError::ForbiddenHmac)?;

    // REQ-003.4: Replay detection & single-use invite burning in volatile memory.
    if room_state.is_burned(path_invite_uuid) {
        return Err(NetworkError::InviteAlreadyBurned);
    }
    if !room_state.burn_uuid(path_invite_uuid.to_string()) {
        return Err(NetworkError::BurnCapacityExceeded);
    }

    // REQ-003.5: Pipe serialized DropPayload down the hibernating host WebSocket(s).
    let serialized =
        to_json_string(&payload).map_err(|e| NetworkError::InvalidDropPayload(e.to_string()))?;

    for sock in host_sockets.iter_mut() {
        if sock
            .recover_host_secret()
            .map_err(NetworkError::SocketError)?
            .is_some()
        {
            sock.send_text(&serialized)
                .map_err(NetworkError::SocketError)?;
        }
    }

    Ok(HttpResponse {
        status: 200,
        body: r#"{"status":"ok"}"#.to_string(),
    })
}

/// Attaches permissive CORS headers to a Cloudflare Worker `Response`.
pub fn with_cors(mut resp: Response) -> Result<Response> {
    resp.headers_mut().set("Access-Control-Allow-Origin", "*")?;
    resp.headers_mut()
        .set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")?;
    resp.headers_mut().set(
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization, X-Host-Secret",
    )?;
    Ok(resp)
}

/// Constructs an HTTP 204 No Content response for CORS preflight `OPTIONS` requests.
pub fn cors_preflight_response() -> Result<Response> {
    let resp = Response::empty()?.with_status(204);
    with_cors(resp)
}

/// Retrieves a binding string from `worker::Env`, checking secrets first, then environment variables.
pub fn get_env_var(env: &worker::Env, key: &str) -> Option<String> {
    if let Ok(sec) = env.secret(key) {
        let val = sec.to_string();
        if !val.trim().is_empty() {
            return Some(val);
        }
    }
    if let Ok(var) = env.var(key) {
        let val = var.to_string();
        if !val.trim().is_empty() {
            return Some(val);
        }
    }
    None
}

/// Parses custom TURN/ICE servers JSON string (array or object with `iceServers`) into `{ "iceServers": ... }`.
pub fn parse_custom_ice_servers(raw_json: &str) -> Option<serde_json::Value> {
    let parsed: serde_json::Value = serde_json::from_str(raw_json).ok()?;
    match parsed {
        serde_json::Value::Array(arr) => Some(serde_json::json!({
            "iceServers": arr
        })),
        serde_json::Value::Object(map) => map
            .get("iceServers")
            .map(|servers| serde_json::json!({ "iceServers": servers })),
        _ => None,
    }
}

/// Returns default STUN configuration structure for WebRTC NAT traversal.
pub fn default_ice_servers_json() -> serde_json::Value {
    serde_json::json!({
        "iceServers": [
            { "urls": "stun:stun.cloudflare.com:3478" },
            { "urls": "stun:stun.l.google.com:19302" }
        ]
    })
}

/// Target-independent handler resolving custom JSON or falling back to default STUN configuration.
pub fn process_ice_servers(custom_json: Option<&str>) -> HttpResponse {
    if let Some(json_str) = custom_json {
        if let Some(val) = parse_custom_ice_servers(json_str) {
            return HttpResponse {
                status: 200,
                body: serde_json::to_string(&val).unwrap_or_default(),
            };
        }
    }
    HttpResponse {
        status: 200,
        body: serde_json::to_string(&default_ice_servers_json()).unwrap_or_default(),
    }
}

/// Constructs a 200 OK Response with default STUN configuration and CORS headers.
pub fn build_default_stun_response() -> Result<Response> {
    with_cors(Response::from_json(&default_ice_servers_json())?)
}

/// Provisions short-lived ephemeral Cloudflare TURN credentials or returns a reliable default STUN configuration.
pub async fn handle_ice_config(env: &worker::Env) -> Result<Response> {
    // Mode 1: Cloudflare Realtime TURN API
    let key_id = get_env_var(env, "CF_TURN_KEY_ID");
    let api_token = get_env_var(env, "CF_TURN_API_TOKEN");

    if let (Some(key_id), Some(api_token)) = (key_id, api_token) {
        let url =
            format!("https://rtc.live.cloudflare.com/v1/turn/keys/{key_id}/credentials/generate");
        let headers = Headers::new();
        let _ = headers.set("Authorization", &format!("Bearer {api_token}"));
        let _ = headers.set("Content-Type", "application/json");

        let mut init = RequestInit::new();
        init.with_method(worker::Method::Post);
        init.with_headers(headers);
        init.with_body(Some(worker::wasm_bindgen::JsValue::from_str(
            "{\"ttl\": 3600}",
        )));

        match Request::new_with_init(&url, &init) {
            Ok(req) => match worker::Fetch::Request(req).send().await {
                Ok(mut cf_res) => {
                    if cf_res.status_code() == 200 {
                        if let Ok(body_str) = cf_res.text().await {
                            if let Ok(json_val) =
                                serde_json::from_str::<serde_json::Value>(&body_str)
                            {
                                return with_cors(Response::from_json(&json_val)?);
                            }
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    worker::console_warn!(
                        "Cloudflare TURN API returned status {} or invalid body",
                        cf_res.status_code()
                    );
                    #[cfg(not(target_arch = "wasm32"))]
                    eprintln!(
                        "Cloudflare TURN API returned status {} or invalid body",
                        cf_res.status_code()
                    );
                }
                Err(e) => {
                    #[cfg(target_arch = "wasm32")]
                    worker::console_warn!("Failed to fetch Cloudflare TURN credentials: {:?}", e);
                    #[cfg(not(target_arch = "wasm32"))]
                    eprintln!("Failed to fetch Cloudflare TURN credentials: {:?}", e);
                }
            },
            Err(e) => {
                #[cfg(target_arch = "wasm32")]
                worker::console_warn!("Failed to build Cloudflare TURN request: {:?}", e);
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("Failed to build Cloudflare TURN request: {:?}", e);
            }
        }
    }

    // Mode 2 & Mode 3: Custom JSON Override or Default STUN Fallback
    let custom_json = get_env_var(env, "TURN_SERVERS_JSON");
    let resolved = process_ice_servers(custom_json.as_deref());
    let val: serde_json::Value =
        serde_json::from_str(&resolved.body).unwrap_or_else(|_| default_ice_servers_json());
    with_cors(Response::from_json(&val)?)
}

/// Cloudflare Durable Object adapter for `GET /host/:do_id` WebSocket upgrade and hibernation registration.
pub fn handle_do_host_upgrade(req: &Request, state: &State) -> Result<Response> {
    let upgrade = req.headers().get("Upgrade")?;
    let mut secret = req.headers().get("X-Host-Secret")?;
    if secret.is_none() {
        secret = req.headers().get("Authorization")?;
    }
    if secret.is_none() {
        if let Ok(url) = req.url() {
            for (k, v) in url.query_pairs() {
                if k == "secret" || k == "host_secret" {
                    secret = Some(v.into_owned());
                    break;
                }
            }
        }
    }

    match upgrade.as_deref() {
        Some(val) if val.trim().eq_ignore_ascii_case("websocket") => {}
        _ => {
            let err = NetworkError::MissingUpgradeHeader;
            return Response::error(err.to_string(), err.status_code());
        }
    }

    let raw_secret = match secret.as_deref() {
        Some(s) => s,
        None => {
            let err = NetworkError::InvalidHostSecret;
            return Response::error(err.to_string(), err.status_code());
        }
    };

    if let Err(err) = parse_host_secret(raw_secret) {
        return Response::error(err.to_string(), err.status_code());
    }

    let pair = WebSocketPair::new()?;
    let mut server = pair.server;
    let client = pair.client;

    if let Err(err) = process_host_upgrade(upgrade.as_deref(), Some(raw_secret), &mut server) {
        return Response::error(err.to_string(), err.status_code());
    }

    state.accept_websocket_with_tags(&server, &[HOST_WEBSOCKET_TAG]);
    Response::from_websocket(client)
}

/// Cloudflare Durable Object adapter for `POST /drop/:do_id/:invite_uuid`.
pub async fn handle_do_dead_drop(
    req: &mut Request,
    state: &State,
    ephemeral: &RefCell<EphemeralRoomState>,
    invite_uuid: &str,
) -> Result<Response> {
    let auth_header = req.headers().get("Authorization")?;
    let body_bytes = req.bytes().await?;

    let mut sockets = state.get_websockets_with_tag(HOST_WEBSOCKET_TAG);
    if sockets.is_empty() {
        sockets = state.get_websockets();
    }

    let result = {
        let mut room_state = ephemeral.borrow_mut();
        process_dead_drop(
            invite_uuid,
            auth_header.as_deref(),
            &body_bytes,
            &mut room_state,
            &mut sockets,
        )
    };

    match result {
        Ok(_) => Response::from_json(&serde_json::json!({ "status": "ok" })),
        Err(err) => Response::error(err.to_string(), err.status_code()),
    }
}
