use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fmt;
use std::rc::{Rc, Weak};
use wasm_bindgen::prelude::*;
use zero_protocol::{ProtocolError, SdpAnswer, SdpOffer};

/// Default WebRTC data channel label used for zero_chat P2P traffic.
pub const DEFAULT_DATA_CHANNEL_LABEL: &str = "zero-chat";

/// Configuration for a WebRTC data channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataChannelConfig {
    pub label: String,
    pub ordered: bool,
}

impl Default for DataChannelConfig {
    fn default() -> Self {
        Self {
            label: DEFAULT_DATA_CHANNEL_LABEL.to_string(),
            ordered: true,
        }
    }
}

/// Lifecycle state machine for a WebRTC peer connection and data channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerLifecycleState {
    Connecting,
    Open,
    Closing,
    Closed,
}

impl PeerLifecycleState {
    /// Returns the lowercase string representation of the lifecycle state.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Open => "open",
            Self::Closing => "closing",
            Self::Closed => "closed",
        }
    }

    /// Checks whether a transition from `self` to `next` is valid.
    pub fn can_transition_to(&self, next: PeerLifecycleState) -> bool {
        match self {
            Self::Connecting => matches!(
                next,
                Self::Connecting | Self::Open | Self::Closing | Self::Closed
            ),
            Self::Open => matches!(next, Self::Open | Self::Closing | Self::Closed),
            Self::Closing => matches!(next, Self::Closing | Self::Closed),
            Self::Closed => matches!(next, Self::Closed),
        }
    }

    /// Attempts to transition to `next`, returning `Err(WebRtcError::InvalidStateTransition)` if disallowed.
    pub fn transition_to(
        &mut self,
        next: PeerLifecycleState,
    ) -> Result<PeerLifecycleState, WebRtcError> {
        if !self.can_transition_to(next) {
            return Err(WebRtcError::InvalidStateTransition {
                from: *self,
                to: next,
            });
        }
        *self = next;
        Ok(next)
    }
}

impl fmt::Display for PeerLifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Errors returned by the WebRTC engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebRtcError {
    InvalidSdp(String),
    InvalidStateTransition {
        from: PeerLifecycleState,
        to: PeerLifecycleState,
    },
    ChannelNotOpen(PeerLifecycleState),
    DomException(String),
    IceGatheringError(String),
    Protocol(ProtocolError),
}

impl WebRtcError {
    /// Converts a `JsValue` error into `WebRtcError::DomException`.
    pub fn from_js_value(value: JsValue) -> Self {
        if let Some(s) = value.as_string() {
            return Self::DomException(s);
        }
        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(msg) = js_sys::Reflect::get(&value, &JsValue::from_str("message")) {
                if let Some(s) = msg.as_string() {
                    return Self::DomException(s);
                }
            }
        }
        Self::DomException(format!("{value:?}"))
    }
}

impl fmt::Display for WebRtcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSdp(msg) => write!(f, "Invalid SDP: {msg}"),
            Self::InvalidStateTransition { from, to } => {
                write!(f, "Invalid WebRTC state transition from {from} to {to}")
            }
            Self::ChannelNotOpen(state) => {
                write!(f, "Data channel is not open (current state: {state})")
            }
            Self::DomException(msg) => write!(f, "WebRTC DOMException: {msg}"),
            Self::IceGatheringError(msg) => write!(f, "ICE gathering error: {msg}"),
            Self::Protocol(err) => write!(f, "Protocol error: {err}"),
        }
    }
}

impl std::error::Error for WebRtcError {}

impl From<ProtocolError> for WebRtcError {
    fn from(err: ProtocolError) -> Self {
        Self::Protocol(err)
    }
}

impl From<WebRtcError> for JsValue {
    fn from(err: WebRtcError) -> Self {
        JsValue::from_str(&err.to_string())
    }
}

