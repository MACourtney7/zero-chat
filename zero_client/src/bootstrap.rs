//! Signaling bootstrap, URL fragment auto-drop, and host WebSocket connection manager.
//!
//! Provides:
//! - Joiner auto-detection of URL `#fragment` on page load (`REQ-004.2`).
//! - Automated SDP Answer generation and background HTTP `POST` dispatch to `/drop/{do_id}/{invite_uuid}` (`REQ-004.3`).
//! - Host WebSocket signaling connection manager (`wss://.../host/{do_id}`) to receive dropped answers (`REQ-004.4`).
//! - Ephemeral edge severance: immediately terminating and closing the host signaling connection upon WebRTC
//!   DataChannel transition to `Open` (`REQ-004.5`).

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use zero_protocol::{from_json_str, to_json_string, DropPayload, InvitePayload};

use crate::invite::{unpack_invite_fragment, InviteError};
use crate::webrtc::{PeerLifecycleState, WebRtcError, WebRtcPeer};

/// Errors encountered during signaling bootstrap, fragment detection, dead-drop HTTP dispatch,
/// and WebSocket host signaling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapError {
    /// Failure originated from the underlying WebRTC engine.
    WebRtc(WebRtcError),
    /// Failure occurred while unpacking or verifying the invite payload.
    Invite(InviteError),
    /// HTTP dead-drop endpoint returned a 4xx or 5xx status code.
    HttpError { status: u16, message: String },
    /// Network fetch failure or browser fetch exception.
    FetchError(String),
    /// Host signaling WebSocket connection error.
    WebSocketError(String),
    /// Window or location object is unavailable in current execution context.
    NoWindowOrLocation,
    /// URL fragment exists but contains no payload parameter.
    NoPayloadInHash,
    /// JSON serialization or deserialization failure.
    Serialization(String),
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WebRtc(e) => write!(f, "WebRTC error: {e}"),
            Self::Invite(e) => write!(f, "Invite error: {e}"),
            Self::HttpError { status, message } => {
                write!(f, "HTTP error {status}: {message}")
            }
            Self::FetchError(e) => write!(f, "Fetch error: {e}"),
            Self::WebSocketError(e) => write!(f, "WebSocket error: {e}"),
            Self::NoWindowOrLocation => write!(f, "Window or location object unavailable"),
            Self::NoPayloadInHash => write!(f, "No invite payload found in URL hash"),
            Self::Serialization(e) => write!(f, "Serialization error: {e}"),
        }
    }
}

impl std::error::Error for BootstrapError {}

impl From<WebRtcError> for BootstrapError {
    fn from(err: WebRtcError) -> Self {
        Self::WebRtc(err)
    }
}

impl From<InviteError> for BootstrapError {
    fn from(err: InviteError) -> Self {
        Self::Invite(err)
    }
}

impl From<zero_protocol::ProtocolError> for BootstrapError {
    fn from(err: zero_protocol::ProtocolError) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Extracts the raw hash string from the browser window's location object (`REQ-004.2`).
pub fn extract_window_hash() -> Result<String, BootstrapError> {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window().ok_or(BootstrapError::NoWindowOrLocation)?;
        let location = window.location();
        location
            .hash()
            .map_err(|e| BootstrapError::FetchError(format!("{e:?}")))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Err(BootstrapError::NoWindowOrLocation)
    }
}

/// Detects and extracts an [`InvitePayload`] from a URL `#fragment` or query-style string (`REQ-004.2`).
pub fn detect_invite_from_hash(hash: &str) -> Result<InvitePayload, BootstrapError> {
    let trimmed = hash.trim();
    if trimmed.is_empty() || trimmed == "#" {
        return Err(BootstrapError::NoPayloadInHash);
    }
    unpack_invite_fragment(trimmed).map_err(BootstrapError::from)
}

/// Detects and extracts an [`InvitePayload`] from the current browser window's URL `#fragment` (`REQ-004.2`).
pub fn detect_invite_from_window() -> Result<InvitePayload, BootstrapError> {
    let hash = extract_window_hash()?;
    detect_invite_from_hash(&hash)
}

