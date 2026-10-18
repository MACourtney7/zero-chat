//! Multi-peer WebRTC mesh topology manager with in-band SDP signaling relay routing,
//! direct channel snapping between secondary peers, message deduplication, and peer disconnect
//! lifecycle handling.
//!
//! Satisfies REQ-006.1 through REQ-006.5.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use zero_protocol::{
    crypto::x25519::X25519Keypair,
    envelope::{from_json_str, to_json_string, ChatMessage, MeshRelayMessage, MeshRelaySignal},
    ProtocolError,
};

use crate::crypto::{generate_message_uuid, ChatSessionCrypto, CryptoError};
use crate::webrtc::{PeerLifecycleState, WebRtcError, WebRtcPeer};

#[cfg(not(target_arch = "wasm32"))]
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    fn noop_raw_waker() -> std::task::RawWaker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> std::task::RawWaker {
            noop_raw_waker()
        }
        static VTABLE: std::task::RawWakerVTable =
            std::task::RawWakerVTable::new(clone, no_op, no_op, no_op);
        std::task::RawWaker::new(std::ptr::null(), &VTABLE)
    }

    let waker = unsafe { std::task::Waker::from_raw(noop_raw_waker()) };
    let mut cx = std::task::Context::from_waker(&waker);
    let mut pinned = std::pin::pin!(fut);
    match pinned.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(output) => output,
        std::task::Poll::Pending => panic!("Future remained pending in synchronous runner"),
    }
}

/// Errors returned by the mesh topology manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshError {
    PeerNotFound(String),
    DuplicatePeer(String),
    WebRtc(WebRtcError),
    Crypto(CryptoError),
    Protocol(ProtocolError),
    InvalidState(String),
    RelayFailed(String),
    ChannelNotOpen(String),
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeerNotFound(id) => write!(f, "Peer not found in mesh: {id}"),
            Self::DuplicatePeer(id) => write!(f, "Peer already exists in mesh: {id}"),
            Self::WebRtc(e) => write!(f, "WebRTC error: {e}"),
            Self::Crypto(e) => write!(f, "Crypto error: {e}"),
            Self::Protocol(e) => write!(f, "Protocol error: {e}"),
            Self::InvalidState(e) => write!(f, "Invalid mesh state: {e}"),
            Self::RelayFailed(e) => write!(f, "Relay forwarding failed: {e}"),
            Self::ChannelNotOpen(id) => write!(f, "Data channel not open for peer: {id}"),
        }
    }
}

impl std::error::Error for MeshError {}

impl From<WebRtcError> for MeshError {
    fn from(err: WebRtcError) -> Self {
        Self::WebRtc(err)
    }
}

impl From<CryptoError> for MeshError {
    fn from(err: CryptoError) -> Self {
        Self::Crypto(err)
    }
}

impl From<ProtocolError> for MeshError {
    fn from(err: ProtocolError) -> Self {
        Self::Protocol(err)
    }
}

/// A connected peer in the local mesh topology.
#[derive(Clone)]
pub struct ConnectedPeer {
    pub peer_id: String,
    pub public_key: [u8; 32],
    pub rtc_peer: WebRtcPeer,
    pub session_crypto: Option<ChatSessionCrypto>,
}

pub type MessageCallback = Rc<dyn Fn(&str, &str)>; // (sender_id, plaintext)
pub type PeerEventCallback = Rc<dyn Fn(&str)>; // (peer_id)

/// Alias for `MeshTopology` conforming to management layer nomenclature.
pub type MeshTopologyManager = MeshTopology;

/// Multi-peer WebRTC mesh topology manager.
#[derive(Clone)]
pub struct MeshTopology {
    inner: Rc<RefCell<MeshTopologyInner>>,
}

struct MeshTopologyInner {
    local_peer_id: String,
    local_private_key: [u8; 32],
    local_public_key: [u8; 32],
    peers: HashMap<String, ConnectedPeer>,
    pending_offers: HashMap<String, WebRtcPeer>, // target_peer_id -> pending WebRtcPeer
    known_public_keys: HashMap<String, [u8; 32]>,
    seen_messages: HashSet<String>,
    seen_message_order: Vec<String>,
    max_seen_cache: usize,
    has_burned: bool,
    on_message_callbacks: Vec<MessageCallback>,
    on_peer_joined_callbacks: Vec<PeerEventCallback>,
    on_peer_left_callbacks: Vec<PeerEventCallback>,
    on_burn_peer_messages_callbacks: Vec<Rc<dyn Fn(&str)>>,
    on_global_burn_callbacks: Vec<Rc<dyn Fn(&str)>>,
}

impl MeshTopology {
    /// Creates a new `MeshTopology` instance with the given peer ID and X25519 private key.
    pub fn new(local_peer_id: &str, local_private_key: [u8; 32]) -> Self {
        let keypair = X25519Keypair::from_private_key(local_private_key);
        let local_public_key = *keypair.public_key_bytes();

        Self {
            inner: Rc::new(RefCell::new(MeshTopologyInner {
                local_peer_id: local_peer_id.to_string(),
                local_private_key,
                local_public_key,
                peers: HashMap::new(),
                pending_offers: HashMap::new(),
                known_public_keys: HashMap::new(),
                seen_messages: HashSet::new(),
                seen_message_order: Vec::new(),
                max_seen_cache: 1024,
                has_burned: false,
                on_message_callbacks: Vec::new(),
                on_peer_joined_callbacks: Vec::new(),
                on_peer_left_callbacks: Vec::new(),
                on_burn_peer_messages_callbacks: Vec::new(),
                on_global_burn_callbacks: Vec::new(),
            })),
        }
    }

    /// Returns the local peer identifier.
    pub fn local_peer_id(&self) -> String {
        self.inner.borrow().local_peer_id.clone()
    }

    /// Returns the local X25519 public key.
    pub fn local_public_key(&self) -> [u8; 32] {
        self.inner.borrow().local_public_key
    }

    /// Registers a peer with an existing open or connecting `WebRtcPeer` channel.
    /// Hooks data channel message callbacks to handle incoming `MeshRelayMessage` signals.
    pub fn register_peer(
        &self,
        peer_id: &str,
        public_key: [u8; 32],
        rtc_peer: WebRtcPeer,
    ) -> Result<(), MeshError> {
        self.add_peer(peer_id, public_key, rtc_peer)
    }