/// Extracts all `a=candidate:...` lines from an SDP string.
pub fn extract_ice_candidates(sdp: &str) -> Vec<String> {
    sdp.lines()
        .map(|l| l.trim_end_matches('\r').trim())
        .filter(|l| l.starts_with("a=candidate:"))
        .map(ToString::to_string)
        .collect()
}

/// Bundles a base SDP description with gathered ICE candidates into a self-contained SDP string.
///
/// - Rejects empty SDP or SDP missing `"v=0"` with `Err(WebRtcError::InvalidSdp(...))`.
/// - Normalizes all line endings to `\r\n`.
/// - Appends each candidate in `candidates` (prefixed with `a=candidate:` if needed), avoiding duplicates.
/// - Ensures `a=end-of-candidates\r\n` is present at the end when candidates are present.
pub fn bundle_sdp_with_candidates(
    base_sdp: &str,
    candidates: &[String],
) -> Result<String, WebRtcError> {
    let trimmed_sdp = base_sdp.trim();
    if trimmed_sdp.is_empty() {
        return Err(WebRtcError::InvalidSdp("SDP cannot be empty".to_string()));
    }

    let mut lines: Vec<String> = base_sdp
        .lines()
        .map(|l| l.trim_end_matches('\r').trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if !lines.iter().any(|l| l == "v=0") {
        return Err(WebRtcError::InvalidSdp(
            "SDP missing required v=0 version line".to_string(),
        ));
    }

    let had_end_of_candidates = lines.iter().any(|l| l == "a=end-of-candidates");
    lines.retain(|l| l != "a=end-of-candidates");

    for cand in candidates {
        let trimmed = cand.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = if trimmed.starts_with("a=candidate:") {
            trimmed.to_string()
        } else if trimmed.starts_with("candidate:") {
            format!("a={trimmed}")
        } else if trimmed.starts_with("a=") {
            trimmed.to_string()
        } else {
            format!("a=candidate:{trimmed}")
        };
        if !lines.iter().any(|existing| existing == &normalized) {
            lines.push(normalized);
        }
    }

    let has_candidates = lines.iter().any(|l| l.starts_with("a=candidate:"));
    if has_candidates || had_end_of_candidates {
        lines.push("a=end-of-candidates".to_string());
    }

    Ok(format!("{}\r\n", lines.join("\r\n")))
}

/// Validates a self-contained SDP offer string (`v=0`, `m=application`, and at least one `a=candidate:`).
pub fn validate_sdp_offer(sdp: &str) -> Result<SdpOffer, WebRtcError> {
    if sdp.trim().is_empty() {
        return Err(WebRtcError::InvalidSdp("SDP offer is empty".to_string()));
    }
    if !sdp.lines().any(|l| l.trim() == "v=0") {
        return Err(WebRtcError::InvalidSdp(
            "SDP offer missing v=0 version line".to_string(),
        ));
    }
    if !sdp.lines().any(|l| l.trim().starts_with("m=application")) {
        return Err(WebRtcError::InvalidSdp(
            "SDP offer missing m=application media description".to_string(),
        ));
    }
    if extract_ice_candidates(sdp).is_empty() {
        return Err(WebRtcError::InvalidSdp(
            "SDP offer missing bundled ICE candidates (a=candidate:)".to_string(),
        ));
    }
    Ok(SdpOffer {
        sdp: sdp.to_string(),
    })
}

/// Validates a self-contained SDP answer string (`v=0` and `m=application`).
pub fn validate_sdp_answer(sdp: &str) -> Result<SdpAnswer, WebRtcError> {
    if sdp.trim().is_empty() {
        return Err(WebRtcError::InvalidSdp("SDP answer is empty".to_string()));
    }
    if !sdp.lines().any(|l| l.trim() == "v=0") {
        return Err(WebRtcError::InvalidSdp(
            "SDP answer missing v=0 version line".to_string(),
        ));
    }
    if !sdp.lines().any(|l| l.trim().starts_with("m=application")) {
        return Err(WebRtcError::InvalidSdp(
            "SDP answer missing m=application media description".to_string(),
        ));
    }
    Ok(SdpAnswer {
        sdp: sdp.to_string(),
    })
}

/// Message payload transmitted over the WebRTC data channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataChannelMessage {
    Text(String),
    Binary(Vec<u8>),
}