/// Dispatches a serialized [`DropPayload`] via HTTP `POST` to `/drop/{do_id}/{invite_uuid}` (`REQ-004.3`).
pub async fn dispatch_drop_post(
    endpoint_url: &str,
    token: &str,
    payload: &DropPayload,
) -> Result<u16, BootstrapError> {
    if endpoint_url.trim().is_empty() {
        return Err(BootstrapError::FetchError(
            "Endpoint URL cannot be empty".to_string(),
        ));
    }

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen_futures::JsFuture;

        let window = web_sys::window().ok_or(BootstrapError::NoWindowOrLocation)?;
        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        let body_str = to_json_string(payload)?;
        opts.set_body(&JsValue::from_str(&body_str));
        opts.set_mode(web_sys::RequestMode::Cors);

        let request = web_sys::Request::new_with_str_and_init(endpoint_url, &opts)
            .map_err(|e| BootstrapError::FetchError(format!("{e:?}")))?;

        let headers = request.headers();
        headers
            .set("Content-Type", "application/json")
            .map_err(|e| BootstrapError::FetchError(format!("{e:?}")))?;
        headers
            .set("Authorization", &format!("Bearer {token}"))
            .map_err(|e| BootstrapError::FetchError(format!("{e:?}")))?;

        let resp_val = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| BootstrapError::FetchError(format!("{e:?}")))?;

        let resp: web_sys::Response = resp_val
            .dyn_into()
            .map_err(|e| BootstrapError::FetchError(format!("{e:?}")))?;

        let status = resp.status();
        if status >= 400 {
            return Err(BootstrapError::HttpError {
                status,
                message: resp.status_text(),
            });
        }
        Ok(status)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = to_json_string(payload)?;

        // Deterministic simulation hook for testing HTTP error statuses in native unit tests
        if let Some(pos) = endpoint_url.find("/simulate_error/") {
            let remainder = &endpoint_url[pos + "/simulate_error/".len()..];
            let code_str = remainder.split('/').next().unwrap_or("");
            let code = code_str.parse::<u16>().unwrap_or(500);
            return Err(BootstrapError::HttpError {
                status: code,
                message: format!("Simulated HTTP error response {code}"),
            });
        }

        // Validate token format isn't empty
        if token.trim().is_empty() {
            return Err(BootstrapError::HttpError {
                status: 401,
                message: "Missing or empty bearer token".to_string(),
            });
        }

        Ok(200)
    }
}

/// Complete joiner auto-drop procedure (`REQ-004.3`):
/// 1. Accepts the SDP offer from [`InvitePayload`] on the joiner's [`WebRtcPeer`].
/// 2. Generates the SDP answer.
/// 3. Builds the [`DropPayload`].
/// 4. Dispatches the HTTP POST to the dead-drop endpoint `{origin_base}/drop/{do_id}/{invite_uuid}`.
pub async fn execute_joiner_autodrop(
    origin_base: &str,
    invite: &InvitePayload,
    peer: &WebRtcPeer,
    joiner_public_key: [u8; 32],
) -> Result<DropPayload, BootstrapError> {
    let clean_base = origin_base.trim_end_matches('/');
    if clean_base.is_empty() {
        return Err(BootstrapError::FetchError(
            "Origin base URL cannot be empty".to_string(),
        ));
    }

    let sdp_answer = peer
        .accept_offer_and_create_answer(&invite.sdp_offer.sdp)
        .await
        .map_err(BootstrapError::from)?;

    let drop_payload = DropPayload {
        invite_uuid: invite.invite_uuid.clone(),
        token: invite.token.clone(),
        sdp_answer,
        joiner_public_key,
    };

    let endpoint_url = format!("{clean_base}/drop/{}/{}", invite.do_id, invite.invite_uuid);
    let status = dispatch_drop_post(&endpoint_url, &invite.token, &drop_payload).await?;

    if status >= 400 {
        return Err(BootstrapError::HttpError {
            status,
            message: format!("Dead-drop dispatch rejected with status {status}"),
        });
    }

    Ok(drop_payload)
}