    /// Connects a new peer, setting up session crypto and data channel callbacks.
    pub fn add_peer(
        &self,
        peer_id: &str,
        public_key: [u8; 32],
        rtc_peer: WebRtcPeer,
    ) -> Result<(), MeshError> {
        let (local_priv, existing_peers, joined_cbs) = {
            let mut inner = self.inner.borrow_mut();
            if inner.peers.contains_key(peer_id) {
                return Err(MeshError::DuplicatePeer(peer_id.to_string()));
            }
            inner
                .known_public_keys
                .insert(peer_id.to_string(), public_key);
            let existing: Vec<(String, WebRtcPeer, [u8; 32])> = inner
                .peers
                .iter()
                .map(|(id, p)| (id.clone(), p.rtc_peer.clone(), p.public_key))
                .collect();
            let cbs = inner.on_peer_joined_callbacks.clone();
            (inner.local_private_key, existing, cbs)
        };

        let session_crypto = Some(ChatSessionCrypto::from_keypair_and_peer_public(
            &local_priv,
            &public_key,
        ));

        // Hook data channel message listener
        let topology_msg = self.clone();
        let pid_msg = peer_id.to_string();
        rtc_peer.on_text_message(move |text| {
            #[cfg(target_arch = "wasm32")]
            {
                let topo = topology_msg.clone();
                let p = pid_msg.clone();
                let t = text.to_string();
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = topo.handle_incoming_message(&p, &t).await;
                });
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = block_on(topology_msg.handle_incoming_message(&pid_msg, text));
            }
        });

        // Hook data channel lifecycle listener for disconnect cleanup (REQ-006.5)
        let topology_state = self.clone();
        let pid_state = peer_id.to_string();
        rtc_peer.on_state_change(move |state| {
            if state == PeerLifecycleState::Closed {
                let _ = topology_state.remove_peer(&pid_state);
            }
        });

        // Store connected peer in routing table
        {
            let mut inner = self.inner.borrow_mut();
            inner.peers.insert(
                peer_id.to_string(),
                ConnectedPeer {
                    peer_id: peer_id.to_string(),
                    public_key,
                    rtc_peer: rtc_peer.clone(),
                    session_crypto,
                },
            );
        }

        // Inform existing peers of newly joined peer and vice versa (MeshRelaySignal::PeerJoined)
        for (existing_id, existing_rtc, existing_pk) in existing_peers {
            if let Ok(msg_to_existing) = to_json_string(&MeshRelayMessage {
                frame_id: generate_message_uuid().unwrap_or_else(|_| "join".to_string()),
                ttl: 3,
                signal: MeshRelaySignal::PeerJoined {
                    peer_id: peer_id.to_string(),
                    public_key,
                },
            }) {
                let _ = existing_rtc.send_text(&msg_to_existing);
            }

            if let Ok(msg_to_new) = to_json_string(&MeshRelayMessage {
                frame_id: generate_message_uuid().unwrap_or_else(|_| "join".to_string()),
                ttl: 3,
                signal: MeshRelaySignal::PeerJoined {
                    peer_id: existing_id,
                    public_key: existing_pk,
                },
            }) {
                let _ = rtc_peer.send_text(&msg_to_new);
            }
        }

        // Trigger on_peer_joined callbacks
        for cb in joined_cbs {
            cb(peer_id);
        }

        Ok(())
    }

    /// Removes a peer from the mesh, closes its data channel, and triggers `on_peer_left` callbacks (REQ-006.5).
    pub fn remove_peer(&self, peer_id: &str) -> Result<(), MeshError> {
        let (removed, callbacks) = {
            let mut inner = self.inner.borrow_mut();
            let peer = inner.peers.remove(peer_id);
            let cbs = inner.on_peer_left_callbacks.clone();
            (peer, cbs)
        };

        match removed {
            Some(peer) => {
                let _ = peer.rtc_peer.close();
                for cb in callbacks {
                    cb(peer_id);
                }
                Ok(())
            }
            None => Err(MeshError::PeerNotFound(peer_id.to_string())),
        }
    }

    /// Disconnects a peer from the mesh, closes its data channel, and triggers `on_peer_left` callbacks (REQ-006.5).
    /// Alias for `remove_peer`.
    pub fn disconnect_peer(&self, peer_id: &str) -> Result<(), MeshError> {
        self.remove_peer(peer_id)
    }

    /// Returns list of currently connected peer IDs.
    pub fn connected_peers(&self) -> Vec<String> {
        let mut list: Vec<String> = self.inner.borrow().peers.keys().cloned().collect();
        list.sort();
        list
    }

    /// Returns true if `peer_id` is currently in the active routing table.
    pub fn has_peer(&self, peer_id: &str) -> bool {
        self.inner.borrow().peers.contains_key(peer_id)
    }

    /// Returns the number of currently connected peers.
    pub fn peer_count(&self) -> usize {
        self.inner.borrow().peers.len()
    }

    /// Retrieves a snapshot of the connected peer, if present.
    pub fn get_peer(&self, peer_id: &str) -> Option<ConnectedPeer> {
        self.inner.borrow().peers.get(peer_id).cloned()
    }

    /// Manually records a known peer's public key (e.g. from an out-of-band discovery or invite).
    pub fn learn_peer_key(&self, peer_id: &str, public_key: [u8; 32]) {
        self.inner
            .borrow_mut()
            .known_public_keys
            .insert(peer_id.to_string(), public_key);
    }

    /// Sets the maximum deduplication cache capacity (useful for cache boundary testing).
    pub fn set_max_seen_cache_for_testing(&self, size: usize) {
        self.inner.borrow_mut().max_seen_cache = size;
    }

    /// Returns the number of message IDs currently stored in the deduplication cache.
    pub fn seen_messages_count(&self) -> usize {
        self.inner.borrow().seen_messages.len()
    }

    /// Returns whether the given message UUID is present in the deduplication cache.
    pub fn has_seen_message(&self, message_uuid: &str) -> bool {
        self.inner.borrow().seen_messages.contains(message_uuid)
    }

    /// Subscribes to decrypted chat messages.
    pub fn on_message<F: Fn(&str, &str) + 'static>(&self, callback: F) {
        self.inner
            .borrow_mut()
            .on_message_callbacks
            .push(Rc::new(callback));
    }

    /// Subscribes to peer joined events.
    pub fn on_peer_joined<F: Fn(&str) + 'static>(&self, callback: F) {
        self.inner
            .borrow_mut()
            .on_peer_joined_callbacks
            .push(Rc::new(callback));
    }

    /// Subscribes to peer left events.
    pub fn on_peer_left<F: Fn(&str) + 'static>(&self, callback: F) {
        self.inner
            .borrow_mut()
            .on_peer_left_callbacks
            .push(Rc::new(callback));
    }

    /// Subscribes to peer selective burn events.
    pub fn on_burn_peer_messages<F: Fn(&str) + 'static>(&self, callback: F) {
        self.inner
            .borrow_mut()
            .on_burn_peer_messages_callbacks
            .push(Rc::new(callback));
    }

    /// Subscribes to global session burn events.
    pub fn on_global_burn<F: Fn(&str) + 'static>(&self, callback: F) {
        self.inner
            .borrow_mut()
            .on_global_burn_callbacks
            .push(Rc::new(callback));
    }

    /// Initiates secondary in-band offer from this node to `target_peer_id` via `relay_peer_id` (REQ-006.2).
    pub async fn initiate_relay_handshake(
        &self,
        relay_peer_id: &str,
        target_peer_id: &str,
        target_public_key: [u8; 32],
    ) -> Result<(), MeshError> {
        let (relay_rtc, local_priv) = {
            let inner = self.inner.borrow();
            let relay = inner
                .peers
                .get(relay_peer_id)
                .map(|p| p.rtc_peer.clone())
                .ok_or_else(|| {
                    MeshError::PeerNotFound(format!("Relay peer '{relay_peer_id}' not found"))
                })?;
            (relay, inner.local_private_key)
        };

        // Cache target public key
        self.inner
            .borrow_mut()
            .known_public_keys
            .insert(target_peer_id.to_string(), target_public_key);

        // Create secondary WebRtcPeer and generate SDP offer
        let rtc_peer = WebRtcPeer::new();
        let offer = rtc_peer.create_offer().await?;

        // Seal offer using end-to-end shared secret (opaque to relay bridge Alice)
        let crypto =
            ChatSessionCrypto::from_keypair_and_peer_public(&local_priv, &target_public_key);
        let sealed_offer = crypto.seal_raw(offer.sdp.as_bytes())?;

        // Store pending offer awaiting RelaySdpAnswer
        {
            let mut inner = self.inner.borrow_mut();
            inner
                .pending_offers
                .insert(target_peer_id.to_string(), rtc_peer);
        }

        // Package and route through intermediate relay peer
        let relay_msg = MeshRelayMessage {
            frame_id: generate_message_uuid()?,
            ttl: 3,
            signal: MeshRelaySignal::RelaySdpOffer {
                from_peer_id: self.local_peer_id(),
                to_peer_id: target_peer_id.to_string(),
                sealed_offer,
            },
        };

        let raw_json = to_json_string(&relay_msg)?;
        relay_rtc.send_text(&raw_json)?;

        Ok(())
    }

    /// Dispatches an incoming raw JSON message received from `from_peer_id`.
    /// Handles opaque relaying (REQ-006.1), direct snap (REQ-006.2), broadcast (REQ-006.3),
    /// and deduplication (REQ-006.4).
    pub async fn handle_incoming_message(
        &self,
        from_peer_id: &str,
        raw_json: &str,
    ) -> Result<(), MeshError> {
        if let Ok(relay_msg) = from_json_str::<MeshRelayMessage>(raw_json) {
            match relay_msg.signal {
                MeshRelaySignal::PeerJoined {
                    peer_id,
                    public_key,
                } => {
                    if peer_id != self.local_peer_id() {
                        self.inner
                            .borrow_mut()
                            .known_public_keys
                            .insert(peer_id, public_key);
                    }
                    Ok(())
                }

                MeshRelaySignal::RelaySdpOffer {
                    from_peer_id: ref offer_from,
                    to_peer_id: ref offer_to,
                    ref sealed_offer,
                } => {
                    if offer_to != &self.local_peer_id() {
                        // REQ-006.1: Opaque Bridge Forwarding
                        // Bridge signaling messages between existing peers without inspecting or decrypting
                        let target_peer = {
                            let inner = self.inner.borrow();
                            inner
                                .peers
                                .get(offer_to)
                                .map(|p| p.rtc_peer.clone())
                                .ok_or_else(|| {
                                    MeshError::RelayFailed(format!(
                                        "Relay target peer '{offer_to}' not found in active mesh"
                                    ))
                                })?
                        };

                        let mut forwarded = relay_msg.clone();
                        if forwarded.ttl > 1 {
                            forwarded.ttl -= 1;
                            let fwd_json = to_json_string(&forwarded)?;
                            target_peer.send_text(&fwd_json)?;
                        }
                        Ok(())
                    } else {
                        // REQ-006.2: Direct Mesh Snap
                        // We are the target of the secondary offer
                        let sender_pk = {
                            let inner = self.inner.borrow();
                            inner.known_public_keys.get(offer_from).copied().ok_or_else(|| {
                                MeshError::PeerNotFound(format!(
                                    "Public key for peer '{offer_from}' unknown; cannot decrypt offer"
                                ))
                            })?
                        };

                        let local_priv = self.inner.borrow().local_private_key;
                        let crypto = ChatSessionCrypto::from_keypair_and_peer_public(
                            &local_priv,
                            &sender_pk,
                        );
                        let offer_bytes = crypto.open_raw(sealed_offer)?;
                        let offer_sdp = String::from_utf8(offer_bytes).map_err(|e| {
                            MeshError::Crypto(CryptoError::Utf8Error(e.to_string()))
                        })?;

                        let new_peer = WebRtcPeer::new();
                        let answer = new_peer.accept_offer_and_create_answer(&offer_sdp).await?;
                        let sealed_answer = crypto.seal_raw(answer.sdp.as_bytes())?;

                        // Route answer back through the transport relay from which the offer came
                        let relay_transport = {
                            let inner = self.inner.borrow();
                            inner
                                .peers
                                .get(from_peer_id)
                                .map(|p| p.rtc_peer.clone())
                                .ok_or_else(|| {
                                    MeshError::RelayFailed(format!(
                                        "Relay transport peer '{from_peer_id}' not found"
                                    ))
                                })?
                        };

                        let answer_relay = MeshRelayMessage {
                            frame_id: generate_message_uuid()?,
                            ttl: 3,
                            signal: MeshRelaySignal::RelaySdpAnswer {
                                from_peer_id: self.local_peer_id(),
                                to_peer_id: offer_from.clone(),
                                sealed_answer,
                            },
                        };

                        let answer_json = to_json_string(&answer_relay)?;
                        relay_transport.send_text(&answer_json)?;

                        // Add new direct peer to routing table
                        self.add_peer(offer_from, sender_pk, new_peer)?;
                        Ok(())
                    }
                }

                MeshRelaySignal::RelaySdpAnswer {
                    from_peer_id: ref answer_from,
                    to_peer_id: ref answer_to,
                    ref sealed_answer,
                } => {
                    if answer_to != &self.local_peer_id() {
                        // REQ-006.1: Opaque Bridge Forwarding
                        let target_peer = {
                            let inner = self.inner.borrow();
                            inner
                                .peers
                                .get(answer_to)
                                .map(|p| p.rtc_peer.clone())
                                .ok_or_else(|| {
                                    MeshError::RelayFailed(format!(
                                        "Relay target peer '{answer_to}' not found in active mesh"
                                    ))
                                })?
                        };

                        let mut forwarded = relay_msg.clone();
                        if forwarded.ttl > 1 {
                            forwarded.ttl -= 1;
                            let fwd_json = to_json_string(&forwarded)?;
                            target_peer.send_text(&fwd_json)?;
                        }
                        Ok(())
                    } else {
                        // REQ-006.2: Direct Mesh Snap
                        // We are the offerer receiving the answer
                        let (pending_peer, target_pk) = {
                            let mut inner = self.inner.borrow_mut();
                            let peer =
                                inner.pending_offers.remove(answer_from).ok_or_else(|| {
                                    MeshError::InvalidState(format!(
                                        "No pending offer found for peer '{answer_from}'"
                                    ))
                                })?;
                            let pk = inner
                                .known_public_keys
                                .get(answer_from)
                                .copied()
                                .ok_or_else(|| {
                                    MeshError::PeerNotFound(format!(
                                        "Public key for peer '{answer_from}' unknown"
                                    ))
                                })?;
                            (peer, pk)
                        };

                        let local_priv = self.inner.borrow().local_private_key;
                        let crypto = ChatSessionCrypto::from_keypair_and_peer_public(
                            &local_priv,
                            &target_pk,
                        );
                        let answer_bytes = crypto.open_raw(sealed_answer)?;
                        let answer_sdp = String::from_utf8(answer_bytes).map_err(|e| {
                            MeshError::Crypto(CryptoError::Utf8Error(e.to_string()))
                        })?;

                        pending_peer.apply_answer(&answer_sdp).await?;
                        self.add_peer(answer_from, target_pk, pending_peer)?;
                        Ok(())
                    }
                }

                MeshRelaySignal::BroadcastChat {
                    ref message_uuid,
                    ref origin_peer_id,
                    ttl,
                    ref sealed_message,
                } => {
                    // REQ-006.4: Deduplication Check
                    {
                        let mut inner = self.inner.borrow_mut();
                        if inner.seen_messages.contains(message_uuid) {
                            return Ok(()); // Suppress duplicate immediately
                        }
                        if inner.seen_message_order.len() >= inner.max_seen_cache {
                            let oldest = inner.seen_message_order.remove(0);
                            inner.seen_messages.remove(&oldest);
                        }
                        inner.seen_messages.insert(message_uuid.clone());
                        inner.seen_message_order.push(message_uuid.clone());
                    }

                    // Decrypt incoming payload (try from_peer_id session crypto first, then origin_peer_id)
                    let plaintext = {
                        let inner = self.inner.borrow();
                        let from_crypto = inner
                            .peers
                            .get(from_peer_id)
                            .and_then(|p| p.session_crypto.clone());
                        let origin_crypto = inner
                            .peers
                            .get(origin_peer_id)
                            .and_then(|p| p.session_crypto.clone());

                        let mut dec = None;
                        if let Some(ref c) = from_crypto {
                            if let Ok(bytes) = c.open_raw(sealed_message) {
                                if let Ok(s) = String::from_utf8(bytes) {
                                    dec = Some(s);
                                }
                            }
                        }
                        if dec.is_none() {
                            if let Some(ref c) = origin_crypto {
                                if let Ok(bytes) = c.open_raw(sealed_message) {
                                    if let Ok(s) = String::from_utf8(bytes) {
                                        dec = Some(s);
                                    }
                                }
                            }
                        }
                        dec.ok_or_else(|| {
                            MeshError::Crypto(CryptoError::DecryptionFailed(
                                "Failed to authenticate or decrypt broadcast chat message"
                                    .to_string(),
                            ))
                        })?
                    };

                    // REQ-006.3: Deliver chat message to local feed
                    let callbacks = self.inner.borrow().on_message_callbacks.clone();
                    for cb in callbacks {
                        cb(origin_peer_id, &plaintext);
                    }

                    // Flood to other directly connected peers if TTL > 1 (excluding incoming peer and origin)
                    if ttl > 1 {
                        let targets: Vec<(WebRtcPeer, Option<ChatSessionCrypto>)> = {
                            let inner = self.inner.borrow();
                            inner
                                .peers
                                .iter()
                                .filter(|(id, _)| *id != from_peer_id && *id != origin_peer_id)
                                .map(|(_, p)| (p.rtc_peer.clone(), p.session_crypto.clone()))
                                .collect()
                        };

                        for (rtc_peer, session_crypto) in targets {
                            if let Some(crypto) = session_crypto {
                                if let Ok(sealed) = crypto.seal_raw(plaintext.as_bytes()) {
                                    let fwd = MeshRelayMessage {
                                        frame_id: generate_message_uuid()
                                            .unwrap_or_else(|_| message_uuid.clone()),
                                        ttl: ttl - 1,
                                        signal: MeshRelaySignal::BroadcastChat {
                                            message_uuid: message_uuid.clone(),
                                            origin_peer_id: origin_peer_id.clone(),
                                            ttl: ttl - 1,
                                            sealed_message: sealed,
                                        },
                                    };
                                    if let Ok(json) = to_json_string(&fwd) {
                                        let _ = rtc_peer.send_text(&json);
                                    }
                                }
                            }
                        }
                    }

                    Ok(())
                }

                MeshRelaySignal::PeerLeftAndBurn { ref peer_id } => {
                    // Deduplicate using seen_messages / frame_id
                    {
                        let mut inner = self.inner.borrow_mut();
                        if inner.seen_messages.contains(&relay_msg.frame_id) {
                            return Ok(());
                        }
                        if inner.seen_message_order.len() >= inner.max_seen_cache {
                            let oldest = inner.seen_message_order.remove(0);
                            inner.seen_messages.remove(&oldest);
                        }
                        inner.seen_messages.insert(relay_msg.frame_id.clone());
                        inner.seen_message_order.push(relay_msg.frame_id.clone());
                    }

                    // Forward to other connected peers if TTL > 1 (excluding incoming peer and leaving peer)
                    if relay_msg.ttl > 1 {
                        let targets: Vec<WebRtcPeer> = {
                            let inner = self.inner.borrow();
                            inner
                                .peers
                                .iter()
                                .filter(|(id, _)| id.as_str() != from_peer_id && id.as_str() != peer_id)
                                .map(|(_, p)| p.rtc_peer.clone())
                                .collect()
                        };

                        let mut forwarded = relay_msg.clone();
                        forwarded.ttl -= 1;
                        if let Ok(fwd_json) = to_json_string(&forwarded) {
                            for rtc in targets {
                                let _ = rtc.send_text(&fwd_json);
                            }
                        }
                    }

                    // Remove peer_id and close its channel
                    let _ = self.remove_peer(peer_id);

                    // Fire all on_burn_peer_messages callbacks with peer_id
                    let callbacks = self.inner.borrow().on_burn_peer_messages_callbacks.clone();
                    for cb in callbacks {
                        cb(peer_id);
                    }

                    Ok(())
                }

                MeshRelaySignal::GlobalBurnSession {
                    ref initiator_peer_id,
                } => {
                    // Deduplicate if already burned
                    {
                        let mut inner = self.inner.borrow_mut();
                        if inner.has_burned {
                            return Ok(());
                        }
                        inner.has_burned = true;
                    }

                    // If relay_msg.ttl > 1, forward the message to other connected peers (excluding from_peer_id) before severing connections
                    if relay_msg.ttl > 1 {
                        let targets: Vec<WebRtcPeer> = {
                            let inner = self.inner.borrow();
                            inner
                                .peers
                                .iter()
                                .filter(|(id, _)| id.as_str() != from_peer_id)
                                .map(|(_, p)| p.rtc_peer.clone())
                                .collect()
                        };

                        let mut forwarded = relay_msg.clone();
                        forwarded.ttl -= 1;
                        if let Ok(fwd_json) = to_json_string(&forwarded) {
                            for rtc in targets {
                                let _ = rtc.send_text(&fwd_json);
                            }
                        }
                    }

                    // Sever all connections, close all rtc_peer instances, and clear internal tables
                    let (peers_to_close, pending_to_close, burn_cbs) = {
                        let mut inner = self.inner.borrow_mut();
                        let peers: Vec<WebRtcPeer> = inner.peers.values().map(|p| p.rtc_peer.clone()).collect();
                        let pending: Vec<WebRtcPeer> = inner.pending_offers.values().cloned().collect();
                        inner.peers.clear();
                        inner.pending_offers.clear();
                        inner.known_public_keys.clear();
                        inner.seen_messages.clear();
                        inner.seen_message_order.clear();
                        let cbs = inner.on_global_burn_callbacks.clone();
                        (peers, pending, cbs)
                    };

                    for p in peers_to_close {
                        let _ = p.close();
                    }
                    for p in pending_to_close {
                        let _ = p.close();
                    }

                    // Fire all on_global_burn callbacks with initiator_peer_id
                    for cb in burn_cbs {
                        cb(initiator_peer_id);
                    }

                    Ok(())
                }

                #[allow(unreachable_patterns)]
                _ => Ok(()),
            }
        } else if let Ok(chat_msg) = from_json_str::<ChatMessage>(raw_json) {
            // Backward-compatible direct ChatMessage handling
            {
                let mut inner = self.inner.borrow_mut();
                if inner.seen_messages.contains(&chat_msg.message_uuid) {
                    return Ok(());
                }
                if inner.seen_message_order.len() >= inner.max_seen_cache {
                    let oldest = inner.seen_message_order.remove(0);
                    inner.seen_messages.remove(&oldest);
                }
                inner.seen_messages.insert(chat_msg.message_uuid.clone());
                inner.seen_message_order.push(chat_msg.message_uuid.clone());
            }

            let crypto = self
                .inner
                .borrow()
                .peers
                .get(from_peer_id)
                .and_then(|p| p.session_crypto.clone());

            if let Some(crypto) = crypto {
                let plaintext = crypto.open_message(&chat_msg)?;
                let callbacks = self.inner.borrow().on_message_callbacks.clone();
                for cb in callbacks {
                    cb(&chat_msg.sender_id, &plaintext);
                }
            }
            Ok(())
        } else {
            Err(MeshError::Protocol(ProtocolError::SerializationError(
                "Message neither MeshRelayMessage nor ChatMessage".to_string(),
            )))
        }
    }

    /// Broadcasts a chat message across all directly connected peers in the mesh (REQ-006.3).
    /// Generates a unique message UUID, records it in the deduplication cache,
    /// and floods with initial TTL = 3.
    pub fn broadcast_chat(&self, plaintext: &str) -> Result<String, MeshError> {
        let message_uuid = generate_message_uuid()?;

        // Record in local deduplication cache
        {
            let mut inner = self.inner.borrow_mut();
            if inner.seen_messages.contains(&message_uuid) {
                return Ok(message_uuid);
            }
            if inner.seen_message_order.len() >= inner.max_seen_cache {
                let oldest = inner.seen_message_order.remove(0);
                inner.seen_messages.remove(&oldest);
            }
            inner.seen_messages.insert(message_uuid.clone());
            inner.seen_message_order.push(message_uuid.clone());
        }

        let peers: Vec<(WebRtcPeer, Option<ChatSessionCrypto>)> = {
            let inner = self.inner.borrow();
            inner
                .peers
                .values()
                .map(|p| (p.rtc_peer.clone(), p.session_crypto.clone()))
                .collect()
        };

        for (rtc_peer, session_crypto) in peers {
            if let Some(crypto) = session_crypto {
                let sealed_message = crypto.seal_raw(plaintext.as_bytes())?;
                let relay_msg = MeshRelayMessage {
                    frame_id: generate_message_uuid().unwrap_or_else(|_| message_uuid.clone()),
                    ttl: 3,
                    signal: MeshRelaySignal::BroadcastChat {
                        message_uuid: message_uuid.clone(),
                        origin_peer_id: self.local_peer_id(),
                        ttl: 3,
                        sealed_message,
                    },
                };
                let raw_json = to_json_string(&relay_msg)?;
                let _ = rtc_peer.send_text(&raw_json);
            }
        }

        Ok(message_uuid)
    }

    /// Broadcasts a leave and selective burn signal to all connected peers,
    /// closing local peer connections and cleaning up topology state (REQ-020.2).
    pub fn broadcast_leave_and_burn(&self) -> Result<(), MeshError> {
        let frame_id = generate_message_uuid()?;
        let relay_msg = MeshRelayMessage {
            frame_id: frame_id.clone(),
            ttl: 3,
            signal: MeshRelaySignal::PeerLeftAndBurn {
                peer_id: self.local_peer_id(),
            },
        };
        let raw_json = to_json_string(&relay_msg)?;

        // Record in local deduplication cache so echoed messages are suppressed
        {
            let mut inner = self.inner.borrow_mut();
            if inner.seen_message_order.len() >= inner.max_seen_cache {
                let oldest = inner.seen_message_order.remove(0);
                inner.seen_messages.remove(&oldest);
            }
            inner.seen_messages.insert(frame_id.clone());
            inner.seen_message_order.push(frame_id);
        }

        let (peers_to_close, pending_to_close) = {
            let inner = self.inner.borrow();
            let peers: Vec<WebRtcPeer> = inner.peers.values().map(|p| p.rtc_peer.clone()).collect();
            let pending: Vec<WebRtcPeer> = inner.pending_offers.values().cloned().collect();
            (peers, pending)
        };

        for p in &peers_to_close {
            let _ = p.send_text(&raw_json);
        }

        {
            let mut inner = self.inner.borrow_mut();
            inner.peers.clear();
            inner.pending_offers.clear();
        }

        for p in peers_to_close {
            let _ = p.close();
        }
        for p in pending_to_close {
            let _ = p.close();
        }

        Ok(())
    }

    /// Broadcasts a global burn signal across the entire mesh, severing all peer
    /// connections, clearing routing tables, and triggering local global burn callbacks (REQ-020.4).
    pub fn broadcast_global_burn(&self) -> Result<(), MeshError> {
        let local_id = self.local_peer_id();
        let frame_id = generate_message_uuid()?;
        let relay_msg = MeshRelayMessage {
            frame_id,
            ttl: 3,
            signal: MeshRelaySignal::GlobalBurnSession {
                initiator_peer_id: local_id.clone(),
            },
        };
        let raw_json = to_json_string(&relay_msg)?;

        let (peers_to_close, pending_to_close, burn_cbs) = {
            let mut inner = self.inner.borrow_mut();
            inner.has_burned = true;
            let peers: Vec<WebRtcPeer> = inner.peers.values().map(|p| p.rtc_peer.clone()).collect();
            let pending: Vec<WebRtcPeer> = inner.pending_offers.values().cloned().collect();
            let cbs = inner.on_global_burn_callbacks.clone();
            (peers, pending, cbs)
        };

        for p in &peers_to_close {
            let _ = p.send_text(&raw_json);
        }

        {
            let mut inner = self.inner.borrow_mut();
            inner.peers.clear();
            inner.pending_offers.clear();
            inner.known_public_keys.clear();
            inner.seen_messages.clear();
            inner.seen_message_order.clear();
        }

        for p in peers_to_close {
            let _ = p.close();
        }
        for p in pending_to_close {
            let _ = p.close();
        }

        for cb in burn_cbs {
            cb(&local_id);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_protocol::SealedFrame;

    fn generate_kp() -> ([u8; 32], [u8; 32]) {
        let kp = X25519Keypair::generate().expect("keypair");
        (*kp.private_key_bytes(), *kp.public_key_bytes())
    }

    #[test]
    fn test_mesh_peer_lifecycle_and_registration() {
        let (alice_sk, _alice_pk) = generate_kp();
        let alice_mesh = MeshTopology::new("alice", alice_sk);

        assert_eq!(alice_mesh.local_peer_id(), "alice");
        assert_eq!(alice_mesh.peer_count(), 0);
        assert!(!alice_mesh.has_peer("bob"));
        assert!(alice_mesh.connected_peers().is_empty());

        let joined_log = Rc::new(RefCell::new(Vec::new()));
        let left_log = Rc::new(RefCell::new(Vec::new()));

        let j_clone = joined_log.clone();
        alice_mesh.on_peer_joined(move |id| {
            j_clone.borrow_mut().push(id.to_string());
        });

        let l_clone = left_log.clone();
        alice_mesh.on_peer_left(move |id| {
            l_clone.borrow_mut().push(id.to_string());
        });

        let (_bob_sk, bob_pk) = generate_kp();
        let bob_peer = WebRtcPeer::new();

        // 1. Add Bob
        alice_mesh
            .add_peer("bob", bob_pk, bob_peer)
            .expect("add bob");
        assert_eq!(alice_mesh.peer_count(), 1);
        assert!(alice_mesh.has_peer("bob"));
        assert_eq!(alice_mesh.connected_peers(), vec!["bob".to_string()]);
        assert_eq!(*joined_log.borrow(), vec!["bob".to_string()]);

        // 2. Reject duplicate Bob
        let dup_peer = WebRtcPeer::new();
        let dup_err = alice_mesh.add_peer("bob", bob_pk, dup_peer).unwrap_err();
        assert_eq!(dup_err, MeshError::DuplicatePeer("bob".to_string()));

        // 3. Add Charlie via register_peer
        let (_charlie_sk, charlie_pk) = generate_kp();
        let charlie_peer = WebRtcPeer::new();
        alice_mesh
            .register_peer("charlie", charlie_pk, charlie_peer)
            .expect("register charlie");
        assert_eq!(alice_mesh.peer_count(), 2);
        assert_eq!(
            alice_mesh.connected_peers(),
            vec!["bob".to_string(), "charlie".to_string()]
        );
        assert_eq!(
            *joined_log.borrow(),
            vec!["bob".to_string(), "charlie".to_string()]
        );

        // 4. Remove Bob
        alice_mesh.remove_peer("bob").expect("remove bob");
        assert_eq!(alice_mesh.peer_count(), 1);
        assert!(!alice_mesh.has_peer("bob"));
        assert_eq!(alice_mesh.connected_peers(), vec!["charlie".to_string()]);
        assert_eq!(*left_log.borrow(), vec!["bob".to_string()]);

        // 5. Reject remove non-existent Bob
        let not_found_err = alice_mesh.remove_peer("bob").unwrap_err();
        assert_eq!(not_found_err, MeshError::PeerNotFound("bob".to_string()));
    }

    #[test]
    fn test_opaque_bridge_relay() {
        // REQ-006.1: Host bridges signaling messages between existing peers
        // without inspecting or decrypting their end-to-end payloads.
        let (alice_sk, alice_pk) = generate_kp();
        let (bob_sk, bob_pk) = generate_kp();
        let (charlie_sk, charlie_pk) = generate_kp();

        let alice_mesh = MeshTopology::new("alice", alice_sk);
        let bob_mesh = MeshTopology::new("bob", bob_sk);
        let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

        // Pre-seed known keys
        bob_mesh.learn_peer_key("charlie", charlie_pk);
        charlie_mesh.learn_peer_key("bob", bob_pk);

        // Connect Alice <-> Bob
        let alice_to_bob = WebRtcPeer::new();
        let bob_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).unwrap();
        alice_mesh
            .add_peer("bob", bob_pk, alice_to_bob)
            .expect("alice adds bob");
        bob_mesh
            .add_peer("alice", alice_pk, bob_to_alice)
            .expect("bob adds alice");

        // Connect Alice <-> Charlie
        let alice_to_charlie = WebRtcPeer::new();
        let charlie_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).unwrap();
        alice_mesh
            .add_peer("charlie", charlie_pk, alice_to_charlie)
            .expect("alice adds charlie");
        charlie_mesh
            .add_peer("alice", alice_pk, charlie_to_alice)
            .expect("charlie adds alice");

        // Charlie generates an SDP offer targeted at Bob
        let charlie_bob = WebRtcPeer::new();
        let offer = block_on(charlie_bob.create_offer()).unwrap();

        // Charlie seals the offer using Charlie-Bob shared secret
        let charlie_crypto = ChatSessionCrypto::from_keypair_and_peer_public(&charlie_sk, &bob_pk);
        let sealed_offer = charlie_crypto
            .seal_raw(offer.sdp.as_bytes())
            .expect("seal offer");

        let offer_msg = MeshRelayMessage {
            frame_id: "relay-offer-001".to_string(),
            ttl: 3,
            signal: MeshRelaySignal::RelaySdpOffer {
                from_peer_id: "charlie".to_string(),
                to_peer_id: "bob".to_string(),
                sealed_offer: sealed_offer.clone(),
            },
        };
        let offer_json = to_json_string(&offer_msg).unwrap();

        // Alice relays offer from Charlie to Bob without decrypting (REQ-006.1)
        block_on(alice_mesh.handle_incoming_message("charlie", &offer_json)).unwrap();

        // Check Bob received the relayed offer and automatically accepted it
        assert!(bob_mesh.has_peer("charlie"));
        let bob_charlie_peer = bob_mesh.get_peer("charlie").unwrap().rtc_peer;
        assert!(bob_charlie_peer.remote_offer().is_some());
        assert_eq!(bob_charlie_peer.remote_offer().unwrap().sdp, offer.sdp);

        // Check that Bob sent RelaySdpAnswer back through Alice
        // Charlie's pending offers would have applied it in initiate_relay_handshake
    }

    #[test]
    fn test_direct_mesh_snap_without_cloud() {
        // REQ-006.2: Third participant establishes direct connection via in-band bridge
        // without cloud edge requests.
        let (alice_sk, alice_pk) = generate_kp();
        let (bob_sk, bob_pk) = generate_kp();
        let (charlie_sk, charlie_pk) = generate_kp();

        let alice_mesh = MeshTopology::new("alice", alice_sk);
        let bob_mesh = MeshTopology::new("bob", bob_sk);
        let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

        // Pre-seed known keys
        bob_mesh.learn_peer_key("charlie", charlie_pk);
        charlie_mesh.learn_peer_key("bob", bob_pk);

        // Connect Alice <-> Bob
        let alice_to_bob = WebRtcPeer::new();
        let bob_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).unwrap();
        alice_mesh
            .add_peer("bob", bob_pk, alice_to_bob)
            .expect("alice adds bob");
        bob_mesh
            .add_peer("alice", alice_pk, bob_to_alice)
            .expect("bob adds alice");

        // Connect Alice <-> Charlie
        let alice_to_charlie = WebRtcPeer::new();
        let charlie_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).unwrap();
        alice_mesh
            .add_peer("charlie", charlie_pk, alice_to_charlie)
            .expect("alice adds charlie");
        charlie_mesh
            .add_peer("alice", alice_pk, charlie_to_alice)
            .expect("charlie adds alice");

        // Charlie initiates in-band relay handshake to Bob via Alice
        block_on(charlie_mesh.initiate_relay_handshake("alice", "bob", bob_pk)).unwrap();

        // Direct snap established: both Charlie and Bob now have direct peers for each other
        assert!(charlie_mesh.has_peer("bob"));
        assert!(bob_mesh.has_peer("charlie"));
        assert_eq!(charlie_mesh.peer_count(), 2); // alice + bob
        assert_eq!(bob_mesh.peer_count(), 2); // alice + charlie

        // Pair the snapped peers for direct loopback messaging
        let charlie_to_bob = charlie_mesh.get_peer("bob").unwrap().rtc_peer;
        let bob_to_charlie = bob_mesh.get_peer("charlie").unwrap().rtc_peer;
        WebRtcPeer::pair_for_testing(&charlie_to_bob, &bob_to_charlie).unwrap();

        // Register on_message callback on Bob
        let bob_received = Rc::new(RefCell::new(Vec::new()));
        let b_clone = bob_received.clone();
        bob_mesh.on_message(move |sender, msg| {
            b_clone
                .borrow_mut()
                .push((sender.to_string(), msg.to_string()));
        });

        // Charlie broadcasts chat directly to all connected peers
        let _uuid = charlie_mesh.broadcast_chat("Direct snap hello!").unwrap();

        // Bob receives message directly from Charlie
        assert_eq!(
            *bob_received.borrow(),
            vec![("charlie".to_string(), "Direct snap hello!".to_string())]
        );
    }

    #[test]
    fn test_broadcast_flooding_and_deduplication() {
        // REQ-006.3 & REQ-006.4: Broadcast delivering chat and deduplicating loops
        let (alice_sk, alice_pk) = generate_kp();
        let (bob_sk, bob_pk) = generate_kp();
        let (charlie_sk, charlie_pk) = generate_kp();

        let alice_mesh = MeshTopology::new("alice", alice_sk);
        let bob_mesh = MeshTopology::new("bob", bob_sk);
        let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

        // Fully connected mesh (triangle)
        let alice_to_bob = WebRtcPeer::new();
        let bob_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).unwrap();
        alice_mesh.add_peer("bob", bob_pk, alice_to_bob).unwrap();
        bob_mesh.add_peer("alice", alice_pk, bob_to_alice).unwrap();

        let alice_to_charlie = WebRtcPeer::new();
        let charlie_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).unwrap();
        alice_mesh
            .add_peer("charlie", charlie_pk, alice_to_charlie)
            .unwrap();
        charlie_mesh
            .add_peer("alice", alice_pk, charlie_to_alice)
            .unwrap();

        let bob_to_charlie = WebRtcPeer::new();
        let charlie_to_bob = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).unwrap();
        bob_mesh
            .add_peer("charlie", charlie_pk, bob_to_charlie)
            .unwrap();
        charlie_mesh
            .add_peer("bob", bob_pk, charlie_to_bob)
            .unwrap();

        // Listeners on Alice, Bob, Charlie
        let alice_feed = Rc::new(RefCell::new(Vec::new()));
        let a_clone = alice_feed.clone();
        alice_mesh.on_message(move |sender, msg| {
            a_clone
                .borrow_mut()
                .push((sender.to_string(), msg.to_string()));
        });

        let charlie_feed = Rc::new(RefCell::new(Vec::new()));
        let c_clone = charlie_feed.clone();
        charlie_mesh.on_message(move |sender, msg| {
            c_clone
                .borrow_mut()
                .push((sender.to_string(), msg.to_string()));
        });

        let bob_feed = Rc::new(RefCell::new(Vec::new()));
        let b_clone = bob_feed.clone();
        bob_mesh.on_message(move |sender, msg| {
            b_clone
                .borrow_mut()
                .push((sender.to_string(), msg.to_string()));
        });

        // Bob broadcasts message to the mesh
        let uuid = bob_mesh.broadcast_chat("Hello mesh world!").unwrap();

        // Assert Alice received exactly once
        assert_eq!(alice_feed.borrow().len(), 1);
        assert_eq!(
            alice_feed.borrow()[0],
            ("bob".to_string(), "Hello mesh world!".to_string())
        );

        // Assert Charlie received exactly once despite triangle flooding loop
        assert_eq!(charlie_feed.borrow().len(), 1);
        assert_eq!(
            charlie_feed.borrow()[0],
            ("bob".to_string(), "Hello mesh world!".to_string())
        );

        // Assert Bob suppressed loopback (did not receive his own broadcast)
        assert_eq!(bob_feed.borrow().len(), 0);

        // Duplicate delivery test: re-inject the exact same broadcast message
        let alice_crypto = ChatSessionCrypto::from_keypair_and_peer_public(&alice_sk, &bob_pk);
        let sealed = alice_crypto
            .seal_raw("Hello mesh world!".as_bytes())
            .unwrap();
        let dup_msg = MeshRelayMessage {
            frame_id: "dup-frame-001".to_string(),
            ttl: 2,
            signal: MeshRelaySignal::BroadcastChat {
                message_uuid: uuid,
                origin_peer_id: "bob".to_string(),
                ttl: 2,
                sealed_message: sealed,
            },
        };
        let dup_json = to_json_string(&dup_msg).unwrap();

        // Manually dispatch to Alice again
        block_on(alice_mesh.handle_incoming_message("bob", &dup_json)).unwrap();

        // Alice message count remains 1 (duplicate suppressed!)
        assert_eq!(alice_feed.borrow().len(), 1);

        // Deduplication cache capacity eviction test
        alice_mesh.set_max_seen_cache_for_testing(2);
        for i in 1..=3 {
            let u = format!("test-uuid-{i}");
            let s = alice_crypto.seal_raw(b"evict").unwrap();
            let msg = MeshRelayMessage {
                frame_id: format!("frame-{i}"),
                ttl: 2,
                signal: MeshRelaySignal::BroadcastChat {
                    message_uuid: u,
                    origin_peer_id: "bob".to_string(),
                    ttl: 2,
                    sealed_message: s,
                },
            };
            let j = to_json_string(&msg).unwrap();
            block_on(alice_mesh.handle_incoming_message("bob", &j)).unwrap();
        }

        // test-uuid-1 should have been evicted, so re-sending it will be accepted again
        let s1 = alice_crypto.seal_raw(b"evict").unwrap();
        let msg1 = MeshRelayMessage {
            frame_id: "frame-re-1".to_string(),
            ttl: 2,
            signal: MeshRelaySignal::BroadcastChat {
                message_uuid: "test-uuid-1".to_string(),
                origin_peer_id: "bob".to_string(),
                ttl: 2,
                sealed_message: s1,
            },
        };
        let j1 = to_json_string(&msg1).unwrap();
        block_on(alice_mesh.handle_incoming_message("bob", &j1)).unwrap();
        // Alice count should now be 1 (original) + 3 (new) + 1 (re-admitted) = 5
        assert_eq!(alice_feed.borrow().len(), 5);
    }

    #[test]
    fn test_peer_disconnect_cleanup() {
        // REQ-006.5: Disconnection of a peer is detected, tearing down associated channels.
        let (alice_sk, _alice_pk) = generate_kp();
        let (_bob_sk, bob_pk) = generate_kp();

        let alice_mesh = MeshTopology::new("alice", alice_sk);

        let left_log = Rc::new(RefCell::new(Vec::new()));
        let l_clone = left_log.clone();
        alice_mesh.on_peer_left(move |id| {
            l_clone.borrow_mut().push(id.to_string());
        });

        let bob_peer = WebRtcPeer::new();
        alice_mesh
            .add_peer("bob", bob_pk, bob_peer.clone())
            .unwrap();
        assert_eq!(alice_mesh.peer_count(), 1);
        assert!(alice_mesh.has_peer("bob"));

        // Trigger close on WebRtcPeer
        bob_peer.close().unwrap();

        // Disconnect detected via state change
        assert_eq!(alice_mesh.peer_count(), 0);
        assert!(!alice_mesh.has_peer("bob"));
        assert_eq!(*left_log.borrow(), vec!["bob".to_string()]);
    }

    #[test]
    fn test_error_display_and_conversions() {
        let err1 = MeshError::PeerNotFound("peer-x".to_string());
        assert_eq!(err1.to_string(), "Peer not found in mesh: peer-x");

        let err2 = MeshError::DuplicatePeer("peer-y".to_string());
        assert_eq!(err2.to_string(), "Peer already exists in mesh: peer-y");

        let err3 = MeshError::RelayFailed("no route".to_string());
        assert_eq!(err3.to_string(), "Relay forwarding failed: no route");

        let err4 = MeshError::ChannelNotOpen("peer-z".to_string());
        assert_eq!(err4.to_string(), "Data channel not open for peer: peer-z");

        let err5 = MeshError::InvalidState("corrupted".to_string());
        assert_eq!(err5.to_string(), "Invalid mesh state: corrupted");

        let webrtc_err: MeshError = WebRtcError::InvalidSdp("bad sdp".to_string()).into();
        assert!(matches!(webrtc_err, MeshError::WebRtc(_)));

        let crypto_err: MeshError = CryptoError::MissingSealedFrame.into();
        assert!(matches!(crypto_err, MeshError::Crypto(_)));

        let proto_err: MeshError = ProtocolError::ExpiredTtl.into();
        assert!(matches!(proto_err, MeshError::Protocol(_)));
    }

    #[test]
    fn test_relay_to_unknown_peer_fails() {
        let (alice_sk, _alice_pk) = generate_kp();
        let alice_mesh = MeshTopology::new("alice", alice_sk);

        let relay_msg = MeshRelayMessage {
            frame_id: "unknown-tgt-01".to_string(),
            ttl: 2,
            signal: MeshRelaySignal::RelaySdpOffer {
                from_peer_id: "bob".to_string(),
                to_peer_id: "nobody".to_string(),
                sealed_offer: SealedFrame {
                    nonce: [0u8; 12],
                    ciphertext: vec![1, 2, 3],
                },
            },
        };
        let json = to_json_string(&relay_msg).unwrap();
        let res = block_on(alice_mesh.handle_incoming_message("bob", &json));
        assert!(matches!(res, Err(MeshError::RelayFailed(_))));
    }

    #[test]
    fn test_invalid_incoming_json() {
        let (alice_sk, _alice_pk) = generate_kp();
        let alice_mesh = MeshTopology::new("alice", alice_sk);

        let res = block_on(alice_mesh.handle_incoming_message("bob", "not json"));
        assert!(matches!(res, Err(MeshError::Protocol(_))));
    }

    #[test]
    fn test_broadcast_leave_and_burn() {
        let (alice_sk, alice_pk) = generate_kp();
        let (bob_sk, bob_pk) = generate_kp();
        let (charlie_sk, charlie_pk) = generate_kp();

        let alice_mesh = MeshTopology::new("alice", alice_sk);
        let bob_mesh = MeshTopology::new("bob", bob_sk);
        let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

        // Fully connected mesh (triangle)
        let alice_to_bob = WebRtcPeer::new();
        let bob_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).unwrap();
        alice_mesh.add_peer("bob", bob_pk, alice_to_bob).unwrap();
        bob_mesh.add_peer("alice", alice_pk, bob_to_alice).unwrap();

        let alice_to_charlie = WebRtcPeer::new();
        let charlie_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).unwrap();
        alice_mesh
            .add_peer("charlie", charlie_pk, alice_to_charlie)
            .unwrap();
        charlie_mesh
            .add_peer("alice", alice_pk, charlie_to_alice)
            .unwrap();

        let bob_to_charlie = WebRtcPeer::new();
        let charlie_to_bob = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).unwrap();
        bob_mesh
            .add_peer("charlie", charlie_pk, bob_to_charlie)
            .unwrap();
        charlie_mesh
            .add_peer("bob", bob_pk, charlie_to_bob)
            .unwrap();

        assert_eq!(alice_mesh.peer_count(), 2);
        assert_eq!(bob_mesh.peer_count(), 2);
        assert_eq!(charlie_mesh.peer_count(), 2);

        let alice_burn_log = Rc::new(RefCell::new(Vec::new()));
        let a_log = alice_burn_log.clone();
        alice_mesh.on_burn_peer_messages(move |peer_id| {
            a_log.borrow_mut().push(peer_id.to_string());
        });

        let charlie_burn_log = Rc::new(RefCell::new(Vec::new()));
        let c_log = charlie_burn_log.clone();
        charlie_mesh.on_burn_peer_messages(move |peer_id| {
            c_log.borrow_mut().push(peer_id.to_string());
        });

        bob_mesh.broadcast_leave_and_burn().unwrap();

        assert_eq!(*alice_burn_log.borrow(), vec!["bob".to_string()]);
        assert_eq!(*charlie_burn_log.borrow(), vec!["bob".to_string()]);

        assert!(!alice_mesh.has_peer("bob"));
        assert!(alice_mesh.has_peer("charlie"));
        assert_eq!(alice_mesh.peer_count(), 1);

        assert!(!charlie_mesh.has_peer("bob"));
        assert!(charlie_mesh.has_peer("alice"));
        assert_eq!(charlie_mesh.peer_count(), 1);

        assert_eq!(bob_mesh.peer_count(), 0);
    }

    #[test]
    fn test_broadcast_global_burn() {
        let (alice_sk, alice_pk) = generate_kp();
        let (bob_sk, bob_pk) = generate_kp();
        let (charlie_sk, charlie_pk) = generate_kp();

        let alice_mesh = MeshTopology::new("alice", alice_sk);
        let bob_mesh = MeshTopology::new("bob", bob_sk);
        let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

        // Fully connected mesh (triangle)
        let alice_to_bob = WebRtcPeer::new();
        let bob_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).unwrap();
        alice_mesh.add_peer("bob", bob_pk, alice_to_bob).unwrap();
        bob_mesh.add_peer("alice", alice_pk, bob_to_alice).unwrap();

        let alice_to_charlie = WebRtcPeer::new();
        let charlie_to_alice = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).unwrap();
        alice_mesh
            .add_peer("charlie", charlie_pk, alice_to_charlie)
            .unwrap();
        charlie_mesh
            .add_peer("alice", alice_pk, charlie_to_alice)
            .unwrap();

        let bob_to_charlie = WebRtcPeer::new();
        let charlie_to_bob = WebRtcPeer::new();
        WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).unwrap();
        bob_mesh
            .add_peer("charlie", charlie_pk, bob_to_charlie)
            .unwrap();
        charlie_mesh
            .add_peer("bob", bob_pk, charlie_to_bob)
            .unwrap();

        let alice_global_burn = Rc::new(RefCell::new(Vec::new()));
        let a_burn = alice_global_burn.clone();
        alice_mesh.on_global_burn(move |initiator| {
            a_burn.borrow_mut().push(initiator.to_string());
        });

        let bob_global_burn = Rc::new(RefCell::new(Vec::new()));
        let b_burn = bob_global_burn.clone();
        bob_mesh.on_global_burn(move |initiator| {
            b_burn.borrow_mut().push(initiator.to_string());
        });

        let charlie_global_burn = Rc::new(RefCell::new(Vec::new()));
        let c_burn = charlie_global_burn.clone();
        charlie_mesh.on_global_burn(move |initiator| {
            c_burn.borrow_mut().push(initiator.to_string());
        });

        alice_mesh.broadcast_global_burn().unwrap();

        assert_eq!(*alice_global_burn.borrow(), vec!["alice".to_string()]);
        assert_eq!(*bob_global_burn.borrow(), vec!["alice".to_string()]);
        assert_eq!(*charlie_global_burn.borrow(), vec!["alice".to_string()]);

        assert_eq!(alice_mesh.peer_count(), 0);
        assert_eq!(bob_mesh.peer_count(), 0);
        assert_eq!(charlie_mesh.peer_count(), 0);

        assert_eq!(alice_mesh.seen_messages_count(), 0);
        assert_eq!(bob_mesh.seen_messages_count(), 0);
        assert_eq!(charlie_mesh.seen_messages_count(), 0);
    }

    #[test]
    fn test_mesh_topology_manager_alias() {
        let (alice_sk, _alice_pk) = generate_kp();
        let mgr: MeshTopologyManager = MeshTopology::new("alice", alice_sk);
        assert_eq!(mgr.local_peer_id(), "alice");
    }
}