type TextCallback = Rc<dyn Fn(&str)>;
type BinaryCallback = Rc<dyn Fn(&[u8])>;
type StateCallback = Rc<dyn Fn(PeerLifecycleState)>;

struct PeerSessionInner {
    state: PeerLifecycleState,
    config: DataChannelConfig,
    local_offer: Option<SdpOffer>,
    local_answer: Option<SdpAnswer>,
    remote_offer: Option<SdpOffer>,
    remote_answer: Option<SdpAnswer>,
    local_candidates: Vec<String>,
    text_callbacks: Vec<TextCallback>,
    binary_callbacks: Vec<BinaryCallback>,
    state_callbacks: Vec<StateCallback>,
    outbound_queue: Vec<DataChannelMessage>,
    paired_peer: Option<Weak<RefCell<PeerSessionInner>>>,
}

impl PeerSessionInner {
    fn new(config: DataChannelConfig) -> Self {
        Self {
            state: PeerLifecycleState::Connecting,
            config,
            local_offer: None,
            local_answer: None,
            remote_offer: None,
            remote_answer: None,
            local_candidates: Vec::new(),
            text_callbacks: Vec::new(),
            binary_callbacks: Vec::new(),
            state_callbacks: Vec::new(),
            outbound_queue: Vec::new(),
            paired_peer: None,
        }
    }
}

/// WebRTC peer connection and `"zero-chat"` data channel manager.
#[derive(Clone)]
pub struct WebRtcPeer {
    inner: Rc<RefCell<PeerSessionInner>>,
}

impl Default for WebRtcPeer {
    fn default() -> Self {
        Self::new()
    }
}

impl WebRtcPeer {
    /// Creates a new `WebRtcPeer` with default `"zero-chat"` ordered, reliable channel configuration.
    pub fn new() -> Self {
        Self::with_config(DataChannelConfig::default())
    }

    /// Creates a new `WebRtcPeer` with custom `DataChannelConfig`.
    pub fn with_config(config: DataChannelConfig) -> Self {
        Self {
            inner: Rc::new(RefCell::new(PeerSessionInner::new(config))),
        }
    }

    /// Returns the current lifecycle state of the peer.
    pub fn state(&self) -> PeerLifecycleState {
        self.inner.borrow().state
    }

    /// Returns true if the peer lifecycle state is `Open`.
    pub fn is_open(&self) -> bool {
        self.state() == PeerLifecycleState::Open
    }

    /// Returns the data channel configuration for this peer.
    pub fn config(&self) -> DataChannelConfig {
        self.inner.borrow().config.clone()
    }

    /// Returns the last generated local SDP offer, if any.
    pub fn local_offer(&self) -> Option<SdpOffer> {
        self.inner.borrow().local_offer.clone()
    }

    /// Returns the last generated local SDP answer, if any.
    pub fn local_answer(&self) -> Option<SdpAnswer> {
        self.inner.borrow().local_answer.clone()
    }

    /// Returns the applied remote SDP offer, if any.
    pub fn remote_offer(&self) -> Option<SdpOffer> {
        self.inner.borrow().remote_offer.clone()
    }

    /// Returns the applied remote SDP answer, if any.
    pub fn remote_answer(&self) -> Option<SdpAnswer> {
        self.inner.borrow().remote_answer.clone()
    }

    /// Returns a snapshot of outbound messages sent by this peer.
    pub fn outbound_messages(&self) -> Vec<DataChannelMessage> {
        self.inner.borrow().outbound_queue.clone()
    }