/// Manages host signaling connection to `/host/{do_id}` and enforces edge severance (`REQ-004.4` & `REQ-004.5`).
pub struct HostSignalingSession {
    do_id: String,
    host_secret: [u8; 32],
    peer: WebRtcPeer,
    is_socket_closed: Rc<RefCell<bool>>,
    #[cfg(target_arch = "wasm32")]
    ws: Option<web_sys::WebSocket>,
}

impl fmt::Debug for HostSignalingSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostSignalingSession")
            .field("do_id", &self.do_id)
            .field("host_secret", &"[REDACTED]")
            .field("is_socket_closed", &self.is_edge_severed())
            .finish()
    }
}

impl HostSignalingSession {
    /// Creates a new `HostSignalingSession` and registers the edge severance listener on `peer`.
    pub fn new(do_id: impl Into<String>, host_secret: [u8; 32], peer: WebRtcPeer) -> Self {
        let is_socket_closed = Rc::new(RefCell::new(false));
        let is_closed_flag = is_socket_closed.clone();

        peer.on_state_change(move |state| {
            if state == PeerLifecycleState::Open {
                *is_closed_flag.borrow_mut() = true;
            }
        });

        Self {
            do_id: do_id.into(),
            host_secret,
            peer,
            is_socket_closed,
            #[cfg(target_arch = "wasm32")]
            ws: None,
        }
    }

    /// Returns the Durable Object identifier for this signaling session.
    pub fn do_id(&self) -> &str {
        &self.do_id
    }

    /// Returns the host's 32-byte secret key.
    pub fn host_secret(&self) -> &[u8; 32] {
        &self.host_secret
    }

    /// Returns a reference to the host's WebRTC peer manager.
    pub fn peer(&self) -> &WebRtcPeer {
        &self.peer
    }

    /// Checks whether the host signaling WebSocket has been severed (`REQ-004.5`).
    pub fn is_edge_severed(&self) -> bool {
        *self.is_socket_closed.borrow()
    }

    /// Explicitly closes the host signaling connection and severs edge connectivity.
    pub fn sever_edge(&self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(ref ws) = self.ws {
            let _ = ws.close();
        }
        *self.is_socket_closed.borrow_mut() = true;
    }

    /// Connects to the host signaling endpoint `wss://{domain}/host/{do_id}?secret={hex}` (`REQ-004.4`).
    pub fn connect(&mut self, ws_base_url: &str) -> Result<(), BootstrapError> {
        let clean_base = ws_base_url.trim_end_matches('/');
        if clean_base.is_empty() {
            return Err(BootstrapError::WebSocketError(
                "WebSocket base URL cannot be empty".to_string(),
            ));
        }

        #[cfg(target_arch = "wasm32")]
        {
            let secret_hex = hex::encode(self.host_secret);
            let ws_url = if clean_base.starts_with("ws://") || clean_base.starts_with("wss://") {
                format!("{clean_base}/host/{}?secret={secret_hex}", self.do_id)
            } else {
                format!("wss://{clean_base}/host/{}?secret={secret_hex}", self.do_id)
            };

            let ws = web_sys::WebSocket::new(&ws_url)
                .map_err(|e| BootstrapError::WebSocketError(format!("{e:?}")))?;

            ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

            // Incoming message handler: deserializes DropPayload and applies answer to peer
            let peer_clone = self.peer.clone();
            let onmessage = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                let data = event.data();
                if let Some(text) = data.as_string() {
                    if let Ok(drop) = from_json_str::<DropPayload>(&text) {
                        let peer = peer_clone.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let _ = peer.apply_answer(&drop.sdp_answer.sdp).await;
                        });
                    }
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>);
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            onmessage.forget();

            // On close handler
            let is_closed_mark = self.is_socket_closed.clone();
            let onclose = Closure::wrap(Box::new(move |_event: web_sys::CloseEvent| {
                *is_closed_mark.borrow_mut() = true;
            }) as Box<dyn FnMut(web_sys::CloseEvent)>);
            ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
            onclose.forget();