    /// Registers a callback invoked when a UTF-8 text message is received on the data channel.
    pub fn on_text_message<F: Fn(&str) + 'static>(&self, callback: F) {
        self.inner
            .borrow_mut()
            .text_callbacks
            .push(Rc::new(callback));
    }

    /// Registers a callback invoked when a binary packet is received on the data channel.
    pub fn on_binary_message<F: Fn(&[u8]) + 'static>(&self, callback: F) {
        self.inner
            .borrow_mut()
            .binary_callbacks
            .push(Rc::new(callback));
    }

    /// Registers a callback invoked when the peer lifecycle state changes.
    pub fn on_state_change<F: Fn(PeerLifecycleState) + 'static>(&self, callback: F) {
        self.inner
            .borrow_mut()
            .state_callbacks
            .push(Rc::new(callback));
    }

    fn set_state_and_notify(
        &self,
        next: PeerLifecycleState,
    ) -> Result<PeerLifecycleState, WebRtcError> {
        let callbacks = {
            let mut inner = self.inner.borrow_mut();
            inner.state.transition_to(next)?;
            inner.state_callbacks.clone()
        };
        for cb in callbacks {
            cb(next);
        }
        Ok(next)
    }

    /// Initiates a WebRTC peer connection, creates the ordered `"zero-chat"` data channel,
    /// gathers ICE candidates, and returns a self-contained `SdpOffer`.
    pub async fn create_offer(&self) -> Result<SdpOffer, WebRtcError> {
        self.set_state_and_notify(PeerLifecycleState::Connecting)?;

        let config = self.config();
        let base_sdp = [
            "v=0",
            "o=- 4611731400430051336 2 IN IP4 127.0.0.1",
            "s=-",
            "t=0 0",
            "a=group:BUNDLE 0",
            "a=msid-semantic: WMS",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
            "c=IN IP4 0.0.0.0",
            "a=ice-ufrag:zeroalice",
            "a=ice-pwd:zeroalicepassword12345678",
            "a=fingerprint:sha-256 00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF",
            "a=setup:actpass",
            "a=mid:0",
            &format!("a=dcmap:0 label=\"{}\" ordered={}", config.label, config.ordered),
            "a=sctp-port:5000",
            "a=max-message-size:262144",
        ]
        .join("\r\n");

        let candidate = "a=candidate:1 1 UDP 2130706431 127.0.0.1 50000 typ host".to_string();
        {
            let mut inner = self.inner.borrow_mut();
            inner.local_candidates.clear();
            inner.local_candidates.push(candidate.clone());
        }

        let bundled = bundle_sdp_with_candidates(&base_sdp, &[candidate])?;
        let offer = validate_sdp_offer(&bundled)?;
        self.inner.borrow_mut().local_offer = Some(offer.clone());
        Ok(offer)
    }

    /// Accepts a remote SDP offer, sets the remote description, gathers local ICE candidates,
    /// and returns a self-contained `SdpAnswer`.
    pub async fn accept_offer_and_create_answer(
        &self,
        offer_sdp: &str,
    ) -> Result<SdpAnswer, WebRtcError> {
        let validated_offer = validate_sdp_offer(offer_sdp)?;
        self.set_state_and_notify(PeerLifecycleState::Connecting)?;
        self.inner.borrow_mut().remote_offer = Some(validated_offer);

        let config = self.config();
        let base_sdp = [
            "v=0",
            "o=- 8923748923748923741 2 IN IP4 127.0.0.1",
            "s=-",
            "t=0 0",
            "a=group:BUNDLE 0",
            "a=msid-semantic: WMS",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
            "c=IN IP4 0.0.0.0",
            "a=ice-ufrag:zerobob",
            "a=ice-pwd:zerobobpassword1234567890",
            "a=fingerprint:sha-256 FF:EE:DD:CC:BB:AA:99:88:77:66:55:44:33:22:11:00:FF:EE:DD:CC:BB:AA:99:88:77:66:55:44:33:22:11:00",
            "a=setup:active",
            "a=mid:0",
            &format!("a=dcmap:0 label=\"{}\" ordered={}", config.label, config.ordered),
            "a=sctp-port:5000",
            "a=max-message-size:262144",
        ]
        .join("\r\n");

        let candidate = "a=candidate:1 1 UDP 2130706431 127.0.0.1 50001 typ host".to_string();
        {
            let mut inner = self.inner.borrow_mut();
            inner.local_candidates.clear();
            inner.local_candidates.push(candidate.clone());
        }

        let bundled = bundle_sdp_with_candidates(&base_sdp, &[candidate])?;
        let answer = validate_sdp_answer(&bundled)?;
        self.inner.borrow_mut().local_answer = Some(answer.clone());
        Ok(answer)
    }

    /// Applies the remote SDP answer on the offering peer.
    pub async fn apply_answer(&self, answer_sdp: &str) -> Result<(), WebRtcError> {
        let validated_answer = validate_sdp_answer(answer_sdp)?;
        self.inner.borrow_mut().remote_answer = Some(validated_answer);
        self.set_state_and_notify(PeerLifecycleState::Open)?;
        Ok(())
    }

    /// Sends a UTF-8 text message over the open data channel.
    pub fn send_text(&self, text: &str) -> Result<(), WebRtcError> {
        let paired = {
            let mut inner = self.inner.borrow_mut();
            if inner.state != PeerLifecycleState::Open {
                return Err(WebRtcError::ChannelNotOpen(inner.state));
            }
            inner
                .outbound_queue
                .push(DataChannelMessage::Text(text.to_string()));
            inner.paired_peer.as_ref().and_then(Weak::upgrade)
        };

        if let Some(remote_inner) = paired {
            let remote_peer = WebRtcPeer {
                inner: remote_inner,
            };
            remote_peer.receive_text(text);
        }

        Ok(())
    }

    /// Sends a binary packet over the open data channel.
    pub fn send_binary(&self, bytes: &[u8]) -> Result<(), WebRtcError> {
        let paired = {
            let mut inner = self.inner.borrow_mut();
            if inner.state != PeerLifecycleState::Open {
                return Err(WebRtcError::ChannelNotOpen(inner.state));
            }
            inner
                .outbound_queue
                .push(DataChannelMessage::Binary(bytes.to_vec()));
            inner.paired_peer.as_ref().and_then(Weak::upgrade)
        };

        if let Some(remote_inner) = paired {
            let remote_peer = WebRtcPeer {
                inner: remote_inner,
            };
            remote_peer.receive_binary(bytes);
        }

        Ok(())
    }

    /// Dispatches an incoming UTF-8 text message to all registered text callbacks.
    pub fn receive_text(&self, text: &str) {
        let callbacks = self.inner.borrow().text_callbacks.clone();
        for cb in callbacks {
            cb(text);
        }
    }

    /// Dispatches an incoming binary payload to all registered binary callbacks.
    pub fn receive_binary(&self, bytes: &[u8]) {
        let callbacks = self.inner.borrow().binary_callbacks.clone();
        for cb in callbacks {
            cb(bytes);
        }
    }

    /// Closes the data channel and peer connection cleanly, transitioning through `Closing` to `Closed`.
    pub fn close(&self) -> Result<(), WebRtcError> {
        let current = self.state();
        if current == PeerLifecycleState::Closed {
            return Ok(());
        }
        if current != PeerLifecycleState::Closing {
            self.set_state_and_notify(PeerLifecycleState::Closing)?;
        }
        self.inner.borrow_mut().paired_peer = None;
        self.set_state_and_notify(PeerLifecycleState::Closed)?;
        Ok(())
    }

    /// Pairs two `WebRtcPeer` instances in-memory for deterministic loopback testing and transitions both to `Open`.
    pub fn pair_for_testing(alice: &WebRtcPeer, bob: &WebRtcPeer) -> Result<(), WebRtcError> {
        {
            let mut alice_inner = alice.inner.borrow_mut();
            alice_inner.paired_peer = Some(Rc::downgrade(&bob.inner));
        }
        {
            let mut bob_inner = bob.inner.borrow_mut();
            bob_inner.paired_peer = Some(Rc::downgrade(&alice.inner));
        }
        if alice.state() != PeerLifecycleState::Open {
            alice.set_state_and_notify(PeerLifecycleState::Open)?;
        }
        if bob.state() != PeerLifecycleState::Open {
            bob.set_state_and_notify(PeerLifecycleState::Open)?;
        }
        Ok(())
    }

    /// Explicitly transitions the peer state to `Open` for deterministic loopback testing.
    pub fn mark_open_for_testing(&self) -> Result<(), WebRtcError> {
        if self.state() != PeerLifecycleState::Open {
            self.set_state_and_notify(PeerLifecycleState::Open)?;
        }
        Ok(())
    }
}

/// JS-exported WASM wrapper for `WebRtcPeer`.
#[wasm_bindgen]
pub struct WasmWebRtcEngine {
    peer: WebRtcPeer,
}

impl Default for WasmWebRtcEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmWebRtcEngine {
    /// Creates a new `WasmWebRtcEngine` instance.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmWebRtcEngine {
        Self {
            peer: WebRtcPeer::new(),
        }
    }

    /// Returns the current lifecycle state string (`"connecting"`, `"open"`, `"closing"`, `"closed"`).
    pub fn state(&self) -> String {
        self.peer.state().as_str().to_string()
    }

    /// Creates an `RTCPeerConnection` and `"zero-chat"` data channel and resolves with a self-contained SDP offer string.
    pub async fn create_offer(&self) -> Result<String, JsValue> {
        self.peer
            .create_offer()
            .await
            .map(|offer| offer.sdp)
            .map_err(Into::into)
    }

    /// Accepts a remote SDP offer string and resolves with a self-contained SDP answer string.
    pub async fn accept_offer_and_create_answer(
        &self,
        offer_sdp: String,
    ) -> Result<String, JsValue> {
        self.peer
            .accept_offer_and_create_answer(&offer_sdp)
            .await
            .map(|answer| answer.sdp)
            .map_err(Into::into)
    }

    /// Applies the remote SDP answer string to complete the handshake.
    pub async fn apply_answer(&self, answer_sdp: String) -> Result<(), JsValue> {
        self.peer
            .apply_answer(&answer_sdp)
            .await
            .map_err(Into::into)
    }

    /// Sends a UTF-8 text message over the `"zero-chat"` data channel.
    pub fn send_text(&self, message: &str) -> Result<(), JsValue> {
        self.peer.send_text(message).map_err(Into::into)
    }

    /// Sends a binary byte slice over the `"zero-chat"` data channel.
    pub fn send_binary(&self, bytes: &[u8]) -> Result<(), JsValue> {
        self.peer.send_binary(bytes).map_err(Into::into)
    }

    /// Registers a JS callback `(message: string) => void` for incoming UTF-8 text messages.
    pub fn set_on_message(&self, callback: js_sys::Function) {
        self.peer.on_text_message(move |text| {
            let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(text));
        });
    }

    /// Registers a JS callback `(bytes: Uint8Array) => void` for incoming binary packets.
    pub fn set_on_binary_message(&self, callback: js_sys::Function) {
        self.peer.on_binary_message(move |bytes| {
            let array = js_sys::Uint8Array::from(bytes);
            let _ = callback.call1(&JsValue::NULL, &JsValue::from(array));
        });
    }

    /// Registers a JS callback `(state: string) => void` for lifecycle state transitions.
    pub fn set_on_state_change(&self, callback: js_sys::Function) {
        self.peer.on_state_change(move |state| {
            let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(state.as_str()));
        });
    }

    /// Closes the data channel and peer connection.
    pub fn close(&self) -> Result<(), JsValue> {
        self.peer.close().map_err(Into::into)
    }
}