            // Edge severance trigger: immediately close WebSocket when WebRTC DataChannel transitions to Open
            let ws_for_severance = ws.clone();
            let is_closed_sev = self.is_socket_closed.clone();
            self.peer.on_state_change(move |state| {
                if state == PeerLifecycleState::Open {
                    let _ = ws_for_severance.close();
                    *is_closed_sev.borrow_mut() = true;
                }
            });

            if self.peer.state() == PeerLifecycleState::Open {
                let _ = ws.close();
                *self.is_socket_closed.borrow_mut() = true;
            }

            self.ws = Some(ws);
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.peer.state() == PeerLifecycleState::Open {
                *self.is_socket_closed.borrow_mut() = true;
            }
            Ok(())
        }
    }

    /// Handles an incoming raw drop message received over the signaling channel, applies the SDP answer
    /// to the host's WebRTC peer, and triggers edge severance upon channel opening.
    pub async fn handle_incoming_drop_message(
        &self,
        raw_message: &str,
    ) -> Result<DropPayload, BootstrapError> {
        let drop: DropPayload = from_json_str(raw_message)?;
        self.peer
            .apply_answer(&drop.sdp_answer.sdp)
            .await
            .map_err(BootstrapError::from)?;
        Ok(drop)
    }

    /// Applies an already-parsed [`DropPayload`] to the host's WebRTC peer.
    pub async fn handle_incoming_drop_payload(
        &self,
        drop: &DropPayload,
    ) -> Result<(), BootstrapError> {
        self.peer
            .apply_answer(&drop.sdp_answer.sdp)
            .await
            .map_err(BootstrapError::from)?;
        Ok(())
    }
}

/// Simulates in-memory the complete Pattern B bootstrap handshake:
/// 1. Host creates a signaling session and attaches edge severance triggers.
/// 2. Joiner executes auto-drop to produce [`DropPayload`].
/// 3. Host receives the drop answer and applies it.
/// 4. Verifies both peers reach [`PeerLifecycleState::Open`].
/// 5. Verifies host signaling session reflects severed edge state (`is_edge_severed() == true`).
pub async fn simulate_handshake_loopback(
    host_peer: &WebRtcPeer,
    joiner_peer: &WebRtcPeer,
    invite: &InvitePayload,
    joiner_pubkey: [u8; 32],
) -> Result<(), BootstrapError> {
    let mut host_session =
        HostSignalingSession::new(&invite.do_id, [0x42u8; 32], host_peer.clone());
    host_session.connect("wss://example.test")?;
    assert!(!host_session.is_edge_severed());

    // Joiner generates answer and dispatches drop payload
    let drop_payload = execute_joiner_autodrop(
        "https://example.test",
        invite,
        joiner_peer,
        joiner_pubkey,
    )
    .await?;

    // Host receives the dropped answer over signaling
    let drop_json = to_json_string(&drop_payload)?;
    host_session.handle_incoming_drop_message(&drop_json).await?;

    // Complete loopback pairing
    WebRtcPeer::pair_for_testing(host_peer, joiner_peer).map_err(BootstrapError::from)?;

    if !host_session.is_edge_severed() {
        return Err(BootstrapError::WebSocketError(
            "Host edge connection was not severed upon handshake completion".to_string(),
        ));
    }
    if host_peer.state() != PeerLifecycleState::Open {
        return Err(BootstrapError::WebRtc(WebRtcError::ChannelNotOpen(
            host_peer.state(),
        )));
    }
    if joiner_peer.state() != PeerLifecycleState::Open {
        return Err(BootstrapError::WebRtc(WebRtcError::ChannelNotOpen(
            joiner_peer.state(),
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invite::{create_offline_invite, pack_invite_fragment};
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    use zero_protocol::{ProtocolError, SdpOffer};

    fn block_on<F: Future>(fut: F) -> F::Output {
        fn noop_raw_waker() -> RawWaker {
            fn no_op(_: *const ()) {}
            fn clone(_: *const ()) -> RawWaker {
                noop_raw_waker()
            }
            static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
            RawWaker::new(std::ptr::null(), &VTABLE)
        }

        let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
        let mut cx = Context::from_waker(&waker);
        let mut pinned = pin!(fut);
        match pinned.as_mut().poll(&mut cx) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("Future remained pending in synchronous test runner"),
        }
    }

    fn dummy_offer() -> SdpOffer {
        SdpOffer {
            sdp: [
                "v=0",
                "o=- 12345 2 IN IP4 127.0.0.1",
                "s=-",
                "t=0 0",
                "a=group:BUNDLE 0",
                "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
                "c=IN IP4 0.0.0.0",
                "a=candidate:1 1 UDP 2130706431 127.0.0.1 50000 typ host",
                "a=end-of-candidates",
            ]
            .join("\r\n"),
        }
    }

    #[test]
    fn test_error_display_and_conversions() {
        let webrtc_err = WebRtcError::ChannelNotOpen(PeerLifecycleState::Connecting);
        let boot_webrtc: BootstrapError = webrtc_err.clone().into();
        assert_eq!(boot_webrtc, BootstrapError::WebRtc(webrtc_err));
        assert!(boot_webrtc.to_string().contains("WebRTC error"));

        let invite_err = InviteError::MissingPayload;
        let boot_invite: BootstrapError = invite_err.clone().into();
        assert_eq!(boot_invite, BootstrapError::Invite(invite_err));
        assert!(boot_invite.to_string().contains("Invite error"));

        let proto_err = ProtocolError::SerializationError("bad format".to_string());
        let boot_proto: BootstrapError = proto_err.into();
        assert_eq!(
            boot_proto,
            BootstrapError::Serialization("Serialization error: bad format".to_string())
        );

        assert_eq!(
            BootstrapError::HttpError {
                status: 404,
                message: "Not Found".to_string()
            }
            .to_string(),
            "HTTP error 404: Not Found"
        );
        assert_eq!(
            BootstrapError::FetchError("network down".to_string()).to_string(),
            "Fetch error: network down"
        );
        assert_eq!(
            BootstrapError::WebSocketError("disconnected".to_string()).to_string(),
            "WebSocket error: disconnected"
        );
        assert_eq!(
            BootstrapError::NoWindowOrLocation.to_string(),
            "Window or location object unavailable"
        );
        assert_eq!(
            BootstrapError::NoPayloadInHash.to_string(),
            "No invite payload found in URL hash"
        );
        assert_eq!(
            BootstrapError::Serialization("corrupt json".to_string()).to_string(),
            "Serialization error: corrupt json"
        );
    }

    #[test]
    fn test_extract_window_hash_native_returns_error() {
        let res = extract_window_hash();
        assert_eq!(res.unwrap_err(), BootstrapError::NoWindowOrLocation);
    }

    #[test]
    fn test_detect_invite_from_hash_valid_and_variants() {
        let host_secret = b"test-secret-key-for-bootstrap-32b";
        let invite = create_offline_invite(
            "do-room-xyz",
            host_secret,
            [1u8; 32],
            dummy_offer(),
            Some("uuid-test-1234"),
            Some([2u8; 32]),
        )
        .expect("create invite");

        let fragment = pack_invite_fragment(&invite).expect("pack fragment");

        // 1. Full fragment with #payload=...
        let detected = detect_invite_from_hash(&fragment).expect("detect from full fragment");
        assert_eq!(detected, invite);

        // 2. Full URL with path and fragment
        let full_url = format!("https://chat.zero.app/join{fragment}");
        let detected = detect_invite_from_hash(&full_url).expect("detect from full url");
        assert_eq!(detected, invite);

        // 3. Query-style without leading hash
        let query_style = fragment.trim_start_matches('#');
        let detected = detect_invite_from_hash(query_style).expect("detect from query style");
        assert_eq!(detected, invite);
    }

    #[test]
    fn test_detect_invite_from_hash_errors() {
        assert_eq!(
            detect_invite_from_hash("").unwrap_err(),
            BootstrapError::NoPayloadInHash
        );
        assert_eq!(
            detect_invite_from_hash("   ").unwrap_err(),
            BootstrapError::NoPayloadInHash
        );
        assert_eq!(
            detect_invite_from_hash("#").unwrap_err(),
            BootstrapError::NoPayloadInHash
        );

        // Missing payload parameter
        let err = detect_invite_from_hash("#other=value").unwrap_err();
        assert_eq!(err, BootstrapError::Invite(InviteError::MissingPayload));

        // Invalid Base64
        let err = detect_invite_from_hash("#payload=@@@").unwrap_err();
        match err {
            BootstrapError::Invite(InviteError::Base64(_)) => {}
            other => panic!("Expected Base64 error, got: {other:?}"),
        }

        // Invalid JSON
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let not_json = URL_SAFE_NO_PAD.encode(b"not json");
        let err = detect_invite_from_hash(&format!("#payload={not_json}")).unwrap_err();
        match err {
            BootstrapError::Invite(InviteError::Protocol(ProtocolError::SerializationError(_))) => {}
            other => panic!("Expected Protocol Serialization error, got: {other:?}"),
        }
    }

    #[test]
    fn test_dispatch_drop_post_mock_and_errors() {
        block_on(async {
            let payload = DropPayload {
                invite_uuid: "uuid-123".to_string(),
                token: "hmac-token".to_string(),
                sdp_answer: zero_protocol::SdpAnswer {
                    sdp: dummy_offer().sdp,
                },
                joiner_public_key: [3u8; 32],
            };

            // 1. Success on normal URL
            let status =
                dispatch_drop_post("https://edge.app/drop/do-1/uuid-123", "token-abc", &payload)
                    .await
                    .expect("dispatch success");
            assert_eq!(status, 200);

            // 2. Empty URL fails
            let err = dispatch_drop_post("", "token-abc", &payload)
                .await
                .unwrap_err();
            assert_eq!(
                err,
                BootstrapError::FetchError("Endpoint URL cannot be empty".to_string())
            );

            // 3. Empty token fails
            let err = dispatch_drop_post("https://edge.app/drop/do-1/uuid-123", "   ", &payload)
                .await
                .unwrap_err();
            match err {
                BootstrapError::HttpError { status, .. } => assert_eq!(status, 401),
                other => panic!("Expected HttpError 401, got {other:?}"),
            }

            // 4. Simulated HTTP errors
            let err = dispatch_drop_post(
                "https://edge.app/simulate_error/403",
                "token-abc",
                &payload,
            )
            .await
            .unwrap_err();
            match err {
                BootstrapError::HttpError { status, .. } => assert_eq!(status, 403),
                other => panic!("Expected HttpError 403, got {other:?}"),
            }

            let err = dispatch_drop_post(
                "https://edge.app/simulate_error/500",
                "token-abc",
                &payload,
            )
            .await
            .unwrap_err();
            match err {
                BootstrapError::HttpError { status, .. } => assert_eq!(status, 500),
                other => panic!("Expected HttpError 500, got {other:?}"),
            }
        });
    }

    #[test]
    fn test_execute_joiner_autodrop_flow() {
        block_on(async {
            let joiner_peer = WebRtcPeer::new();
            let invite = InvitePayload {
                do_id: "do-beta".to_string(),
                invite_uuid: "uuid-auto-drop".to_string(),
                token: "token-auto-drop".to_string(),
                sdp_offer: dummy_offer(),
                host_public_key: [5u8; 32],
                guest_private_key: None,
            };

            let drop = execute_joiner_autodrop(
                "https://edge.app",
                &invite,
                &joiner_peer,
                [9u8; 32],
            )
            .await
            .expect("execute joiner autodrop");

            assert_eq!(drop.invite_uuid, invite.invite_uuid);
            assert_eq!(drop.token, invite.token);
            assert_eq!(drop.joiner_public_key, [9u8; 32]);
            assert!(drop.sdp_answer.sdp.contains("v=0"));
            assert!(drop.sdp_answer.sdp.contains("m=application"));

            // Error with empty base URL
            let err = execute_joiner_autodrop("", &invite, &joiner_peer, [9u8; 32])
                .await
                .unwrap_err();
            assert_eq!(
                err,
                BootstrapError::FetchError("Origin base URL cannot be empty".to_string())
            );

            // Error with simulated HTTP failure
            let err = execute_joiner_autodrop(
                "https://edge.app/simulate_error/404",
                &invite,
                &joiner_peer,
                [9u8; 32],
            )
            .await
            .unwrap_err();
            match err {
                BootstrapError::HttpError { status, .. } => assert_eq!(status, 404),
                other => panic!("Expected HttpError 404, got {other:?}"),
            }
        });
    }

    #[test]
    fn test_host_signaling_session_and_edge_severance() {
        block_on(async {
            let host_peer = WebRtcPeer::new();
            let mut session =
                HostSignalingSession::new("do-room-01", [7u8; 32], host_peer.clone());

            assert_eq!(session.do_id(), "do-room-01");
            assert_eq!(session.host_secret(), &[7u8; 32]);
            assert!(!session.is_edge_severed());

            // Connect signaling
            session.connect("wss://edge.app").expect("connect");
            assert!(!session.is_edge_severed());

            // Verify Debug representation redacts secret
            let debug_str = format!("{session:?}");
            assert!(debug_str.contains("do-room-01"));
            assert!(debug_str.contains("[REDACTED]"));
            assert!(!debug_str.contains("7, 7, 7"));

            // Receive dropped answer
            let joiner_peer = WebRtcPeer::new();
            let answer = joiner_peer
                .accept_offer_and_create_answer(&dummy_offer().sdp)
                .await
                .expect("create answer");

            let drop = DropPayload {
                invite_uuid: "uuid-123".to_string(),
                token: "hmac-token".to_string(),
                sdp_answer: answer,
                joiner_public_key: [8u8; 32],
            };
            let drop_json = to_json_string(&drop).expect("serialize drop");

            session
                .handle_incoming_drop_message(&drop_json)
                .await
                .expect("handle drop message");

            // WebRTC peer transitioned to Open, so edge severance must have fired immediately
            assert_eq!(host_peer.state(), PeerLifecycleState::Open);
            assert!(session.is_edge_severed());

            // Explicit sever edge also sets flag
            let other_session =
                HostSignalingSession::new("do-room-02", [0u8; 32], WebRtcPeer::new());
            assert!(!other_session.is_edge_severed());
            other_session.sever_edge();
            assert!(other_session.is_edge_severed());
        });
    }

    #[test]
    fn test_simulate_handshake_loopback_e2e() {
        block_on(async {
            let host_peer = WebRtcPeer::new();
            let joiner_peer = WebRtcPeer::new();

            let host_offer = host_peer.create_offer().await.expect("host offer");
            let host_secret = b"host-room-secret-hmac-sha256-32b";
            let invite = create_offline_invite(
                "do-e2e-loopback",
                host_secret,
                [11u8; 32],
                host_offer,
                Some("uuid-e2e-loopback"),
                Some([22u8; 32]),
            )
            .expect("create invite");

            let joiner_pubkey = [33u8; 32];
            simulate_handshake_loopback(&host_peer, &joiner_peer, &invite, joiner_pubkey)
                .await
                .expect("handshake loopback");

            assert_eq!(host_peer.state(), PeerLifecycleState::Open);
            assert_eq!(joiner_peer.state(), PeerLifecycleState::Open);

            // Verify bidirectional data messaging over the opened channel
            let host_received_text = Rc::new(RefCell::new(String::new()));
            let host_received_clone = host_received_text.clone();
            host_peer.on_text_message(move |text| {
                *host_received_clone.borrow_mut() = text.to_string();
            });

            let joiner_received_bytes = Rc::new(RefCell::new(Vec::new()));
            let joiner_received_clone = joiner_received_bytes.clone();
            joiner_peer.on_binary_message(move |bytes| {
                *joiner_received_clone.borrow_mut() = bytes.to_vec();
            });

            joiner_peer
                .send_text("Hello Host over P2P DataChannel!")
                .expect("joiner sends text");
            assert_eq!(
                *host_received_text.borrow(),
                "Hello Host over P2P DataChannel!"
            );

            host_peer
                .send_binary(&[0xDE, 0xAD, 0xBE, 0xEF])
                .expect("host sends binary");
            assert_eq!(*joiner_received_bytes.borrow(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
        });
    }
}
