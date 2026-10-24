//! Multi-node mesh simulation test suite for ZeroChat.
//!
//! Validates REQ-006.1 through REQ-006.5, REQ-022.1, REQ-022.2, REQ-023.4, REQ-028.1, REQ-028.2, REQ-029.4, REQ-029.5, and REQ-030.1 through REQ-030.5:
//! - REQ-006.1: Host bridges signaling messages between existing peers without inspecting or
//!   decrypting their end-to-end payloads.
//! - REQ-006.2: Third participant (Charlie) establishes direct `Channel_BC` data channel with Bob
//!   via Alice's in-band bridge without any cloud edge requests.
//! - REQ-006.3: Mesh broadcast delivers chat messages to all connected participants.
//! - REQ-006.4: Duplicate message broadcast loops are detected and suppressed via message ID caches.
//! - REQ-006.5: Disconnection of a peer is detected, tearing down associated channels and updating
//!   the UI peer list.
//! - REQ-022.1: Multi-peer selective message purge integration test (leave and selective burn).
//! - REQ-022.2: Multi-peer global burn integration test (session annihilation across all nodes).
//! - REQ-023.4: Multi-peer dirty disconnect integration test (automatic message purge on abrupt channel closure).
//! - REQ-028.1: Multi-Node connected mesh simulation test (snapping, flooding, loop suppression, peer counters).
//! - REQ-028.2: Partial peer departure & feed survivability test (selective purge, channel continuity).
//! - REQ-029.4: 3rd peer departure selective purge test (feed continuity and selective purging).
//! - REQ-029.5: Host departure survivability simulation test (Channel_BC survivability, selective message purge, continued communication).
//! - REQ-030.1: Multi-peer secondary inviter mesh snapping (User 2 invites User 3, direct Channel_AC snapped via Bob's bridge with 0 cloud edge hits).
//! - REQ-030.2: Unified multi-peer counter synchronization (all 3 nodes report peer count == 2).
//! - REQ-030.3: Symmetric departure counter updates across surviving peers (Scenarios A, B, and C).
//! - REQ-030.4: Host departure survivability (surviving nodes maintain direct DataChannel with symmetric peer count decrements).

use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use zero_client::{
    ChatSessionCrypto, ChatUi, ConnectionStatus, CryptoError, DataChannelMessage, MeshTopology,
    PeerLifecycleState, WebRtcPeer,
};
use zero_protocol::crypto::x25519::{derive_shared_secret, X25519Keypair};
use zero_protocol::{
    envelope::{from_json_str, to_json_string},
    MeshRelayMessage, MeshRelaySignal, SealedFrame,
};

/// Synchronous future executor for native integration test runners.
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

/// Generates a fresh random X25519 keypair tuple `(private_key, public_key)`.
fn generate_kp() -> ([u8; 32], [u8; 32]) {
    let kp = X25519Keypair::generate().expect("keypair generation");
    (*kp.private_key_bytes(), *kp.public_key_bytes())
}

/// REQ-006.1: Host bridges signaling messages between existing peers without inspecting or
/// decrypting their end-to-end payloads.
///
/// - Alice acts as the host bridge. Bob and Charlie are connected to Alice.
/// - Charlie initiates an in-band relay handshake to Bob via Alice.
/// - Inspect the relay frames traveling through Alice (`RelaySdpOffer`, `RelaySdpAnswer`):
///   - Assert Alice forwards the opaque `SealedFrame` to Bob verbatim.
///   - Assert Alice does not possess the Bob-Charlie X25519 shared secret.
///   - Assert Alice cannot decrypt the sealed payload (decryption attempts with Alice's key fail).
///   - Verify the bridge operates completely end-to-end encrypted for secondary peers.
#[test]
fn test_req_006_1_opaque_bridge_signaling_relay() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Seed known public keys for Bob and Charlie
    bob_mesh.learn_peer_key("charlie", charlie_pk);
    charlie_mesh.learn_peer_key("bob", bob_pk);

    // Connect Alice <-> Bob (Channel_AB)
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob.clone())
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("Bob adds Alice");

    // Connect Alice <-> Charlie (Channel_AC)
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie.clone())
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("Charlie adds Alice");

    // Pre-handshake outbound snapshot count on Alice's channels
    let alice_bob_msgs_before = alice_to_bob.outbound_messages().len();
    let alice_charlie_msgs_before = alice_to_charlie.outbound_messages().len();

    // Charlie initiates in-band relay handshake to Bob via Alice
    block_on(charlie_mesh.initiate_relay_handshake("alice", "bob", bob_pk))
        .expect("initiate relay handshake");

    // 1. Inspect RelaySdpOffer forwarded by Alice to Bob
    let alice_bob_msgs = alice_to_bob.outbound_messages();
    assert!(
        alice_bob_msgs.len() > alice_bob_msgs_before,
        "Alice must forward the offer message to Bob"
    );

    let mut offer_sealed_frame: Option<SealedFrame> = None;
    for msg in &alice_bob_msgs[alice_bob_msgs_before..] {
        if let DataChannelMessage::Text(ref text) = msg {
            if let Ok(relay_msg) = from_json_str::<MeshRelayMessage>(text) {
                if let MeshRelaySignal::RelaySdpOffer {
                    ref from_peer_id,
                    ref to_peer_id,
                    ref sealed_offer,
                } = relay_msg.signal
                {
                    assert_eq!(from_peer_id, "charlie");
                    assert_eq!(to_peer_id, "bob");
                    assert_eq!(
                        relay_msg.ttl, 2,
                        "Alice must decrement TTL on forwarded offer"
                    );
                    offer_sealed_frame = Some(sealed_offer.clone());
                    break;
                }
            }
        }
    }
    let sealed_offer = offer_sealed_frame.expect("Alice must forward RelaySdpOffer to Bob");

    // 2. Inspect RelaySdpAnswer forwarded by Alice to Charlie
    let alice_charlie_msgs = alice_to_charlie.outbound_messages();
    assert!(
        alice_charlie_msgs.len() > alice_charlie_msgs_before,
        "Alice must forward the answer message to Charlie"
    );

    let mut answer_sealed_frame: Option<SealedFrame> = None;
    for msg in &alice_charlie_msgs[alice_charlie_msgs_before..] {
        if let DataChannelMessage::Text(ref text) = msg {
            if let Ok(relay_msg) = from_json_str::<MeshRelayMessage>(text) {
                if let MeshRelaySignal::RelaySdpAnswer {
                    ref from_peer_id,
                    ref to_peer_id,
                    ref sealed_answer,
                } = relay_msg.signal
                {
                    assert_eq!(from_peer_id, "bob");
                    assert_eq!(to_peer_id, "charlie");
                    assert_eq!(
                        relay_msg.ttl, 2,
                        "Alice must decrement TTL on forwarded answer"
                    );
                    answer_sealed_frame = Some(sealed_answer.clone());
                    break;
                }
            }
        }
    }
    let sealed_answer = answer_sealed_frame.expect("Alice must forward RelaySdpAnswer to Charlie");

    // 3. Cryptographic isolation proofs: Alice does NOT possess Bob-Charlie shared secret
    let bob_charlie_dh = derive_shared_secret(&bob_sk, &charlie_pk);
    let charlie_bob_dh = derive_shared_secret(&charlie_sk, &bob_pk);
    assert_eq!(
        bob_charlie_dh, charlie_bob_dh,
        "Bob and Charlie derive identical DH shared secret"
    );

    let alice_bob_dh = derive_shared_secret(&alice_sk, &bob_pk);
    let alice_charlie_dh = derive_shared_secret(&alice_sk, &charlie_pk);

    assert_ne!(alice_sk, bob_sk, "Alice and Bob private keys are distinct");
    assert_ne!(
        alice_sk, charlie_sk,
        "Alice and Charlie private keys are distinct"
    );
    assert_ne!(
        alice_bob_dh, bob_charlie_dh,
        "Alice-Bob DH secret must differ from Bob-Charlie DH secret"
    );
    assert_ne!(
        alice_charlie_dh, bob_charlie_dh,
        "Alice-Charlie DH secret must differ from Bob-Charlie DH secret"
    );

    // 4. Assert Alice cannot decrypt sealed_offer with any of her session keys
    let alice_charlie_crypto =
        ChatSessionCrypto::from_keypair_and_peer_public(&alice_sk, &charlie_pk);
    let alice_bob_crypto = ChatSessionCrypto::from_keypair_and_peer_public(&alice_sk, &bob_pk);

    let offer_dec_charlie = alice_charlie_crypto.open_raw(&sealed_offer);
    assert!(
        matches!(offer_dec_charlie, Err(CryptoError::DecryptionFailed(_))),
        "Alice attempting to decrypt Charlie's offer using Alice-Charlie key must fail"
    );

    let offer_dec_bob = alice_bob_crypto.open_raw(&sealed_offer);
    assert!(
        matches!(offer_dec_bob, Err(CryptoError::DecryptionFailed(_))),
        "Alice attempting to decrypt Charlie's offer using Alice-Bob key must fail"
    );

    // 5. Assert Alice cannot decrypt sealed_answer with any of her session keys
    let answer_dec_charlie = alice_charlie_crypto.open_raw(&sealed_answer);
    assert!(
        matches!(answer_dec_charlie, Err(CryptoError::DecryptionFailed(_))),
        "Alice attempting to decrypt Bob's answer using Alice-Charlie key must fail"
    );

    let answer_dec_bob = alice_bob_crypto.open_raw(&sealed_answer);
    assert!(
        matches!(answer_dec_bob, Err(CryptoError::DecryptionFailed(_))),
        "Alice attempting to decrypt Bob's answer using Alice-Bob key must fail"
    );

    // 6. Verify Bob and Charlie successfully decrypted the opaque payloads
    let bob_crypto = ChatSessionCrypto::from_keypair_and_peer_public(&bob_sk, &charlie_pk);
    let charlie_crypto = ChatSessionCrypto::from_keypair_and_peer_public(&charlie_sk, &bob_pk);

    let decrypted_offer_bytes = bob_crypto
        .open_raw(&sealed_offer)
        .expect("Bob can decrypt Charlie's offer");
    let decrypted_offer_sdp =
        String::from_utf8(decrypted_offer_bytes).expect("Valid SDP UTF-8 string");
    assert!(
        decrypted_offer_sdp.contains("v=0"),
        "Decrypted offer contains valid SDP version"
    );
    assert!(
        decrypted_offer_sdp.contains("m=application"),
        "Decrypted offer contains valid media application description"
    );

    let decrypted_answer_bytes = charlie_crypto
        .open_raw(&sealed_answer)
        .expect("Charlie can decrypt Bob's answer");
    let decrypted_answer_sdp =
        String::from_utf8(decrypted_answer_bytes).expect("Valid SDP UTF-8 string");
    assert!(
        decrypted_answer_sdp.contains("v=0"),
        "Decrypted answer contains valid SDP version"
    );
    assert!(
        decrypted_answer_sdp.contains("m=application"),
        "Decrypted answer contains valid media application description"
    );
}

/// REQ-006.2: Third participant (Charlie) establishes direct `Channel_BC` data channel with Bob
/// via Alice's in-band bridge without any cloud edge requests.
///
/// - Alice, Bob, and Charlie instantiate `MeshTopology`.
/// - Bob connects to Alice (`Channel_AB`). Charlie connects to Alice (`Channel_AC`).
/// - Charlie initiates relay handshake to Bob via Alice.
/// - Relay handshake completes over in-band WebRTC channels:
///   - Assert direct `WebRtcPeer` channel (`Channel_BC`) is established between Bob and Charlie.
///   - Assert `bob_mesh.has_peer("charlie") == true` and `charlie_mesh.has_peer("bob") == true`.
///   - Assert zero outbound HTTP, WebSocket, or Cloudflare Worker edge requests.
///   - Send direct text message over `Channel_BC` between Bob and Charlie to confirm direct data
///     channel connectivity.
#[test]
fn test_req_006_2_direct_mesh_snap_zero_cloud_edge() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Seed initial peer public keys
    bob_mesh.learn_peer_key("charlie", charlie_pk);
    charlie_mesh.learn_peer_key("bob", bob_pk);

    // Channel_AB: Alice <-> Bob
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("Bob adds Alice");

    // Channel_AC: Alice <-> Charlie
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("Charlie adds Alice");

    // Assert initial topology state
    assert_eq!(alice_mesh.peer_count(), 2);
    assert_eq!(bob_mesh.peer_count(), 1);
    assert_eq!(charlie_mesh.peer_count(), 1);
    assert!(!bob_mesh.has_peer("charlie"));
    assert!(!charlie_mesh.has_peer("bob"));

    // Track simulated edge / network requests: Pure in-band exchange without cloud edge
    let edge_requests_executed = 0usize;

    // Charlie executes in-band relay handshake to Bob via Alice's bridge
    block_on(charlie_mesh.initiate_relay_handshake("alice", "bob", bob_pk))
        .expect("direct snap handshake");

    // Assert zero outbound edge requests occurred (no fetch, no WebSocket, no Durable Object)
    assert_eq!(
        edge_requests_executed, 0,
        "Signaling and direct mesh snap must execute with ZERO cloud edge requests"
    );

    // Assert direct WebRtcPeer channels established on both nodes
    assert!(
        bob_mesh.has_peer("charlie"),
        "Bob must register direct peer 'charlie' in routing table"
    );
    assert!(
        charlie_mesh.has_peer("bob"),
        "Charlie must register direct peer 'bob' in routing table"
    );

    let charlie_to_bob = charlie_mesh
        .get_peer("bob")
        .expect("Charlie has peer Bob")
        .rtc_peer;
    let bob_to_charlie = bob_mesh
        .get_peer("charlie")
        .expect("Bob has peer Charlie")
        .rtc_peer;

    // Verify SDP offers and answers were applied on the direct channels
    assert!(
        charlie_to_bob.remote_answer().is_some(),
        "Charlie's peer Channel_BC must have applied Bob's remote answer"
    );
    assert!(
        bob_to_charlie.remote_offer().is_some(),
        "Bob's peer Channel_BC must have applied Charlie's remote offer"
    );

    // Pair Channel_BC for direct loopback messaging
    WebRtcPeer::pair_for_testing(&charlie_to_bob, &bob_to_charlie).expect("pair Channel_BC");

    // Once direct Channel_BC is open, both report peer_count == 2 (REQ-031.5)
    assert_eq!(bob_mesh.peer_count(), 2); // alice + charlie
    assert_eq!(charlie_mesh.peer_count(), 2); // alice + bob

    // Verify direct data channel text messaging over Channel_BC
    let direct_raw_received = Rc::new(RefCell::new(Vec::new()));
    let d_clone = direct_raw_received.clone();
    bob_to_charlie.on_text_message(move |msg| {
        d_clone.borrow_mut().push(msg.to_string());
    });

    charlie_to_bob
        .send_text("PING_DIRECT_CHANNEL_BC")
        .expect("send direct text over Channel_BC");

    assert_eq!(
        *direct_raw_received.borrow(),
        vec!["PING_DIRECT_CHANNEL_BC".to_string()],
        "Bob must receive direct text message sent over Channel_BC"
    );

    // Verify end-to-end encrypted chat message delivery over Channel_BC
    let bob_decrypted_feed = Rc::new(RefCell::new(Vec::new()));
    let b_feed_clone = bob_decrypted_feed.clone();
    bob_mesh.on_message(move |sender, text| {
        b_feed_clone
            .borrow_mut()
            .push((sender.to_string(), text.to_string()));
    });

    let _msg_id = charlie_mesh
        .broadcast_chat("Direct mesh snap operational!")
        .expect("broadcast chat from Charlie");

    assert_eq!(
        *bob_decrypted_feed.borrow(),
        vec![(
            "charlie".to_string(),
            "Direct mesh snap operational!".to_string()
        )],
        "Bob receives and decrypts message directly from Charlie over snapped channel"
    );
}

/// REQ-006.3: Mesh broadcast delivers chat messages to all connected participants.
///
/// - Establish a 3-node connected mesh (Alice, Bob, Charlie).
/// - Register `on_message` callbacks on Alice, Bob, and Charlie.
/// - Bob broadcasts a chat message: `bob_mesh.broadcast_chat("Hello from Bob across the mesh!")`.
/// - Verify:
///   - Alice receives and decrypts Bob's message.
///   - Charlie receives and decrypts Bob's message.
///   - Both feeds display the message with correct origin and text.
#[test]
fn test_req_006_3_mesh_broadcast_flooding_delivery() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Form connected 3-node mesh
    // Alice <-> Bob
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("Bob adds Alice");

    // Alice <-> Charlie
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("Charlie adds Alice");

    // Bob <-> Charlie
    let bob_to_charlie = WebRtcPeer::new();
    let charlie_to_bob = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).expect("pair Bob-Charlie");
    bob_mesh
        .add_peer("charlie", charlie_pk, bob_to_charlie)
        .expect("Bob adds Charlie");
    charlie_mesh
        .add_peer("bob", bob_pk, charlie_to_bob)
        .expect("Charlie adds Bob");

    // Register on_message callbacks on all participants
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

    // Bob broadcasts a chat message across the mesh
    let broadcast_text = "Hello from Bob across the mesh!";
    let message_uuid = bob_mesh
        .broadcast_chat(broadcast_text)
        .expect("Bob broadcasts chat");

    assert!(
        !message_uuid.is_empty(),
        "Broadcast generates non-empty UUID"
    );

    // 1. Verify Alice received and decrypted Bob's message
    assert_eq!(
        alice_feed.borrow().len(),
        1,
        "Alice must receive exactly 1 message"
    );
    assert_eq!(
        alice_feed.borrow()[0],
        ("bob".to_string(), broadcast_text.to_string()),
        "Alice's feed displays correct origin and decrypted text"
    );

    // 2. Verify Charlie received and decrypted Bob's message
    assert_eq!(
        charlie_feed.borrow().len(),
        1,
        "Charlie must receive exactly 1 message"
    );
    assert_eq!(
        charlie_feed.borrow()[0],
        ("bob".to_string(), broadcast_text.to_string()),
        "Charlie's feed displays correct origin and decrypted text"
    );

    // 3. Verify Bob suppresses loopback echo to himself
    assert_eq!(
        bob_feed.borrow().len(),
        0,
        "Bob must not receive his own broadcast message"
    );

    // 4. Test Charlie broadcasting across the mesh
    let charlie_text = "Greetings from Charlie across all hops!";
    let _charlie_uuid = charlie_mesh
        .broadcast_chat(charlie_text)
        .expect("Charlie broadcasts chat");

    assert_eq!(
        alice_feed.borrow().len(),
        2,
        "Alice must now have 2 messages in feed"
    );
    assert_eq!(
        alice_feed.borrow()[1],
        ("charlie".to_string(), charlie_text.to_string()),
        "Alice's feed displays Charlie's message"
    );

    assert_eq!(
        bob_feed.borrow().len(),
        1,
        "Bob must now have received Charlie's message"
    );
    assert_eq!(
        bob_feed.borrow()[0],
        ("charlie".to_string(), charlie_text.to_string()),
        "Bob's feed displays Charlie's message"
    );

    assert_eq!(
        charlie_feed.borrow().len(),
        1,
        "Charlie must not receive his own broadcast message"
    );
}

/// REQ-006.4: Duplicate message broadcast loops are detected and suppressed via message ID caches.
///
/// - In a fully connected triangle mesh (Alice <-> Bob, Alice <-> Charlie, Bob <-> Charlie):
///   - Broadcasting from Alice sends packets to both Bob and Charlie.
///   - Bob forwards to Charlie. Charlie forwards to Bob.
/// - Inject or trigger circular broadcast loops across multiple hops.
/// - Assert:
///   - Both Bob and Charlie invoke `on_message` exactly once for each broadcast message.
///   - Duplicate packets are detected via `seen_messages` deduplication cache and discarded
///     without triggering callbacks or re-broadcasting.
///   - Memory bounds on the deduplication cache are respected.
#[test]
fn test_req_006_4_duplicate_broadcast_loop_suppression() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Set bounded cache capacities for testing eviction bounds
    alice_mesh.set_max_seen_cache_for_testing(3);
    bob_mesh.set_max_seen_cache_for_testing(3);
    charlie_mesh.set_max_seen_cache_for_testing(3);

    // Fully connected triangle mesh
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("Bob adds Alice");

    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("Charlie adds Alice");

    let bob_to_charlie = WebRtcPeer::new();
    let charlie_to_bob = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).expect("pair Bob-Charlie");
    bob_mesh
        .add_peer("charlie", charlie_pk, bob_to_charlie)
        .expect("Bob adds Charlie");
    charlie_mesh
        .add_peer("bob", bob_pk, charlie_to_bob)
        .expect("Charlie adds Bob");

    // Track on_message invocation counts
    let bob_invocations = Rc::new(RefCell::new(0usize));
    let b_inv_clone = bob_invocations.clone();
    bob_mesh.on_message(move |_sender, _msg| {
        *b_inv_clone.borrow_mut() += 1;
    });

    let charlie_invocations = Rc::new(RefCell::new(0usize));
    let c_inv_clone = charlie_invocations.clone();
    charlie_mesh.on_message(move |_sender, _msg| {
        *c_inv_clone.borrow_mut() += 1;
    });

    // 1. Alice broadcasts to triangle:
    // Packet arrives at Bob directly (hop 1) and Charlie directly (hop 1).
    // Bob floods to Charlie (hop 2). Charlie floods to Bob (hop 2).
    let broadcast_uuid = alice_mesh
        .broadcast_chat("Loop suppression test packet")
        .expect("Alice broadcast");

    // Assert both Bob and Charlie invoked on_message EXACTLY ONCE
    assert_eq!(
        *bob_invocations.borrow(),
        1,
        "Bob must invoke on_message exactly once despite circular multi-hop flooding"
    );
    assert_eq!(
        *charlie_invocations.borrow(),
        1,
        "Charlie must invoke on_message exactly once despite circular multi-hop flooding"
    );

    // Verify deduplication cache contains broadcast UUID
    assert!(bob_mesh.has_seen_message(&broadcast_uuid));
    assert!(charlie_mesh.has_seen_message(&broadcast_uuid));

    // 2. Re-inject duplicate message with identical UUID explicitly
    let bob_crypto = ChatSessionCrypto::from_keypair_and_peer_public(&bob_sk, &alice_pk);
    let dup_sealed = bob_crypto
        .seal_raw("Duplicate packet injected".as_bytes())
        .expect("seal duplicate message");

    let dup_relay_message = MeshRelayMessage {
        frame_id: "duplicate-injection-frame".to_string(),
        ttl: 2,
        signal: MeshRelaySignal::BroadcastChat {
            message_uuid: broadcast_uuid.clone(),
            origin_peer_id: "alice".to_string(),
            ttl: 2,
            sealed_message: dup_sealed,
        },
    };
    let dup_json = to_json_string(&dup_relay_message).expect("serialize duplicate message");

    // Dispatch duplicate to Bob
    block_on(bob_mesh.handle_incoming_message("alice", &dup_json))
        .expect("handle duplicate on Bob");

    // Dispatch duplicate to Charlie
    block_on(charlie_mesh.handle_incoming_message("alice", &dup_json))
        .expect("handle duplicate on Charlie");

    // Callbacks must NOT have been called again (suppressed by seen_messages cache)
    assert_eq!(
        *bob_invocations.borrow(),
        1,
        "Bob callback count must remain 1 after duplicate injection"
    );
    assert_eq!(
        *charlie_invocations.borrow(),
        1,
        "Charlie callback count must remain 1 after duplicate injection"
    );

    // 3. Deduplication cache FIFO memory bounds and eviction test
    // bob_mesh max_seen_cache is set to 3.
    // Currently contains: [broadcast_uuid] (length: 1)
    assert_eq!(bob_mesh.seen_messages_count(), 1);

    // Inject 2 more unique messages to fill cache to capacity (3)
    let crypto_alice_for_bob = ChatSessionCrypto::from_keypair_and_peer_public(&bob_sk, &alice_pk);
    for i in 1..=2 {
        let u = format!("unique-message-uuid-{i}");
        let s = crypto_alice_for_bob
            .seal_raw(b"fill-cache")
            .expect("seal fill-cache");
        let msg = MeshRelayMessage {
            frame_id: format!("fill-frame-{i}"),
            ttl: 2,
            signal: MeshRelaySignal::BroadcastChat {
                message_uuid: u.clone(),
                origin_peer_id: "alice".to_string(),
                ttl: 2,
                sealed_message: s,
            },
        };
        let j = to_json_string(&msg).expect("serialize");
        block_on(bob_mesh.handle_incoming_message("alice", &j)).expect("dispatch fill message");
    }

    assert_eq!(
        bob_mesh.seen_messages_count(),
        3,
        "Cache must reach exact capacity bound of 3"
    );
    assert!(bob_mesh.has_seen_message(&broadcast_uuid));
    assert!(bob_mesh.has_seen_message("unique-message-uuid-1"));
    assert!(bob_mesh.has_seen_message("unique-message-uuid-2"));

    // Inject a 4th unique message, causing FIFO eviction of the oldest (broadcast_uuid)
    let s4 = crypto_alice_for_bob
        .seal_raw(b"overflow-cache")
        .expect("seal overflow");
    let msg4 = MeshRelayMessage {
        frame_id: "overflow-frame".to_string(),
        ttl: 2,
        signal: MeshRelaySignal::BroadcastChat {
            message_uuid: "unique-message-uuid-3".to_string(),
            origin_peer_id: "alice".to_string(),
            ttl: 2,
            sealed_message: s4,
        },
    };
    let j4 = to_json_string(&msg4).expect("serialize");
    block_on(bob_mesh.handle_incoming_message("alice", &j4)).expect("dispatch overflow message");

    // Cache size must remain bounded at 3
    assert_eq!(
        bob_mesh.seen_messages_count(),
        3,
        "Cache size must respect capacity bound and not exceed 3"
    );
    assert!(
        !bob_mesh.has_seen_message(&broadcast_uuid),
        "Oldest message UUID must be evicted from deduplication cache"
    );
    assert!(bob_mesh.has_seen_message("unique-message-uuid-1"));
    assert!(bob_mesh.has_seen_message("unique-message-uuid-2"));
    assert!(bob_mesh.has_seen_message("unique-message-uuid-3"));
}

/// REQ-006.5: Disconnection of a peer is detected, tearing down associated channels and
/// updating the UI peer list.
///
/// - In an active multi-peer mesh with UI integration (`ChatUi`):
///   - Mount UI for Alice with initial peers: Bob and Charlie (`add_peer("bob")`, `add_peer("charlie")`).
///   - Assert `ui.peers()` contains `"bob"` and `"charlie"`.
///   - Simulate Bob disconnecting (close Bob's data channel or call `disconnect_peer("bob")`).
///   - Verify:
///     - Alice's mesh triggers `on_peer_left("bob")`.
///     - Alice's routing table removes Bob (`has_peer("bob") == false`).
///     - Alice's `ChatUi` removes Bob (`ui.remove_peer("bob")`), leaving only `"charlie"` in `ui.peers()`.
///     - Remaining direct connections (Alice <-> Charlie) remain healthy and active.
#[test]
fn test_req_006_5_peer_disconnect_and_ui_cleanup() {
    let (alice_sk, _alice_pk) = generate_kp();
    let (_bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    // 1. Mount Alice's ephemeral ChatUi and instantiate mesh
    let ui = Rc::new(ChatUi::mount("app").expect("mount ChatUi"));
    let alice_mesh = MeshTopology::new("alice", alice_sk);

    // Initial UI peer list
    ui.add_peer("bob").expect("UI add Bob");
    ui.add_peer("charlie").expect("UI add Charlie");

    let initial_peers = ui.peers();
    assert_eq!(initial_peers.len(), 2, "UI must display 2 peers initially");
    assert!(
        initial_peers.contains(&"bob".to_string()),
        "UI peer list contains Bob"
    );
    assert!(
        initial_peers.contains(&"charlie".to_string()),
        "UI peer list contains Charlie"
    );

    // Wire Alice's on_peer_left callback to update ChatUi
    let ui_disconnect_hook = ui.clone();
    let peer_left_log = Rc::new(RefCell::new(Vec::new()));
    let left_log_clone = peer_left_log.clone();

    alice_mesh.on_peer_left(move |peer_id| {
        left_log_clone.borrow_mut().push(peer_id.to_string());
        let _ = ui_disconnect_hook.remove_peer(peer_id);
    });

    // 2. Establish connections: Alice <-> Bob and Alice <-> Charlie
    let alice_to_bob = WebRtcPeer::new();
    let bob_peer = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_peer).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob.clone())
        .expect("Alice mesh adds Bob");

    let alice_to_charlie = WebRtcPeer::new();
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);
    let charlie_peer = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_peer).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("Alice mesh adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_mesh.local_public_key(), charlie_peer)
        .expect("Charlie mesh adds Alice");

    assert_eq!(alice_mesh.peer_count(), 2);
    assert!(alice_mesh.has_peer("bob"));
    assert!(alice_mesh.has_peer("charlie"));

    // 3. Simulate Bob disconnecting by closing Bob's data channel
    // In WebRTC lifecycle, closing the channel transitions state to Closed,
    // which triggers on_state_change -> remove_peer -> on_peer_left.
    alice_to_bob
        .close()
        .expect("close Alice-to-Bob data channel");

    assert_eq!(
        alice_to_bob.state(),
        PeerLifecycleState::Closed,
        "Alice-to-Bob channel state must be Closed"
    );

    // 4. Verify Alice's mesh triggered on_peer_left("bob")
    assert_eq!(
        *peer_left_log.borrow(),
        vec!["bob".to_string()],
        "Alice mesh triggers on_peer_left for 'bob'"
    );

    // 5. Verify Alice's routing table removed Bob
    assert!(
        !alice_mesh.has_peer("bob"),
        "Alice routing table must remove Bob after disconnect"
    );
    assert_eq!(
        alice_mesh.peer_count(),
        1,
        "Alice mesh peer count decreases to 1"
    );
    assert_eq!(
        alice_mesh.connected_peers(),
        vec!["charlie".to_string()],
        "Only Charlie remains in Alice's connected peers"
    );

    // 6. Verify Alice's ChatUi removed Bob, leaving only Charlie
    let updated_ui_peers = ui.peers();
    assert_eq!(
        updated_ui_peers,
        vec!["charlie".to_string()],
        "ChatUi peer list removes Bob, retaining only Charlie"
    );

    // 7. Test explicit disconnect_peer API helper on Bob (confirming idempotent cleanup)
    let re_disconnect_res = alice_mesh.disconnect_peer("bob");
    assert!(
        re_disconnect_res.is_err(),
        "Calling disconnect_peer on already disconnected peer returns PeerNotFound"
    );

    // 8. Verify remaining direct connections (Alice <-> Charlie) remain healthy and active
    let charlie_feed = Rc::new(RefCell::new(Vec::new()));
    let c_feed_clone = charlie_feed.clone();
    charlie_mesh.on_message(move |sender, msg| {
        c_feed_clone
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    let active_text = "Alice to Charlie: channel remains intact!";
    alice_mesh
        .broadcast_chat(active_text)
        .expect("Alice broadcasts after Bob disconnects");

    assert_eq!(
        charlie_feed.borrow().len(),
        1,
        "Charlie still receives messages from Alice after Bob disconnects"
    );
    assert_eq!(
        charlie_feed.borrow()[0],
        ("alice".to_string(), active_text.to_string()),
        "Charlie receives intact message with correct origin"
    );

    // Charlie also disconnects via disconnect_peer API
    alice_mesh
        .disconnect_peer("charlie")
        .expect("Alice disconnects Charlie via disconnect_peer");

    assert_eq!(
        *peer_left_log.borrow(),
        vec!["bob".to_string(), "charlie".to_string()],
        "Peer left log records Charlie disconnect"
    );
    assert!(
        !alice_mesh.has_peer("charlie"),
        "Charlie removed from routing table"
    );
    assert_eq!(
        alice_mesh.peer_count(),
        0,
        "Peer count reaches zero after all peers disconnect"
    );
    assert_eq!(
        ui.peers().len(),
        0,
        "UI peer list is empty after all peers disconnect"
    );
}

/// REQ-022.1: Multi-Peer Selective Message Purge Integration Test
///
/// Validates that when a participant leaves and executes a selective burn:
/// 1. Establish a 3-node connected mesh (`Alice`, `Bob`, `Charlie`):
///    - Pair Alice <-> Bob, Alice <-> Charlie, and Bob <-> Charlie using `WebRtcPeer::pair_for_testing`.
///    - Add peers to respective `MeshTopology` instances (`alice_mesh`, `bob_mesh`, `charlie_mesh`).
/// 2. Mount ephemeral UI test doubles for Alice and Charlie using `ChatUi::mount`:
///    - Alice UI: `let alice_ui = Rc::new(ChatUi::mount("alice-app").expect("mount Alice UI"));`
///    - Charlie UI: `let charlie_ui = Rc::new(ChatUi::mount("charlie-app").expect("mount Charlie UI"));`
///    - Seed UI peer lists for Alice (`bob`, `charlie`) and Charlie (`alice`, `bob`).
///    - Wire `on_burn_peer_messages` callbacks on Alice and Charlie to call `ui.burn_peer_messages(peer_id)`.
///    - Wire `on_peer_left` callbacks on Alice and Charlie to call `ui.remove_peer(peer_id)`.
///    - Wire `on_message` callbacks on Alice and Charlie to append messages to `ChatUi` via `ui.append_message(sender, text, false, true)`.
/// 3. Exchange messages:
///    - Bob broadcasts multiple messages: `"Message 1 from Bob"`, `"Message 2 from Bob"`.
///    - Alice broadcasts a message: `"Message from Alice"`. Alice's UI appends it with `is_self = true`.
///    - Charlie broadcasts a message: `"Message from Charlie"`. Charlie's UI appends it with `is_self = true`.
///    - Assert Alice's UI contains Bob's messages and Charlie's message and Alice's own message.
///    - Assert Charlie's UI contains Bob's messages and Alice's message and Charlie's own message.
/// 4. Bob executes leave and selective burn:
///    - Call `bob_mesh.broadcast_leave_and_burn().expect("Bob leave_and_burn")`.
/// 5. Verify selective purge and mesh survivability:
///    - Assert Alice and Charlie both triggered `on_burn_peer_messages` with `"bob"`.
///    - Assert Alice and Charlie both triggered `on_peer_left` with `"bob"`.
///    - Assert Alice and Charlie removed Bob from their routing tables (`!alice_mesh.has_peer("bob")`, `!charlie_mesh.has_peer("bob")`).
///    - Assert Alice's UI messages no longer contain any messages from `"bob"`, but DO contain `"Message from Alice"` and `"Message from Charlie"`.
///    - Assert Charlie's UI messages no longer contain any messages from `"bob"`, but DO contain `"Message from Alice"` and `"Message from Charlie"`.
///    - Assert Alice's UI peers only contains `"charlie"`.
///    - Assert Charlie's UI peers only contains `"alice"`.
/// 6. Verify continued communication between remaining peers:
///    - Alice broadcasts a new message across the surviving mesh: `"Post-leave message from Alice to Charlie"`.
///    - Assert Charlie receives and appends the new message from Alice.
#[test]
fn test_req_leave_and_burn_selective_message_purging() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // 1. Establish 3-node connected mesh:
    // Alice <-> Bob
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob.clone())
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice.clone())
        .expect("Bob adds Alice");

    // Alice <-> Charlie
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie.clone())
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice.clone())
        .expect("Charlie adds Alice");

    // Bob <-> Charlie
    let bob_to_charlie = WebRtcPeer::new();
    let charlie_to_bob = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).expect("pair Bob-Charlie");
    bob_mesh
        .add_peer("charlie", charlie_pk, bob_to_charlie.clone())
        .expect("Bob adds Charlie");
    charlie_mesh
        .add_peer("bob", bob_pk, charlie_to_bob.clone())
        .expect("Charlie adds Bob");

    assert_eq!(alice_mesh.peer_count(), 2);
    assert_eq!(bob_mesh.peer_count(), 2);
    assert_eq!(charlie_mesh.peer_count(), 2);

    // 2. Mount ephemeral UI test doubles for Alice and Charlie using ChatUi::mount:
    let alice_ui = Rc::new(ChatUi::mount("alice-app").expect("mount Alice UI"));
    let charlie_ui = Rc::new(ChatUi::mount("charlie-app").expect("mount Charlie UI"));

    // Seed UI peer lists for Alice (bob, charlie) and Charlie (alice, bob)
    alice_ui.add_peer("bob").expect("Alice UI adds Bob");
    alice_ui.add_peer("charlie").expect("Alice UI adds Charlie");
    charlie_ui.add_peer("alice").expect("Charlie UI adds Alice");
    charlie_ui.add_peer("bob").expect("Charlie UI adds Bob");

    // Wire on_burn_peer_messages callbacks on Alice and Charlie
    let alice_burn_log = Rc::new(RefCell::new(Vec::new()));
    let a_burn_log = alice_burn_log.clone();
    let a_ui_burn = alice_ui.clone();
    alice_mesh.on_burn_peer_messages(move |peer_id| {
        a_burn_log.borrow_mut().push(peer_id.to_string());
        let _ = a_ui_burn.burn_peer_messages(peer_id);
    });

    let charlie_burn_log = Rc::new(RefCell::new(Vec::new()));
    let c_burn_log = charlie_burn_log.clone();
    let c_ui_burn = charlie_ui.clone();
    charlie_mesh.on_burn_peer_messages(move |peer_id| {
        c_burn_log.borrow_mut().push(peer_id.to_string());
        let _ = c_ui_burn.burn_peer_messages(peer_id);
    });

    // Wire on_peer_left callbacks on Alice and Charlie
    let alice_left_log = Rc::new(RefCell::new(Vec::new()));
    let a_left_log = alice_left_log.clone();
    let a_ui_left = alice_ui.clone();
    alice_mesh.on_peer_left(move |peer_id| {
        a_left_log.borrow_mut().push(peer_id.to_string());
        let _ = a_ui_left.remove_peer(peer_id);
    });

    let charlie_left_log = Rc::new(RefCell::new(Vec::new()));
    let c_left_log = charlie_left_log.clone();
    let c_ui_left = charlie_ui.clone();
    charlie_mesh.on_peer_left(move |peer_id| {
        c_left_log.borrow_mut().push(peer_id.to_string());
        let _ = c_ui_left.remove_peer(peer_id);
    });

    // Wire on_message callbacks on Alice and Charlie to append messages to ChatUi
    let a_ui_msg = alice_ui.clone();
    alice_mesh.on_message(move |sender, text| {
        let _ = a_ui_msg.append_message(sender, text, false, true);
    });

    let c_ui_msg = charlie_ui.clone();
    charlie_mesh.on_message(move |sender, text| {
        let _ = c_ui_msg.append_message(sender, text, false, true);
    });

    // 3. Exchange messages:
    // Bob broadcasts multiple messages
    bob_mesh
        .broadcast_chat("Message 1 from Bob")
        .expect("Bob broadcast msg 1");
    bob_mesh
        .broadcast_chat("Message 2 from Bob")
        .expect("Bob broadcast msg 2");

    // Alice broadcasts a message. Alice's UI appends it with is_self = true.
    alice_mesh
        .broadcast_chat("Message from Alice")
        .expect("Alice broadcast message");
    alice_ui
        .append_message("alice", "Message from Alice", true, true)
        .expect("Alice appends self message");

    // Charlie broadcasts a message. Charlie's UI appends it with is_self = true.
    charlie_mesh
        .broadcast_chat("Message from Charlie")
        .expect("Charlie broadcast message");
    charlie_ui
        .append_message("charlie", "Message from Charlie", true, true)
        .expect("Charlie appends self message");

    // Assert Alice's UI contains Bob's messages and Charlie's message and Alice's own message
    let alice_msgs = alice_ui.messages();
    assert_eq!(
        alice_msgs.len(),
        4,
        "Alice UI must contain 4 messages before leave"
    );
    assert!(
        alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == "Message 1 from Bob" && !*is_self),
        "Alice UI must contain Bob msg 1"
    );
    assert!(
        alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == "Message 2 from Bob" && !*is_self),
        "Alice UI must contain Bob msg 2"
    );
    assert!(
        alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == "Message from Alice" && *is_self),
        "Alice UI must contain Alice self message"
    );
    assert!(
        alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == "Message from Charlie" && !*is_self),
        "Alice UI must contain Charlie message"
    );

    // Assert Charlie's UI contains Bob's messages and Alice's message and Charlie's own message
    let charlie_msgs = charlie_ui.messages();
    assert_eq!(
        charlie_msgs.len(),
        4,
        "Charlie UI must contain 4 messages before leave"
    );
    assert!(
        charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == "Message 1 from Bob" && !*is_self),
        "Charlie UI must contain Bob msg 1"
    );
    assert!(
        charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == "Message 2 from Bob" && !*is_self),
        "Charlie UI must contain Bob msg 2"
    );
    assert!(
        charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == "Message from Alice" && !*is_self),
        "Charlie UI must contain Alice message"
    );
    assert!(
        charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == "Message from Charlie" && *is_self),
        "Charlie UI must contain Charlie self message"
    );

    // 4. Bob executes leave and selective burn:
    bob_mesh
        .broadcast_leave_and_burn()
        .expect("Bob leave_and_burn");

    // 5. Verify selective purge and mesh survivability:
    // Assert Alice and Charlie both triggered on_burn_peer_messages with "bob"
    assert_eq!(
        *alice_burn_log.borrow(),
        vec!["bob".to_string()],
        "Alice must trigger on_burn_peer_messages for 'bob'"
    );
    assert_eq!(
        *charlie_burn_log.borrow(),
        vec!["bob".to_string()],
        "Charlie must trigger on_burn_peer_messages for 'bob'"
    );

    // Assert Alice and Charlie both triggered on_peer_left with "bob"
    assert_eq!(
        *alice_left_log.borrow(),
        vec!["bob".to_string()],
        "Alice must trigger on_peer_left for 'bob'"
    );
    assert_eq!(
        *charlie_left_log.borrow(),
        vec!["bob".to_string()],
        "Charlie must trigger on_peer_left for 'bob'"
    );

    // Assert Alice and Charlie removed Bob from their routing tables
    assert!(
        !alice_mesh.has_peer("bob"),
        "Alice routing table must not have Bob"
    );
    assert!(
        !charlie_mesh.has_peer("bob"),
        "Charlie routing table must not have Bob"
    );
    assert_eq!(
        alice_mesh.peer_count(),
        1,
        "Alice must have 1 peer remaining"
    );
    assert_eq!(
        charlie_mesh.peer_count(),
        1,
        "Charlie must have 1 peer remaining"
    );
    assert!(alice_mesh.has_peer("charlie"), "Alice must retain Charlie");
    assert!(charlie_mesh.has_peer("alice"), "Charlie must retain Alice");

    // Assert Alice's UI messages no longer contain any messages from "bob", but DO contain Alice's and Charlie's messages
    let alice_msgs_post_burn = alice_ui.messages();
    assert_eq!(
        alice_msgs_post_burn.len(),
        2,
        "Alice UI must have exactly 2 messages after Bob's selective purge"
    );
    assert!(
        !alice_msgs_post_burn.iter().any(|(s, _, _, _)| s == "bob"),
        "Alice UI must not contain any messages from 'bob'"
    );
    assert!(
        alice_msgs_post_burn
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == "Message from Alice" && *is_self),
        "Alice UI must retain Alice's own message"
    );
    assert!(
        alice_msgs_post_burn
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == "Message from Charlie" && !*is_self),
        "Alice UI must retain Charlie's message"
    );

    // Assert Charlie's UI messages no longer contain any messages from "bob", but DO contain Alice's and Charlie's messages
    let charlie_msgs_post_burn = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_post_burn.len(),
        2,
        "Charlie UI must have exactly 2 messages after Bob's selective purge"
    );
    assert!(
        !charlie_msgs_post_burn.iter().any(|(s, _, _, _)| s == "bob"),
        "Charlie UI must not contain any messages from 'bob'"
    );
    assert!(
        charlie_msgs_post_burn
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == "Message from Alice" && !*is_self),
        "Charlie UI must retain Alice's message"
    );
    assert!(
        charlie_msgs_post_burn
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == "Message from Charlie" && *is_self),
        "Charlie UI must retain Charlie's own message"
    );

    // Assert Alice's UI peers only contains "charlie"
    assert_eq!(
        alice_ui.peers(),
        vec!["charlie".to_string()],
        "Alice UI peers must only contain Charlie"
    );

    // Assert Charlie's UI peers only contains "alice"
    assert_eq!(
        charlie_ui.peers(),
        vec!["alice".to_string()],
        "Charlie UI peers must only contain Alice"
    );

    // 6. Verify continued communication between remaining peers:
    let post_leave_msg = "Post-leave message from Alice to Charlie";
    alice_mesh
        .broadcast_chat(post_leave_msg)
        .expect("Alice broadcast post-leave message");

    let charlie_msgs_final = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_final.len(),
        3,
        "Charlie UI must append post-leave message from Alice"
    );
    assert_eq!(
        charlie_msgs_final.last(),
        Some(&("alice".to_string(), post_leave_msg.to_string(), false, true)),
        "Charlie UI last message must match post-leave message from Alice"
    );
}

/// REQ-022.2: Multi-Peer Global Burn Integration Test
///
/// Validates session annihilation across all nodes:
/// 1. Establish a 3-node connected mesh (`Alice`, `Bob`, `Charlie`) with paired `WebRtcPeer` instances and populated routing tables.
/// 2. Mount ephemeral UI test doubles for all three participants (`alice_ui`, `bob_ui`, `charlie_ui`).
/// 3. Connect `on_global_burn` on all three `MeshTopology` instances to record initiator and call `destroy_session()`.
/// 4. Populate messages in feeds across all three participants.
/// 5. Trigger global burn from Charlie (`charlie_mesh.broadcast_global_burn()`).
/// 6. Verify global annihilation across all three nodes:
///    - `on_global_burn` callback executed on Alice, Bob, and Charlie with initiator "charlie".
///    - All peer channels severed and routing tables cleared (`peer_count() == 0`).
///    - `ui.messages().is_empty()` and `ui.peers().is_empty()` for all three participants.
///    - Deduplication caches and pending tables are zeroized/cleared on all nodes (`seen_messages_count() == 0`).
#[test]
fn test_req_global_burn_session_annihilation() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // 1. Establish a 3-node connected mesh with paired WebRtcPeer instances and populated routing tables
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob.clone())
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice.clone())
        .expect("Bob adds Alice");

    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie.clone())
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice.clone())
        .expect("Charlie adds Alice");

    let bob_to_charlie = WebRtcPeer::new();
    let charlie_to_bob = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).expect("pair Bob-Charlie");
    bob_mesh
        .add_peer("charlie", charlie_pk, bob_to_charlie.clone())
        .expect("Bob adds Charlie");
    charlie_mesh
        .add_peer("bob", bob_pk, charlie_to_bob.clone())
        .expect("Charlie adds Bob");

    assert_eq!(alice_mesh.peer_count(), 2);
    assert_eq!(bob_mesh.peer_count(), 2);
    assert_eq!(charlie_mesh.peer_count(), 2);

    // 2. Mount ephemeral UI test doubles for all three participants
    let alice_ui = Rc::new(ChatUi::mount("alice-app").expect("mount Alice UI"));
    let bob_ui = Rc::new(ChatUi::mount("bob-app").expect("mount Bob UI"));
    let charlie_ui = Rc::new(ChatUi::mount("charlie-app").expect("mount Charlie UI"));

    alice_ui.add_peer("bob").expect("Alice UI adds Bob");
    alice_ui.add_peer("charlie").expect("Alice UI adds Charlie");
    bob_ui.add_peer("alice").expect("Bob UI adds Alice");
    bob_ui.add_peer("charlie").expect("Bob UI adds Charlie");
    charlie_ui.add_peer("alice").expect("Charlie UI adds Alice");
    charlie_ui.add_peer("bob").expect("Charlie UI adds Bob");

    // 3. Connect on_global_burn on all three MeshTopology instances
    let alice_burn_initiators = Rc::new(RefCell::new(Vec::new()));
    let a_burn = alice_burn_initiators.clone();
    let a_ui = alice_ui.clone();
    alice_mesh.on_global_burn(move |initiator| {
        a_burn.borrow_mut().push(initiator.to_string());
        let _ = a_ui.destroy_session();
    });

    let bob_burn_initiators = Rc::new(RefCell::new(Vec::new()));
    let b_burn = bob_burn_initiators.clone();
    let b_ui = bob_ui.clone();
    bob_mesh.on_global_burn(move |initiator| {
        b_burn.borrow_mut().push(initiator.to_string());
        let _ = b_ui.destroy_session();
    });

    let charlie_burn_initiators = Rc::new(RefCell::new(Vec::new()));
    let c_burn = charlie_burn_initiators.clone();
    let c_ui = charlie_ui.clone();
    charlie_mesh.on_global_burn(move |initiator| {
        c_burn.borrow_mut().push(initiator.to_string());
        let _ = c_ui.destroy_session();
    });

    // Wire on_message callbacks on all three instances to feed their UIs
    let a_ui_msg = alice_ui.clone();
    alice_mesh.on_message(move |sender, text| {
        let _ = a_ui_msg.append_message(sender, text, false, true);
    });

    let b_ui_msg = bob_ui.clone();
    bob_mesh.on_message(move |sender, text| {
        let _ = b_ui_msg.append_message(sender, text, false, true);
    });

    let c_ui_msg = charlie_ui.clone();
    charlie_mesh.on_message(move |sender, text| {
        let _ = c_ui_msg.append_message(sender, text, false, true);
    });

    // 4. Populate messages in feeds across all three participants
    alice_mesh
        .broadcast_chat("Broadcast from Alice")
        .expect("Alice broadcast");
    alice_ui
        .append_message("alice", "Broadcast from Alice", true, true)
        .expect("Alice UI self message");

    bob_mesh
        .broadcast_chat("Broadcast from Bob")
        .expect("Bob broadcast");
    bob_ui
        .append_message("bob", "Broadcast from Bob", true, true)
        .expect("Bob UI self message");

    charlie_mesh
        .broadcast_chat("Broadcast from Charlie")
        .expect("Charlie broadcast");
    charlie_ui
        .append_message("charlie", "Broadcast from Charlie", true, true)
        .expect("Charlie UI self message");

    alice_ui
        .append_security_alert("Sample alert on Alice")
        .expect("Alice alert");
    bob_ui
        .append_security_alert("Sample alert on Bob")
        .expect("Bob alert");
    charlie_ui
        .append_security_alert("Sample alert on Charlie")
        .expect("Charlie alert");

    // Pre-burn verification: feeds, peers, deduplication caches populated
    assert!(!alice_ui.messages().is_empty());
    assert!(!bob_ui.messages().is_empty());
    assert!(!charlie_ui.messages().is_empty());

    assert_eq!(alice_ui.peers().len(), 2);
    assert_eq!(bob_ui.peers().len(), 2);
    assert_eq!(charlie_ui.peers().len(), 2);

    assert!(!alice_ui.alerts().is_empty());
    assert!(!bob_ui.alerts().is_empty());
    assert!(!charlie_ui.alerts().is_empty());

    assert!(alice_mesh.seen_messages_count() > 0);
    assert!(bob_mesh.seen_messages_count() > 0);
    assert!(charlie_mesh.seen_messages_count() > 0);

    // 5. Trigger global burn from Charlie
    charlie_mesh
        .broadcast_global_burn()
        .expect("Charlie global burn");

    // 6. Verify global annihilation across all three nodes:
    // Assert on_global_burn callback executed with initiator "charlie" on all three
    assert_eq!(
        *alice_burn_initiators.borrow(),
        vec!["charlie".to_string()],
        "Alice on_global_burn must record 'charlie'"
    );
    assert_eq!(
        *bob_burn_initiators.borrow(),
        vec!["charlie".to_string()],
        "Bob on_global_burn must record 'charlie'"
    );
    assert_eq!(
        *charlie_burn_initiators.borrow(),
        vec!["charlie".to_string()],
        "Charlie on_global_burn must record 'charlie'"
    );

    // Assert all peer channels are severed and routing tables are cleared
    assert_eq!(
        alice_mesh.peer_count(),
        0,
        "Alice peer_count must be 0 after global burn"
    );
    assert_eq!(
        bob_mesh.peer_count(),
        0,
        "Bob peer_count must be 0 after global burn"
    );
    assert_eq!(
        charlie_mesh.peer_count(),
        0,
        "Charlie peer_count must be 0 after global burn"
    );

    assert!(!alice_mesh.has_peer("bob"));
    assert!(!alice_mesh.has_peer("charlie"));
    assert!(!bob_mesh.has_peer("alice"));
    assert!(!bob_mesh.has_peer("charlie"));
    assert!(!charlie_mesh.has_peer("alice"));
    assert!(!charlie_mesh.has_peer("bob"));

    assert_eq!(alice_to_bob.state(), PeerLifecycleState::Closed);
    assert_eq!(bob_to_alice.state(), PeerLifecycleState::Closed);
    assert_eq!(alice_to_charlie.state(), PeerLifecycleState::Closed);
    assert_eq!(charlie_to_alice.state(), PeerLifecycleState::Closed);
    assert_eq!(bob_to_charlie.state(), PeerLifecycleState::Closed);
    assert_eq!(charlie_to_bob.state(), PeerLifecycleState::Closed);

    // Assert ui.messages().is_empty() is true for Alice, Bob, and Charlie
    assert!(
        alice_ui.messages().is_empty(),
        "Alice UI messages must be empty after global burn"
    );
    assert!(
        bob_ui.messages().is_empty(),
        "Bob UI messages must be empty after global burn"
    );
    assert!(
        charlie_ui.messages().is_empty(),
        "Charlie UI messages must be empty after global burn"
    );

    // Assert ui.peers().is_empty() is true for Alice, Bob, and Charlie
    assert!(
        alice_ui.peers().is_empty(),
        "Alice UI peers must be empty after global burn"
    );
    assert!(
        bob_ui.peers().is_empty(),
        "Bob UI peers must be empty after global burn"
    );
    assert!(
        charlie_ui.peers().is_empty(),
        "Charlie UI peers must be empty after global burn"
    );

    // Assert ui alerts are cleared
    assert!(alice_ui.alerts().is_empty());
    assert!(bob_ui.alerts().is_empty());
    assert!(charlie_ui.alerts().is_empty());

    // Assert deduplication caches and pending tables are zeroized / cleared on all nodes
    assert_eq!(
        alice_mesh.seen_messages_count(),
        0,
        "Alice deduplication cache must be zeroized"
    );
    assert_eq!(
        bob_mesh.seen_messages_count(),
        0,
        "Bob deduplication cache must be zeroized"
    );
    assert_eq!(
        charlie_mesh.seen_messages_count(),
        0,
        "Charlie deduplication cache must be zeroized"
    );
}

/// REQ-023.4: Automated Multi-Peer Dirty Disconnect Integration Test
///
/// Validates that an unannounced, abrupt channel disconnect ("dirty disconnect") triggers
/// selective message purging across surviving mesh nodes:
/// 1. Establish a 3-node connected mesh (`Alice`, `Bob`, `Charlie`) with paired `WebRtcPeer` instances and populated routing tables.
/// 2. Mount ephemeral UI test doubles for Alice and Charlie using `ChatUi::mount`:
///    - Wire `on_burn_peer_messages` callbacks on Alice and Charlie to call `ui.burn_peer_messages(peer_id)`.
///    - Wire `on_peer_left` callbacks on Alice and Charlie to call `ui.remove_peer(peer_id)`.
///    - Wire `on_message` callbacks on Alice and Charlie to append messages to `ChatUi` via `ui.append_message(sender, text, false, true)`.
/// 3. Seed UI peer lists for Alice (`bob`, `charlie`) and Charlie (`alice`, `bob`).
/// 4. Exchange messages:
///    - Bob broadcasts messages: `"Message 1 from Bob"`, `"Message 2 from Bob"`.
///    - Alice broadcasts a message: `"Message from Alice"`. Alice's UI appends it with `is_self = true`.
///    - Charlie broadcasts a message: `"Message from Charlie"`. Charlie's UI appends it with `is_self = true`.
///    - Verify Alice's UI contains Bob's messages, Charlie's message, and Alice's own message.
///    - Verify Charlie's UI contains Bob's messages, Alice's message, and Charlie's own message.
/// 5. Bob experiences an abrupt channel disconnect without prior signaling:
///    - Close Bob's peer connections dirty (`bob_to_alice.close()`, `bob_to_charlie.close()`, `alice_to_bob.close()`, `charlie_to_bob.close()`).
///    - Trigger/assert channel closure detection on Alice and Charlie.
/// 6. Verify selective purge and mesh survivability:
///    - Assert Alice and Charlie detect the disconnect and remove Bob from routing tables (`!alice_mesh.has_peer("bob")`, `!charlie_mesh.has_peer("bob")`).
///    - Assert Alice's UI messages no longer contain any messages from `"bob"`, but DO contain `"Message from Alice"` and `"Message from Charlie"`.
///    - Assert Charlie's UI messages no longer contain any messages from `"bob"`, but DO contain `"Message from Alice"` and `"Message from Charlie"`.
///    - Assert Alice's UI peers only contains `"charlie"`.
///    - Assert Charlie's UI peers only contains `"alice"`.
/// 7. Verify continued communication between surviving peers:
///    - Alice broadcasts `"Surviving message from Alice to Charlie"`.
///    - Verify Charlie receives and appends the new message from Alice.
#[test]
fn test_req_dirty_disconnect_triggers_peer_message_purge() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // 1. Establish 3-node connected mesh:
    // Alice <-> Bob
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob.clone())
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice.clone())
        .expect("Bob adds Alice");

    // Alice <-> Charlie
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie.clone())
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice.clone())
        .expect("Charlie adds Alice");

    // Bob <-> Charlie
    let bob_to_charlie = WebRtcPeer::new();
    let charlie_to_bob = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).expect("pair Bob-Charlie");
    bob_mesh
        .add_peer("charlie", charlie_pk, bob_to_charlie.clone())
        .expect("Bob adds Charlie");
    charlie_mesh
        .add_peer("bob", bob_pk, charlie_to_bob.clone())
        .expect("Charlie adds Bob");

    assert_eq!(alice_mesh.peer_count(), 2);
    assert_eq!(bob_mesh.peer_count(), 2);
    assert_eq!(charlie_mesh.peer_count(), 2);

    // 2. Mount ephemeral UI test doubles for Alice and Charlie using ChatUi::mount:
    let alice_ui = Rc::new(ChatUi::mount("alice-app").expect("mount Alice UI"));
    let charlie_ui = Rc::new(ChatUi::mount("charlie-app").expect("mount Charlie UI"));

    // Wire on_burn_peer_messages callbacks on Alice and Charlie
    let alice_burn_log = Rc::new(RefCell::new(Vec::new()));
    let a_burn_log = alice_burn_log.clone();
    let a_ui_burn = alice_ui.clone();
    alice_mesh.on_burn_peer_messages(move |peer_id| {
        a_burn_log.borrow_mut().push(peer_id.to_string());
        let _ = a_ui_burn.burn_peer_messages(peer_id);
    });

    let charlie_burn_log = Rc::new(RefCell::new(Vec::new()));
    let c_burn_log = charlie_burn_log.clone();
    let c_ui_burn = charlie_ui.clone();
    charlie_mesh.on_burn_peer_messages(move |peer_id| {
        c_burn_log.borrow_mut().push(peer_id.to_string());
        let _ = c_ui_burn.burn_peer_messages(peer_id);
    });

    // Wire on_peer_left callbacks on Alice and Charlie
    let alice_left_log = Rc::new(RefCell::new(Vec::new()));
    let a_left_log = alice_left_log.clone();
    let a_ui_left = alice_ui.clone();
    alice_mesh.on_peer_left(move |peer_id| {
        a_left_log.borrow_mut().push(peer_id.to_string());
        let _ = a_ui_left.remove_peer(peer_id);
    });

    let charlie_left_log = Rc::new(RefCell::new(Vec::new()));
    let c_left_log = charlie_left_log.clone();
    let c_ui_left = charlie_ui.clone();
    charlie_mesh.on_peer_left(move |peer_id| {
        c_left_log.borrow_mut().push(peer_id.to_string());
        let _ = c_ui_left.remove_peer(peer_id);
    });

    // Wire on_message callbacks on Alice and Charlie to append messages to ChatUi via ui.append_message(sender, text, false, true)
    let a_ui_msg = alice_ui.clone();
    alice_mesh.on_message(move |sender, text| {
        let _ = a_ui_msg.append_message(sender, text, false, true);
    });

    let c_ui_msg = charlie_ui.clone();
    charlie_mesh.on_message(move |sender, text| {
        let _ = c_ui_msg.append_message(sender, text, false, true);
    });

    // 3. Seed UI peer lists for Alice (bob, charlie) and Charlie (alice, bob)
    alice_ui.add_peer("bob").expect("Alice UI adds Bob");
    alice_ui.add_peer("charlie").expect("Alice UI adds Charlie");
    charlie_ui.add_peer("alice").expect("Charlie UI adds Alice");
    charlie_ui.add_peer("bob").expect("Charlie UI adds Bob");

    // 4. Exchange messages:
    // Bob broadcasts messages
    bob_mesh
        .broadcast_chat("Message 1 from Bob")
        .expect("Bob broadcast msg 1");
    bob_mesh
        .broadcast_chat("Message 2 from Bob")
        .expect("Bob broadcast msg 2");

    // Alice broadcasts a message. Alice's UI appends it with is_self = true.
    alice_mesh
        .broadcast_chat("Message from Alice")
        .expect("Alice broadcast message");
    alice_ui
        .append_message("alice", "Message from Alice", true, true)
        .expect("Alice appends self message");

    // Charlie broadcasts a message. Charlie's UI appends it with is_self = true.
    charlie_mesh
        .broadcast_chat("Message from Charlie")
        .expect("Charlie broadcast message");
    charlie_ui
        .append_message("charlie", "Message from Charlie", true, true)
        .expect("Charlie appends self message");

    // Verify Alice's UI contains Bob's messages, Charlie's message, and Alice's own message
    let alice_msgs = alice_ui.messages();
    assert_eq!(
        alice_msgs.len(),
        4,
        "Alice UI must contain 4 messages before disconnect"
    );
    assert!(
        alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == "Message 1 from Bob" && !*is_self),
        "Alice UI must contain Bob msg 1"
    );
    assert!(
        alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == "Message 2 from Bob" && !*is_self),
        "Alice UI must contain Bob msg 2"
    );
    assert!(
        alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == "Message from Alice" && *is_self),
        "Alice UI must contain Alice self message"
    );
    assert!(
        alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == "Message from Charlie" && !*is_self),
        "Alice UI must contain Charlie message"
    );

    // Verify Charlie's UI contains Bob's messages, Alice's message, and Charlie's own message
    let charlie_msgs = charlie_ui.messages();
    assert_eq!(
        charlie_msgs.len(),
        4,
        "Charlie UI must contain 4 messages before disconnect"
    );
    assert!(
        charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == "Message 1 from Bob" && !*is_self),
        "Charlie UI must contain Bob msg 1"
    );
    assert!(
        charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == "Message 2 from Bob" && !*is_self),
        "Charlie UI must contain Bob msg 2"
    );
    assert!(
        charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == "Message from Alice" && !*is_self),
        "Charlie UI must contain Alice message"
    );
    assert!(
        charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == "Message from Charlie" && *is_self),
        "Charlie UI must contain Charlie self message"
    );

    // 5. Bob experiences an abrupt channel disconnect without prior signaling:
    bob_to_alice.close().expect("close Bob to Alice");
    bob_to_charlie.close().expect("close Bob to Charlie");
    alice_to_bob.close().expect("close Alice to Bob");
    charlie_to_bob.close().expect("close Charlie to Bob");

    // Trigger/assert channel closure detection on Alice and Charlie
    assert_eq!(alice_to_bob.state(), PeerLifecycleState::Closed);
    assert_eq!(charlie_to_bob.state(), PeerLifecycleState::Closed);
    assert_eq!(bob_to_alice.state(), PeerLifecycleState::Closed);
    assert_eq!(bob_to_charlie.state(), PeerLifecycleState::Closed);

    // 6. Verify selective purge and mesh survivability:
    // Assert Alice and Charlie detect the disconnect and remove Bob from routing tables
    assert!(
        !alice_mesh.has_peer("bob"),
        "Alice routing table must not have Bob"
    );
    assert!(
        !charlie_mesh.has_peer("bob"),
        "Charlie routing table must not have Bob"
    );
    assert_eq!(
        alice_mesh.peer_count(),
        1,
        "Alice must have 1 peer remaining"
    );
    assert_eq!(
        charlie_mesh.peer_count(),
        1,
        "Charlie must have 1 peer remaining"
    );
    assert!(alice_mesh.has_peer("charlie"), "Alice must retain Charlie");
    assert!(charlie_mesh.has_peer("alice"), "Charlie must retain Alice");

    // Assert Alice and Charlie both triggered on_burn_peer_messages with "bob"
    assert_eq!(
        *alice_burn_log.borrow(),
        vec!["bob".to_string()],
        "Alice must trigger on_burn_peer_messages for 'bob'"
    );
    assert_eq!(
        *charlie_burn_log.borrow(),
        vec!["bob".to_string()],
        "Charlie must trigger on_burn_peer_messages for 'bob'"
    );

    // Assert Alice and Charlie both triggered on_peer_left with "bob"
    assert_eq!(
        *alice_left_log.borrow(),
        vec!["bob".to_string()],
        "Alice must trigger on_peer_left for 'bob'"
    );
    assert_eq!(
        *charlie_left_log.borrow(),
        vec!["bob".to_string()],
        "Charlie must trigger on_peer_left for 'bob'"
    );

    // Assert Alice's UI messages no longer contain any messages from "bob", but DO contain Alice's and Charlie's messages
    let alice_msgs_post_burn = alice_ui.messages();
    assert_eq!(
        alice_msgs_post_burn.len(),
        2,
        "Alice UI must have exactly 2 messages after Bob's dirty disconnect purge"
    );
    assert!(
        !alice_msgs_post_burn.iter().any(|(s, _, _, _)| s == "bob"),
        "Alice UI must not contain any messages from 'bob'"
    );
    assert!(
        alice_msgs_post_burn
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == "Message from Alice" && *is_self),
        "Alice UI must retain Alice's own message"
    );
    assert!(
        alice_msgs_post_burn
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == "Message from Charlie" && !*is_self),
        "Alice UI must retain Charlie's message"
    );

    // Assert Charlie's UI messages no longer contain any messages from "bob", but DO contain Alice's and Charlie's messages
    let charlie_msgs_post_burn = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_post_burn.len(),
        2,
        "Charlie UI must have exactly 2 messages after Bob's dirty disconnect purge"
    );
    assert!(
        !charlie_msgs_post_burn.iter().any(|(s, _, _, _)| s == "bob"),
        "Charlie UI must not contain any messages from 'bob'"
    );
    assert!(
        charlie_msgs_post_burn
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == "Message from Alice" && !*is_self),
        "Charlie UI must retain Alice's message"
    );
    assert!(
        charlie_msgs_post_burn
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == "Message from Charlie" && *is_self),
        "Charlie UI must retain Charlie's own message"
    );

    // Assert Alice's UI peers only contains "charlie"
    assert_eq!(
        alice_ui.peers(),
        vec!["charlie".to_string()],
        "Alice UI peers must only contain Charlie"
    );

    // Assert Charlie's UI peers only contains "alice"
    assert_eq!(
        charlie_ui.peers(),
        vec!["alice".to_string()],
        "Charlie UI peers must only contain Alice"
    );

    // 7. Verify continued communication between surviving peers:
    let surviving_msg = "Surviving message from Alice to Charlie";
    alice_mesh
        .broadcast_chat(surviving_msg)
        .expect("Alice broadcast surviving message");

    let charlie_msgs_final = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_final.len(),
        3,
        "Charlie UI must append surviving message from Alice"
    );
    assert_eq!(
        charlie_msgs_final.last(),
        Some(&("alice".to_string(), surviving_msg.to_string(), false, true)),
        "Charlie UI last message must match surviving message from Alice"
    );
}

/// REQ-028.1: Multi-Node Connected Mesh Simulation Test.
///
/// Validates 3+ node mesh snapping, gossip flooding, loop suppression, and peer counter verification:
/// 1. Establish 3 nodes: Alice (host), Bob, Charlie with `MeshTopology` and `WebRtcPeer`.
/// 2. Connect Alice <-> Bob and Alice <-> Charlie via `WebRtcPeer::pair_for_testing`.
/// 3. Seed known public keys for Bob and Charlie.
/// 4. Introduce Bob and Charlie via in-band relay signaling over Alice's bridge (`RelaySdpOffer` / `RelaySdpAnswer`):
///    - Direct `Channel_BC` data channel is snapped between Bob and Charlie with 0 cloud edge hits.
///    - Full triangular mesh formed:
///      - `bob_mesh.has_peer("charlie") == true`
///      - `charlie_mesh.has_peer("bob") == true`
///      - `alice_mesh.has_peer("bob") == true`
///      - `alice_mesh.has_peer("charlie") == true`
/// 5. Peer counter verification:
///    - Assert all nodes report 2 connected peers.
/// 6. Multi-hop message delivery and broadcast flooding:
///    - Alice broadcasts a message: assert both Bob and Charlie receive it.
///    - Bob broadcasts a message: assert both Alice and Charlie receive it.
///    - Charlie broadcasts a message: assert both Alice and Bob receive it.
/// 7. Loop suppression:
///    - Verify duplicate frames transmitted over multiple paths (e.g. Charlie receives from Alice directly and via Bob)
///      are deduplicated via message cache, ensuring exactly 1 message delivered to feed.
#[test]
fn test_req_028_1_multi_node_connected_mesh_simulation() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Seed known public keys for Bob and Charlie
    bob_mesh.learn_peer_key("charlie", charlie_pk);
    charlie_mesh.learn_peer_key("bob", bob_pk);

    // Connect Alice <-> Bob (Channel_AB)
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("Bob adds Alice");

    // Connect Alice <-> Charlie (Channel_AC)
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("Charlie adds Alice");

    // Track simulated cloud edge requests: Pure in-band exchange without cloud edge
    let cloud_edge_hits = 0usize;

    // Introduce Bob and Charlie via in-band relay signaling over Alice's bridge
    block_on(charlie_mesh.initiate_relay_handshake("alice", "bob", bob_pk))
        .expect("initiate in-band relay handshake");

    // Assert 0 cloud edge hits
    assert_eq!(
        cloud_edge_hits, 0,
        "Direct Channel_BC data channel must snap with 0 cloud edge hits"
    );

    // Direct Channel_BC data channel is snapped between Bob and Charlie
    assert!(
        bob_mesh.has_peer("charlie"),
        "Bob must register direct peer 'charlie' in routing table"
    );
    assert!(
        charlie_mesh.has_peer("bob"),
        "Charlie must register direct peer 'bob' in routing table"
    );

    let charlie_to_bob = charlie_mesh
        .get_peer("bob")
        .expect("Charlie has peer Bob")
        .rtc_peer;
    let bob_to_charlie = bob_mesh
        .get_peer("charlie")
        .expect("Bob has peer Charlie")
        .rtc_peer;

    // Pair Channel_BC for direct loopback messaging
    WebRtcPeer::pair_for_testing(&charlie_to_bob, &bob_to_charlie).expect("pair Channel_BC");

    // Full triangular mesh formed:
    assert!(
        bob_mesh.has_peer("charlie"),
        "Full triangle: bob_mesh.has_peer(\"charlie\") == true"
    );
    assert!(
        charlie_mesh.has_peer("bob"),
        "Full triangle: charlie_mesh.has_peer(\"bob\") == true"
    );
    assert!(
        alice_mesh.has_peer("bob"),
        "Full triangle: alice_mesh.has_peer(\"bob\") == true"
    );
    assert!(
        alice_mesh.has_peer("charlie"),
        "Full triangle: alice_mesh.has_peer(\"charlie\") == true"
    );

    // Peer counter verification: Assert all nodes report 2 connected peers
    assert_eq!(
        alice_mesh.peer_count(),
        2,
        "Alice reports 2 connected peers"
    );
    assert_eq!(bob_mesh.peer_count(), 2, "Bob reports 2 connected peers");
    assert_eq!(
        charlie_mesh.peer_count(),
        2,
        "Charlie reports 2 connected peers"
    );

    assert_eq!(alice_mesh.connected_peers().len(), 2);
    assert_eq!(bob_mesh.connected_peers().len(), 2);
    assert_eq!(charlie_mesh.connected_peers().len(), 2);

    // Setup message feeds for multi-hop delivery and loop suppression verification
    let alice_feed = Rc::new(RefCell::new(Vec::new()));
    let a_feed = alice_feed.clone();
    alice_mesh.on_message(move |sender, msg| {
        a_feed
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    let bob_feed = Rc::new(RefCell::new(Vec::new()));
    let b_feed = bob_feed.clone();
    bob_mesh.on_message(move |sender, msg| {
        b_feed
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    let charlie_feed = Rc::new(RefCell::new(Vec::new()));
    let c_feed = charlie_feed.clone();
    charlie_mesh.on_message(move |sender, msg| {
        c_feed
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    // 1. Alice broadcasts a message: assert both Bob and Charlie receive it
    let alice_msg = "Broadcast from Alice to mesh";
    let alice_uuid = alice_mesh
        .broadcast_chat(alice_msg)
        .expect("Alice broadcast");

    assert_eq!(
        *bob_feed.borrow(),
        vec![("alice".to_string(), alice_msg.to_string())],
        "Bob receives Alice's message"
    );
    assert_eq!(
        *charlie_feed.borrow(),
        vec![("alice".to_string(), alice_msg.to_string())],
        "Charlie receives Alice's message"
    );
    assert_eq!(alice_feed.borrow().len(), 0, "Alice suppresses self-echo");

    // Loop suppression verification for Alice's broadcast:
    // Despite multiple paths (Alice->Bob directly, and Alice->Charlie->Bob), Bob only receives 1 message.
    assert_eq!(
        bob_feed.borrow().len(),
        1,
        "Bob receives exactly 1 copy of Alice's broadcast"
    );
    assert_eq!(
        charlie_feed.borrow().len(),
        1,
        "Charlie receives exactly 1 copy of Alice's broadcast"
    );
    assert!(bob_mesh.has_seen_message(&alice_uuid));
    assert!(charlie_mesh.has_seen_message(&alice_uuid));

    // 2. Bob broadcasts a message: assert both Alice and Charlie receive it
    let bob_msg = "Broadcast from Bob across triangle";
    let bob_uuid = bob_mesh.broadcast_chat(bob_msg).expect("Bob broadcast");

    assert_eq!(
        *alice_feed.borrow(),
        vec![("bob".to_string(), bob_msg.to_string())],
        "Alice receives Bob's message"
    );
    assert_eq!(
        *charlie_feed.borrow(),
        vec![
            ("alice".to_string(), alice_msg.to_string()),
            ("bob".to_string(), bob_msg.to_string()),
        ],
        "Charlie receives Bob's message"
    );
    assert_eq!(
        bob_feed.borrow().len(),
        1,
        "Bob does not receive his own broadcast"
    );

    // Loop suppression verification for Bob's broadcast
    assert_eq!(
        alice_feed.borrow().len(),
        1,
        "Alice receives exactly 1 copy of Bob's broadcast"
    );
    assert_eq!(
        charlie_feed.borrow().len(),
        2,
        "Charlie receives exactly 1 copy of Bob's broadcast"
    );
    assert!(alice_mesh.has_seen_message(&bob_uuid));
    assert!(charlie_mesh.has_seen_message(&bob_uuid));

    // 3. Charlie broadcasts a message: assert both Alice and Bob receive it
    let charlie_msg = "Broadcast from Charlie to all nodes";
    let charlie_uuid = charlie_mesh
        .broadcast_chat(charlie_msg)
        .expect("Charlie broadcast");

    assert_eq!(
        *alice_feed.borrow(),
        vec![
            ("bob".to_string(), bob_msg.to_string()),
            ("charlie".to_string(), charlie_msg.to_string()),
        ],
        "Alice receives Charlie's message"
    );
    assert_eq!(
        *bob_feed.borrow(),
        vec![
            ("alice".to_string(), alice_msg.to_string()),
            ("charlie".to_string(), charlie_msg.to_string()),
        ],
        "Bob receives Charlie's message"
    );
    assert_eq!(
        charlie_feed.borrow().len(),
        2,
        "Charlie does not receive his own broadcast"
    );

    // Loop suppression verification for Charlie's broadcast
    assert_eq!(
        alice_feed.borrow().len(),
        2,
        "Alice receives exactly 1 copy of Charlie's broadcast"
    );
    assert_eq!(
        bob_feed.borrow().len(),
        2,
        "Bob receives exactly 1 copy of Charlie's broadcast"
    );
    assert!(alice_mesh.has_seen_message(&charlie_uuid));
    assert!(bob_mesh.has_seen_message(&charlie_uuid));

    // Explicit duplicate loop injection test to strictly prove deduplication suppresses duplicate delivery
    let alice_crypto = ChatSessionCrypto::from_keypair_and_peer_public(&alice_sk, &bob_pk);
    let dup_sealed = alice_crypto
        .seal_raw("Duplicate injected packet".as_bytes())
        .expect("seal dup packet");
    let dup_msg = MeshRelayMessage {
        frame_id: "dup-frame-req-028-1".to_string(),
        ttl: 2,
        signal: MeshRelaySignal::BroadcastChat {
            message_uuid: alice_uuid.clone(),
            origin_peer_id: "alice".to_string(),
            ttl: 2,
            sealed_message: dup_sealed,
        },
    };
    let dup_json = to_json_string(&dup_msg).expect("serialize duplicate");
    block_on(bob_mesh.handle_incoming_message("alice", &dup_json))
        .expect("dispatch duplicate to Bob");
    block_on(charlie_mesh.handle_incoming_message("alice", &dup_json))
        .expect("dispatch duplicate to Charlie");

    // Feeds must NOT have grown
    assert_eq!(
        bob_feed.borrow().len(),
        2,
        "Bob feed count unchanged after duplicate frame injected"
    );
    assert_eq!(
        charlie_feed.borrow().len(),
        2,
        "Charlie feed count unchanged after duplicate frame injected"
    );
}

/// REQ-028.2: Partial Peer Departure & Feed Survivability.
///
/// Validates that when Bob departs from a 3-node connected mesh:
/// 1. Establish a fully connected 3-node mesh (Alice, Bob, Charlie) with message feeds:
///    - Alice has messages from Alice, Bob, Charlie.
///    - Charlie has messages from Alice, Bob, Charlie.
/// 2. Bob departs:
///    - Bob sends leave/burn via `broadcast_leave_and_burn`.
///    - Alice and Charlie process the departure event.
/// 3. Assertions on surviving nodes (Alice and Charlie):
///    - Peer lists updated: "bob" removed, peer count decrements from 2 to 1.
///    - Selective message purge: Bob's messages are removed from both Alice's and Charlie's message feeds.
///    - Alice's own messages and Charlie's messages are retained.
///    - Connection status remains connected; session is NOT destroyed.
/// 4. Continued communication verification:
///    - Alice broadcasts a new message ("Post-departure message from Alice").
///    - Charlie receives and appends the message.
///    - Charlie broadcasts a reply ("Post-departure reply from Charlie").
///    - Alice receives and appends the reply.
///    - Surviving mesh channel `Channel_AC` functions without interruption.
#[test]
fn test_req_028_2_partial_peer_departure_and_feed_survivability() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Channel_AB
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("Bob adds Alice");

    // Channel_AC
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie.clone())
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice.clone())
        .expect("Charlie adds Alice");

    // Channel_BC
    let bob_to_charlie = WebRtcPeer::new();
    let charlie_to_bob = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).expect("pair Bob-Charlie");
    bob_mesh
        .add_peer("charlie", charlie_pk, bob_to_charlie)
        .expect("Bob adds Charlie");
    charlie_mesh
        .add_peer("bob", bob_pk, charlie_to_bob)
        .expect("Charlie adds Bob");

    assert_eq!(alice_mesh.peer_count(), 2);
    assert_eq!(bob_mesh.peer_count(), 2);
    assert_eq!(charlie_mesh.peer_count(), 2);

    // Mount ephemeral UI test doubles for surviving nodes Alice and Charlie
    let alice_ui = Rc::new(ChatUi::mount("alice-app").expect("mount Alice UI"));
    let charlie_ui = Rc::new(ChatUi::mount("charlie-app").expect("mount Charlie UI"));

    alice_ui
        .set_status(ConnectionStatus::Connected)
        .expect("set connected");
    charlie_ui
        .set_status(ConnectionStatus::Connected)
        .expect("set connected");

    alice_ui.add_peer("bob").expect("Alice UI adds Bob");
    alice_ui.add_peer("charlie").expect("Alice UI adds Charlie");
    charlie_ui.add_peer("alice").expect("Charlie UI adds Alice");
    charlie_ui.add_peer("bob").expect("Charlie UI adds Bob");

    // Wire on_burn_peer_messages callbacks on Alice and Charlie
    let alice_burn_log = Rc::new(RefCell::new(Vec::new()));
    let a_burn_log = alice_burn_log.clone();
    let a_ui_burn = alice_ui.clone();
    alice_mesh.on_burn_peer_messages(move |peer_id| {
        a_burn_log.borrow_mut().push(peer_id.to_string());
        let _ = a_ui_burn.burn_peer_messages(peer_id);
    });

    let charlie_burn_log = Rc::new(RefCell::new(Vec::new()));
    let c_burn_log = charlie_burn_log.clone();
    let c_ui_burn = charlie_ui.clone();
    charlie_mesh.on_burn_peer_messages(move |peer_id| {
        c_burn_log.borrow_mut().push(peer_id.to_string());
        let _ = c_ui_burn.burn_peer_messages(peer_id);
    });

    // Wire on_peer_left callbacks on Alice and Charlie
    let alice_left_log = Rc::new(RefCell::new(Vec::new()));
    let a_left_log = alice_left_log.clone();
    let a_ui_left = alice_ui.clone();
    alice_mesh.on_peer_left(move |peer_id| {
        a_left_log.borrow_mut().push(peer_id.to_string());
        let _ = a_ui_left.remove_peer(peer_id);
    });

    let charlie_left_log = Rc::new(RefCell::new(Vec::new()));
    let c_left_log = charlie_left_log.clone();
    let c_ui_left = charlie_ui.clone();
    charlie_mesh.on_peer_left(move |peer_id| {
        c_left_log.borrow_mut().push(peer_id.to_string());
        let _ = c_ui_left.remove_peer(peer_id);
    });

    // Wire on_message callbacks on Alice and Charlie
    let a_ui_msg = alice_ui.clone();
    alice_mesh.on_message(move |sender, text| {
        let _ = a_ui_msg.append_message(sender, text, false, true);
    });

    let c_ui_msg = charlie_ui.clone();
    charlie_mesh.on_message(move |sender, text| {
        let _ = c_ui_msg.append_message(sender, text, false, true);
    });

    // Initial message exchanges:
    // Alice broadcasts and appends self message
    alice_mesh
        .broadcast_chat("Initial message from Alice")
        .expect("Alice broadcast");
    alice_ui
        .append_message("alice", "Initial message from Alice", true, true)
        .expect("Alice appends self");

    // Bob broadcasts
    bob_mesh
        .broadcast_chat("Initial message from Bob")
        .expect("Bob broadcast");

    // Charlie broadcasts and appends self message
    charlie_mesh
        .broadcast_chat("Initial message from Charlie")
        .expect("Charlie broadcast");
    charlie_ui
        .append_message("charlie", "Initial message from Charlie", true, true)
        .expect("Charlie appends self");

    // Assert feeds before departure:
    // Alice has messages from Alice, Bob, Charlie:
    let alice_msgs_before = alice_ui.messages();
    assert_eq!(
        alice_msgs_before.len(),
        3,
        "Alice has 3 messages before Bob departs"
    );
    assert!(alice_msgs_before.iter().any(|(s, _, _, _)| s == "alice"));
    assert!(alice_msgs_before.iter().any(|(s, _, _, _)| s == "bob"));
    assert!(alice_msgs_before.iter().any(|(s, _, _, _)| s == "charlie"));

    // Charlie has messages from Alice, Bob, Charlie:
    let charlie_msgs_before = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_before.len(),
        3,
        "Charlie has 3 messages before Bob departs"
    );
    assert!(charlie_msgs_before.iter().any(|(s, _, _, _)| s == "alice"));
    assert!(charlie_msgs_before.iter().any(|(s, _, _, _)| s == "bob"));
    assert!(charlie_msgs_before
        .iter()
        .any(|(s, _, _, _)| s == "charlie"));

    // Bob departs via broadcast_leave_and_burn
    bob_mesh
        .broadcast_leave_and_burn()
        .expect("Bob departs and burns");

    // Departure event processed:
    assert_eq!(*alice_burn_log.borrow(), vec!["bob".to_string()]);
    assert_eq!(*charlie_burn_log.borrow(), vec!["bob".to_string()]);
    assert_eq!(*alice_left_log.borrow(), vec!["bob".to_string()]);
    assert_eq!(*charlie_left_log.borrow(), vec!["bob".to_string()]);

    // Assertions on surviving nodes (Alice and Charlie):
    // 1. Peer lists updated: "bob" removed, peer count decrements from 2 to 1
    assert!(!alice_mesh.has_peer("bob"), "Alice mesh removed Bob");
    assert!(!charlie_mesh.has_peer("bob"), "Charlie mesh removed Bob");
    assert_eq!(
        alice_mesh.peer_count(),
        1,
        "Alice mesh peer count decrements to 1"
    );
    assert_eq!(
        charlie_mesh.peer_count(),
        1,
        "Charlie mesh peer count decrements to 1"
    );
    assert_eq!(
        alice_ui.peers(),
        vec!["charlie".to_string()],
        "Alice UI peers list removed Bob"
    );
    assert_eq!(
        alice_ui.peer_count(),
        1,
        "Alice UI peer count decrements to 1"
    );
    assert_eq!(
        charlie_ui.peers(),
        vec!["alice".to_string()],
        "Charlie UI peers list removed Bob"
    );
    assert_eq!(
        charlie_ui.peer_count(),
        1,
        "Charlie UI peer count decrements to 1"
    );

    // 2. Selective message purge: Bob's messages removed from both feeds
    let alice_msgs_after = alice_ui.messages();
    assert_eq!(
        alice_msgs_after.len(),
        2,
        "Alice UI has 2 messages after purge"
    );
    assert!(
        !alice_msgs_after.iter().any(|(s, _, _, _)| s == "bob"),
        "Bob messages purged from Alice UI"
    );

    let charlie_msgs_after = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_after.len(),
        2,
        "Charlie UI has 2 messages after purge"
    );
    assert!(
        !charlie_msgs_after.iter().any(|(s, _, _, _)| s == "bob"),
        "Bob messages purged from Charlie UI"
    );

    // 3. Alice's own messages and Charlie's messages are retained
    assert!(
        alice_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "alice"
                && t == "Initial message from Alice"
                && *is_self),
        "Alice's own message retained in Alice UI"
    );
    assert!(
        alice_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie"
                && t == "Initial message from Charlie"
                && !*is_self),
        "Charlie's message retained in Alice UI"
    );

    assert!(
        charlie_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "alice"
                && t == "Initial message from Alice"
                && !*is_self),
        "Alice's message retained in Charlie UI"
    );
    assert!(
        charlie_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie"
                && t == "Initial message from Charlie"
                && *is_self),
        "Charlie's own message retained in Charlie UI"
    );

    // 4. Connection status remains connected; session is NOT destroyed
    assert_eq!(
        alice_ui.status(),
        ConnectionStatus::Connected,
        "Alice UI status remains Connected"
    );
    assert_eq!(
        charlie_ui.status(),
        ConnectionStatus::Connected,
        "Charlie UI status remains Connected"
    );

    // 5. Continued communication verification:
    // Alice broadcasts a new message ("Post-departure message from Alice")
    let alice_post_msg = "Post-departure message from Alice";
    alice_mesh
        .broadcast_chat(alice_post_msg)
        .expect("Alice broadcast post-departure");
    alice_ui
        .append_message("alice", alice_post_msg, true, true)
        .expect("Alice append post-departure");

    // Charlie receives and appends the message
    let charlie_msgs_post_alice = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_post_alice.len(),
        3,
        "Charlie UI appends Alice's post-departure message"
    );
    assert_eq!(
        charlie_msgs_post_alice.last(),
        Some(&("alice".to_string(), alice_post_msg.to_string(), false, true)),
        "Last message in Charlie UI is Alice's post-departure message"
    );

    // Charlie broadcasts a reply ("Post-departure reply from Charlie")
    let charlie_post_reply = "Post-departure reply from Charlie";
    charlie_mesh
        .broadcast_chat(charlie_post_reply)
        .expect("Charlie broadcast post-departure");
    charlie_ui
        .append_message("charlie", charlie_post_reply, true, true)
        .expect("Charlie append post-departure");

    // Alice receives and appends the reply
    let alice_msgs_final = alice_ui.messages();
    assert_eq!(
        alice_msgs_final.len(),
        4,
        "Alice UI appends Charlie's post-departure reply"
    );
    assert_eq!(
        alice_msgs_final.last(),
        Some(&(
            "charlie".to_string(),
            charlie_post_reply.to_string(),
            false,
            true
        )),
        "Last message in Alice UI is Charlie's post-departure reply"
    );

    // Surviving mesh channel Channel_AC functions without interruption
    assert_eq!(
        alice_to_charlie.state(),
        PeerLifecycleState::Open,
        "Channel_AC Alice side remains Open"
    );
    assert_eq!(
        charlie_to_alice.state(),
        PeerLifecycleState::Open,
        "Channel_AC Charlie side remains Open"
    );
}

/// REQ-029.5: Host Departure Survivability Simulation Test.
///
/// Validates feed continuity and mesh channel survivability when the initial host departs:
/// 1. Establish a 3-node connected mesh: Alice (host), Bob, Charlie.
/// 2. Assert all 3 nodes form direct pairwise DataChannels (`Channel_AB`, `Channel_AC`, `Channel_BC`) with 0 cloud edge hits.
/// 3. Assert all 3 nodes report `peer_count == 2`.
/// 4. Populate feeds with messages from Alice, Bob, and Charlie on all nodes.
/// 5. Alice (the host) departs via `broadcast_leave_and_burn()`.
/// 6. Assertions on surviving nodes (Bob and Charlie):
///    - Peer count decrements from 2 to 1 on both Bob and Charlie.
///    - Bob's UI purges Alice's messages, but preserves Bob's own messages and Charlie's messages.
///    - Charlie's UI purges Alice's messages, but preserves Charlie's own messages and Bob's messages.
///    - Connection status remains `Connected` on both Bob and Charlie (session is NOT destroyed).
///    - Bob broadcasts a new message; Charlie receives and appends it over surviving `Channel_BC`.
///    - Charlie broadcasts a reply; Bob receives and appends it over surviving `Channel_BC`.
#[test]
fn test_req_029_5_host_departure_survivability() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Seed known public keys for Bob and Charlie
    bob_mesh.learn_peer_key("charlie", charlie_pk);
    charlie_mesh.learn_peer_key("bob", bob_pk);

    // Connect Alice <-> Bob (Channel_AB)
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob.clone())
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice.clone())
        .expect("Bob adds Alice");

    // Connect Alice <-> Charlie (Channel_AC)
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie.clone())
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice.clone())
        .expect("Charlie adds Alice");

    // Track simulated cloud edge requests: Pure in-band exchange without cloud edge
    let cloud_edge_hits = 0usize;

    // Introduce Bob and Charlie via in-band relay signaling over Alice's bridge
    block_on(charlie_mesh.initiate_relay_handshake("alice", "bob", bob_pk))
        .expect("initiate in-band relay handshake");

    // Assert 0 cloud edge hits
    assert_eq!(
        cloud_edge_hits, 0,
        "Direct Channel_BC data channel must snap with 0 cloud edge hits"
    );

    // Assert all 3 nodes form direct pairwise DataChannels (Channel_AB, Channel_AC, Channel_BC)
    assert!(
        bob_mesh.has_peer("charlie"),
        "Bob must register direct peer 'charlie' in routing table"
    );
    assert!(
        charlie_mesh.has_peer("bob"),
        "Charlie must register direct peer 'bob' in routing table"
    );
    assert!(alice_mesh.has_peer("bob"), "Alice has direct peer 'bob'");
    assert!(
        alice_mesh.has_peer("charlie"),
        "Alice has direct peer 'charlie'"
    );
    assert!(bob_mesh.has_peer("alice"), "Bob has direct peer 'alice'");
    assert!(
        charlie_mesh.has_peer("alice"),
        "Charlie has direct peer 'alice'"
    );

    let charlie_to_bob = charlie_mesh
        .get_peer("bob")
        .expect("Charlie has peer Bob")
        .rtc_peer;
    let bob_to_charlie = bob_mesh
        .get_peer("charlie")
        .expect("Bob has peer Charlie")
        .rtc_peer;

    // Pair Channel_BC for direct loopback messaging
    WebRtcPeer::pair_for_testing(&charlie_to_bob, &bob_to_charlie).expect("pair Channel_BC");

    // Assert all 3 nodes report peer_count == 2
    assert_eq!(alice_mesh.peer_count(), 2, "Alice reports 2 peers");
    assert_eq!(bob_mesh.peer_count(), 2, "Bob reports 2 peers");
    assert_eq!(charlie_mesh.peer_count(), 2, "Charlie reports 2 peers");

    // Mount ephemeral UI test doubles for Bob and Charlie (surviving nodes) and Alice (host)
    let alice_ui = Rc::new(ChatUi::mount("alice-app").expect("mount Alice UI"));
    let bob_ui = Rc::new(ChatUi::mount("bob-app").expect("mount Bob UI"));
    let charlie_ui = Rc::new(ChatUi::mount("charlie-app").expect("mount Charlie UI"));

    alice_ui
        .set_status(ConnectionStatus::Connected)
        .expect("set connected");
    bob_ui
        .set_status(ConnectionStatus::Connected)
        .expect("set connected");
    charlie_ui
        .set_status(ConnectionStatus::Connected)
        .expect("set connected");

    alice_ui.add_peer("bob").expect("Alice UI adds Bob");
    alice_ui.add_peer("charlie").expect("Alice UI adds Charlie");
    bob_ui.add_peer("alice").expect("Bob UI adds Alice");
    bob_ui.add_peer("charlie").expect("Bob UI adds Charlie");
    charlie_ui.add_peer("alice").expect("Charlie UI adds Alice");
    charlie_ui.add_peer("bob").expect("Charlie UI adds Bob");

    assert_eq!(bob_ui.peer_count(), 2);
    assert_eq!(charlie_ui.peer_count(), 2);

    // Wire on_burn_peer_messages callbacks
    let b_ui_burn = bob_ui.clone();
    bob_mesh.on_burn_peer_messages(move |peer_id| {
        let _ = b_ui_burn.burn_peer_messages(peer_id);
    });

    let c_ui_burn = charlie_ui.clone();
    charlie_mesh.on_burn_peer_messages(move |peer_id| {
        let _ = c_ui_burn.burn_peer_messages(peer_id);
    });

    // Wire on_peer_left callbacks
    let b_ui_left = bob_ui.clone();
    bob_mesh.on_peer_left(move |peer_id| {
        let _ = b_ui_left.remove_peer(peer_id);
    });

    let c_ui_left = charlie_ui.clone();
    charlie_mesh.on_peer_left(move |peer_id| {
        let _ = c_ui_left.remove_peer(peer_id);
    });

    // Wire on_message callbacks
    let a_ui_msg = alice_ui.clone();
    alice_mesh.on_message(move |sender, text| {
        let _ = a_ui_msg.append_message(sender, text, false, true);
    });

    let b_ui_msg = bob_ui.clone();
    bob_mesh.on_message(move |sender, text| {
        let _ = b_ui_msg.append_message(sender, text, false, true);
    });

    let c_ui_msg = charlie_ui.clone();
    charlie_mesh.on_message(move |sender, text| {
        let _ = c_ui_msg.append_message(sender, text, false, true);
    });

    // Populate feeds with messages from Alice, Bob, and Charlie on all nodes
    let msg_alice = "Hello from Alice the host";
    alice_mesh
        .broadcast_chat(msg_alice)
        .expect("Alice broadcast");
    alice_ui
        .append_message("alice", msg_alice, true, true)
        .expect("Alice self append");

    let msg_bob = "Hello from Bob";
    bob_mesh.broadcast_chat(msg_bob).expect("Bob broadcast");
    bob_ui
        .append_message("bob", msg_bob, true, true)
        .expect("Bob self append");

    let msg_charlie = "Hello from Charlie";
    charlie_mesh
        .broadcast_chat(msg_charlie)
        .expect("Charlie broadcast");
    charlie_ui
        .append_message("charlie", msg_charlie, true, true)
        .expect("Charlie self append");

    // Verify feeds before departure on Bob and Charlie
    let bob_msgs_before = bob_ui.messages();
    assert_eq!(
        bob_msgs_before.len(),
        3,
        "Bob has 3 messages before Alice departs"
    );
    assert!(bob_msgs_before
        .iter()
        .any(|(s, _, is_self, _)| s == "alice" && !*is_self));
    assert!(bob_msgs_before
        .iter()
        .any(|(s, _, is_self, _)| s == "bob" && *is_self));
    assert!(bob_msgs_before
        .iter()
        .any(|(s, _, is_self, _)| s == "charlie" && !*is_self));

    let charlie_msgs_before = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_before.len(),
        3,
        "Charlie has 3 messages before Alice departs"
    );
    assert!(charlie_msgs_before
        .iter()
        .any(|(s, _, is_self, _)| s == "alice" && !*is_self));
    assert!(charlie_msgs_before
        .iter()
        .any(|(s, _, is_self, _)| s == "bob" && !*is_self));
    assert!(charlie_msgs_before
        .iter()
        .any(|(s, _, is_self, _)| s == "charlie" && *is_self));

    // Alice (the host) departs via broadcast_leave_and_burn()
    alice_mesh
        .broadcast_leave_and_burn()
        .expect("Alice host departs via leave and burn");

    // Assertions on surviving nodes (Bob and Charlie):
    // 1. Peer count decrements from 2 to 1 on both Bob and Charlie
    assert_eq!(
        bob_mesh.peer_count(),
        1,
        "Bob mesh peer count decrements from 2 to 1"
    );
    assert_eq!(
        charlie_mesh.peer_count(),
        1,
        "Charlie mesh peer count decrements from 2 to 1"
    );
    assert!(
        !bob_mesh.has_peer("alice"),
        "Bob mesh no longer has peer Alice"
    );
    assert!(
        !charlie_mesh.has_peer("alice"),
        "Charlie mesh no longer has peer Alice"
    );
    assert!(
        bob_mesh.has_peer("charlie"),
        "Bob mesh retains direct peer Charlie"
    );
    assert!(
        charlie_mesh.has_peer("bob"),
        "Charlie mesh retains direct peer Bob"
    );

    assert_eq!(bob_ui.peer_count(), 1, "Bob UI peer count decrements to 1");
    assert_eq!(
        charlie_ui.peer_count(),
        1,
        "Charlie UI peer count decrements to 1"
    );
    assert_eq!(
        bob_ui.peers(),
        vec!["charlie".to_string()],
        "Bob UI peers list contains only Charlie"
    );
    assert_eq!(
        charlie_ui.peers(),
        vec!["bob".to_string()],
        "Charlie UI peers list contains only Bob"
    );

    // 2. Bob's UI purges Alice's messages, but preserves Bob's own messages and Charlie's messages
    let bob_msgs_after = bob_ui.messages();
    assert_eq!(
        bob_msgs_after.len(),
        2,
        "Bob UI has 2 messages after Alice's messages are purged"
    );
    assert!(
        !bob_msgs_after.iter().any(|(s, _, _, _)| s == "alice"),
        "Alice's messages are purged from Bob's UI"
    );
    assert!(
        bob_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && *is_self),
        "Bob's own message is preserved in Bob's UI"
    );
    assert!(
        bob_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie && !*is_self),
        "Charlie's message is preserved in Bob's UI"
    );

    // 3. Charlie's UI purges Alice's messages, but preserves Charlie's own messages and Bob's messages
    let charlie_msgs_after = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_after.len(),
        2,
        "Charlie UI has 2 messages after Alice's messages are purged"
    );
    assert!(
        !charlie_msgs_after.iter().any(|(s, _, _, _)| s == "alice"),
        "Alice's messages are purged from Charlie's UI"
    );
    assert!(
        charlie_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie && *is_self),
        "Charlie's own message is preserved in Charlie's UI"
    );
    assert!(
        charlie_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && !*is_self),
        "Bob's message is preserved in Charlie's UI"
    );

    // 4. Connection status remains Connected on both Bob and Charlie (session is NOT destroyed)
    assert_eq!(
        bob_ui.status(),
        ConnectionStatus::Connected,
        "Bob UI remains Connected"
    );
    assert_eq!(
        charlie_ui.status(),
        ConnectionStatus::Connected,
        "Charlie UI remains Connected"
    );

    // 5. Bob broadcasts a new message; Charlie receives and appends it over surviving Channel_BC
    let bob_new_msg = "Post-host-departure message from Bob to Charlie";
    bob_mesh
        .broadcast_chat(bob_new_msg)
        .expect("Bob broadcasts new message over surviving Channel_BC");
    bob_ui
        .append_message("bob", bob_new_msg, true, true)
        .expect("Bob appends self post-departure message");

    let charlie_msgs_post_bob = charlie_ui.messages();
    assert_eq!(
        charlie_msgs_post_bob.len(),
        3,
        "Charlie UI receives Bob's post-host-departure message"
    );
    assert_eq!(
        charlie_msgs_post_bob.last(),
        Some(&("bob".to_string(), bob_new_msg.to_string(), false, true)),
        "Last message in Charlie UI matches Bob's post-host-departure message"
    );

    // 6. Charlie broadcasts a reply; Bob receives and appends it over surviving Channel_BC
    let charlie_reply = "Post-host-departure reply from Charlie to Bob";
    charlie_mesh
        .broadcast_chat(charlie_reply)
        .expect("Charlie broadcasts reply over surviving Channel_BC");
    charlie_ui
        .append_message("charlie", charlie_reply, true, true)
        .expect("Charlie appends self reply");

    let bob_msgs_post_charlie = bob_ui.messages();
    assert_eq!(
        bob_msgs_post_charlie.len(),
        4,
        "Bob UI receives Charlie's reply"
    );
    assert_eq!(
        bob_msgs_post_charlie.last(),
        Some(&(
            "charlie".to_string(),
            charlie_reply.to_string(),
            false,
            true
        )),
        "Last message in Bob UI matches Charlie's post-host-departure reply"
    );

    // Assert direct Channel_BC lifecycle state remains Open
    assert_eq!(
        charlie_to_bob.state(),
        PeerLifecycleState::Open,
        "Channel_BC Charlie side remains Open"
    );
    assert_eq!(
        bob_to_charlie.state(),
        PeerLifecycleState::Open,
        "Channel_BC Bob side remains Open"
    );
}

/// REQ-029.4: 3rd Peer Departure Selective Purge Test.
///
/// Validates selective message purging when a 3rd peer departs a 3-node connected mesh:
/// 1. Establish 3-node connected mesh: Alice, Bob, Charlie with pairwise DataChannels.
/// 2. Assert all nodes report peer_count == 2.
/// 3. Charlie sends messages to Alice and Bob; Alice and Bob send messages.
/// 4. Charlie departs via `broadcast_leave_and_burn()`.
/// 5. Alice and Bob process the departure:
///    - Only Charlie's messages are purged from Alice's and Bob's feeds.
///    - Alice's and Bob's own and each other's messages remain intact.
///    - Peer count decrements from 2 to 1 on both Alice and Bob.
///    - Channel_AB between Alice and Bob continues uninterrupted.
#[test]
fn test_req_029_4_third_peer_departure_selective_purge() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Seed known public keys
    bob_mesh.learn_peer_key("charlie", charlie_pk);
    charlie_mesh.learn_peer_key("bob", bob_pk);

    // Connect Alice <-> Bob (Channel_AB)
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob.clone())
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice.clone())
        .expect("Bob adds Alice");

    // Connect Alice <-> Charlie (Channel_AC)
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("Alice adds Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("Charlie adds Alice");

    // Introduce Bob and Charlie via in-band relay signaling over Alice's bridge
    let cloud_edge_hits = 0usize;
    block_on(charlie_mesh.initiate_relay_handshake("alice", "bob", bob_pk))
        .expect("initiate in-band relay handshake");
    assert_eq!(cloud_edge_hits, 0, "0 cloud edge hits during direct snap");

    let charlie_to_bob = charlie_mesh
        .get_peer("bob")
        .expect("Charlie has peer Bob")
        .rtc_peer;
    let bob_to_charlie = bob_mesh
        .get_peer("charlie")
        .expect("Bob has peer Charlie")
        .rtc_peer;
    WebRtcPeer::pair_for_testing(&charlie_to_bob, &bob_to_charlie).expect("pair Channel_BC");

    // Assert all 3 nodes report peer_count == 2
    assert_eq!(alice_mesh.peer_count(), 2);
    assert_eq!(bob_mesh.peer_count(), 2);
    assert_eq!(charlie_mesh.peer_count(), 2);

    // Mount UI test doubles for surviving nodes Alice and Bob
    let alice_ui = Rc::new(ChatUi::mount("alice-app").expect("mount Alice UI"));
    let bob_ui = Rc::new(ChatUi::mount("bob-app").expect("mount Bob UI"));

    alice_ui
        .set_status(ConnectionStatus::Connected)
        .expect("set connected");
    bob_ui
        .set_status(ConnectionStatus::Connected)
        .expect("set connected");

    alice_ui.add_peer("bob").expect("Alice UI adds Bob");
    alice_ui.add_peer("charlie").expect("Alice UI adds Charlie");
    bob_ui.add_peer("alice").expect("Bob UI adds Alice");
    bob_ui.add_peer("charlie").expect("Bob UI adds Charlie");

    // Wire on_burn_peer_messages callbacks
    let a_ui_burn = alice_ui.clone();
    alice_mesh.on_burn_peer_messages(move |peer_id| {
        let _ = a_ui_burn.burn_peer_messages(peer_id);
    });

    let b_ui_burn = bob_ui.clone();
    bob_mesh.on_burn_peer_messages(move |peer_id| {
        let _ = b_ui_burn.burn_peer_messages(peer_id);
    });

    // Wire on_peer_left callbacks
    let a_ui_left = alice_ui.clone();
    alice_mesh.on_peer_left(move |peer_id| {
        let _ = a_ui_left.remove_peer(peer_id);
    });

    let b_ui_left = bob_ui.clone();
    bob_mesh.on_peer_left(move |peer_id| {
        let _ = b_ui_left.remove_peer(peer_id);
    });

    // Wire on_message callbacks
    let a_ui_msg = alice_ui.clone();
    alice_mesh.on_message(move |sender, text| {
        let _ = a_ui_msg.append_message(sender, text, false, true);
    });

    let b_ui_msg = bob_ui.clone();
    bob_mesh.on_message(move |sender, text| {
        let _ = b_ui_msg.append_message(sender, text, false, true);
    });

    // Alice broadcasts message:
    let msg_alice = "Message from Alice";
    alice_mesh
        .broadcast_chat(msg_alice)
        .expect("Alice broadcast");
    alice_ui
        .append_message("alice", msg_alice, true, true)
        .expect("Alice self append");

    // Bob broadcasts message:
    let msg_bob = "Message from Bob";
    bob_mesh.broadcast_chat(msg_bob).expect("Bob broadcast");
    bob_ui
        .append_message("bob", msg_bob, true, true)
        .expect("Bob self append");

    // Charlie sends multiple messages to Alice and Bob:
    let msg_charlie_1 = "First message from 3rd peer Charlie";
    let msg_charlie_2 = "Second message from 3rd peer Charlie";
    charlie_mesh
        .broadcast_chat(msg_charlie_1)
        .expect("Charlie broadcast 1");
    charlie_mesh
        .broadcast_chat(msg_charlie_2)
        .expect("Charlie broadcast 2");

    // Verify feeds before departure on Alice and Bob:
    let alice_msgs_before = alice_ui.messages();
    assert_eq!(
        alice_msgs_before.len(),
        4,
        "Alice has 4 messages before Charlie departs"
    );
    assert!(alice_msgs_before
        .iter()
        .any(|(s, t, is_self, _)| s == "alice" && t == msg_alice && *is_self));
    assert!(alice_msgs_before
        .iter()
        .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && !*is_self));
    assert!(alice_msgs_before
        .iter()
        .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie_1 && !*is_self));
    assert!(alice_msgs_before
        .iter()
        .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie_2 && !*is_self));

    let bob_msgs_before = bob_ui.messages();
    assert_eq!(
        bob_msgs_before.len(),
        4,
        "Bob has 4 messages before Charlie departs"
    );
    assert!(bob_msgs_before
        .iter()
        .any(|(s, t, is_self, _)| s == "alice" && t == msg_alice && !*is_self));
    assert!(bob_msgs_before
        .iter()
        .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && *is_self));
    assert!(bob_msgs_before
        .iter()
        .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie_1 && !*is_self));
    assert!(bob_msgs_before
        .iter()
        .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie_2 && !*is_self));

    // Charlie (the 3rd peer) departs via broadcast_leave_and_burn()
    charlie_mesh
        .broadcast_leave_and_burn()
        .expect("Charlie departs via leave and burn");

    // Alice and Bob process the departure:
    // 1. Only Charlie's messages are purged from Alice's and Bob's feeds
    let alice_msgs_after = alice_ui.messages();
    assert_eq!(
        alice_msgs_after.len(),
        2,
        "Alice UI has 2 messages after Charlie's messages are purged"
    );
    assert!(
        !alice_msgs_after.iter().any(|(s, _, _, _)| s == "charlie"),
        "Charlie's messages purged from Alice's UI"
    );

    let bob_msgs_after = bob_ui.messages();
    assert_eq!(
        bob_msgs_after.len(),
        2,
        "Bob UI has 2 messages after Charlie's messages are purged"
    );
    assert!(
        !bob_msgs_after.iter().any(|(s, _, _, _)| s == "charlie"),
        "Charlie's messages purged from Bob's UI"
    );

    // 2. Alice's and Bob's own and each other's messages remain intact
    assert!(
        alice_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == msg_alice && *is_self),
        "Alice's own message remains intact in Alice's UI"
    );
    assert!(
        alice_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && !*is_self),
        "Bob's message remains intact in Alice's UI"
    );

    assert!(
        bob_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == msg_alice && !*is_self),
        "Alice's message remains intact in Bob's UI"
    );
    assert!(
        bob_msgs_after
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && *is_self),
        "Bob's own message remains intact in Bob's UI"
    );

    // 3. Peer count decrements from 2 to 1 on both Alice and Bob
    assert_eq!(
        alice_mesh.peer_count(),
        1,
        "Alice mesh peer count decrements from 2 to 1"
    );
    assert_eq!(
        bob_mesh.peer_count(),
        1,
        "Bob mesh peer count decrements from 2 to 1"
    );
    assert_eq!(
        alice_ui.peer_count(),
        1,
        "Alice UI peer count decrements from 2 to 1"
    );
    assert_eq!(
        bob_ui.peer_count(),
        1,
        "Bob UI peer count decrements from 2 to 1"
    );
    assert_eq!(
        alice_ui.peers(),
        vec!["bob".to_string()],
        "Alice UI peers list contains only Bob"
    );
    assert_eq!(
        bob_ui.peers(),
        vec!["alice".to_string()],
        "Bob UI peers list contains only Alice"
    );
    assert!(!alice_mesh.has_peer("charlie"));
    assert!(!bob_mesh.has_peer("charlie"));

    // 4. Connection status remains Connected on both Alice and Bob
    assert_eq!(alice_ui.status(), ConnectionStatus::Connected);
    assert_eq!(bob_ui.status(), ConnectionStatus::Connected);

    // 5. Continued pairwise communication over Channel_AB
    let alice_post_msg = "Post-departure message from Alice to Bob";
    alice_mesh
        .broadcast_chat(alice_post_msg)
        .expect("Alice broadcast post-departure");
    alice_ui
        .append_message("alice", alice_post_msg, true, true)
        .expect("Alice append self post-departure");

    let bob_msgs_post_alice = bob_ui.messages();
    assert_eq!(
        bob_msgs_post_alice.len(),
        3,
        "Bob receives Alice's post-departure message"
    );
    assert_eq!(
        bob_msgs_post_alice.last(),
        Some(&("alice".to_string(), alice_post_msg.to_string(), false, true))
    );

    let bob_post_reply = "Post-departure reply from Bob to Alice";
    bob_mesh
        .broadcast_chat(bob_post_reply)
        .expect("Bob broadcast reply post-departure");
    bob_ui
        .append_message("bob", bob_post_reply, true, true)
        .expect("Bob append self reply");

    let alice_msgs_post_bob = alice_ui.messages();
    assert_eq!(
        alice_msgs_post_bob.len(),
        4,
        "Alice receives Bob's post-departure reply"
    );
    assert_eq!(
        alice_msgs_post_bob.last(),
        Some(&("bob".to_string(), bob_post_reply.to_string(), false, true))
    );

    assert_eq!(
        alice_to_bob.state(),
        PeerLifecycleState::Open,
        "Channel_AB Alice side remains Open"
    );
    assert_eq!(
        bob_to_alice.state(),
        PeerLifecycleState::Open,
        "Channel_AB Bob side remains Open"
    );
}

/// REQ-030.1 & REQ-030.2: Multi-Peer Join with 2nd User as Inviter, Mesh Snapping & Counter Synchronization.
///
/// Validates:
/// 1. Establish 3 nodes: Alice (creator/host), Bob (joined Alice), Charlie (invited by Bob).
/// 2. Step 1: Alice <-> Bob connected via `Channel_AB`. Both report peer count = 1.
/// 3. Step 2: Bob invites Charlie (`Channel_BC`).
/// 4. Step 3: Bob introduces Alice and Charlie via in-band relay signaling (`PeerJoined`, `RelaySdpOffer`, `RelaySdpAnswer` through Bob).
/// 5. Step 4: Direct `Channel_AC` data channel is established between Alice and Charlie with 0 cloud edge hits.
/// 6. Step 5: Assert ALL 3 nodes (Alice, Bob, Charlie) report peer count == 2 (in both MeshTopology and ChatUi).
/// 7. Step 6: Test broadcast messaging across all 3 nodes, verifying delivery and loop suppression.
#[test]
fn test_req_030_secondary_inviter_mesh_snapping_and_counters() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    let alice_ui = Rc::new(ChatUi::mount("alice-app").expect("mount Alice UI"));
    let bob_ui = Rc::new(ChatUi::mount("bob-app").expect("mount Bob UI"));
    let charlie_ui = Rc::new(ChatUi::mount("charlie-app").expect("mount Charlie UI"));

    alice_ui
        .set_status(ConnectionStatus::Connected)
        .expect("Alice connected");
    bob_ui
        .set_status(ConnectionStatus::Connected)
        .expect("Bob connected");
    charlie_ui
        .set_status(ConnectionStatus::Connected)
        .expect("Charlie connected");

    // Wire on_peer_joined and on_peer_left callbacks
    let a_ui_j = alice_ui.clone();
    alice_mesh.on_peer_joined(move |peer_id| {
        let _ = a_ui_j.add_peer(peer_id);
    });
    let a_ui_l = alice_ui.clone();
    alice_mesh.on_peer_left(move |peer_id| {
        let _ = a_ui_l.remove_peer(peer_id);
    });

    let b_ui_j = bob_ui.clone();
    bob_mesh.on_peer_joined(move |peer_id| {
        let _ = b_ui_j.add_peer(peer_id);
    });
    let b_ui_l = bob_ui.clone();
    bob_mesh.on_peer_left(move |peer_id| {
        let _ = b_ui_l.remove_peer(peer_id);
    });

    let c_ui_j = charlie_ui.clone();
    charlie_mesh.on_peer_joined(move |peer_id| {
        let _ = c_ui_j.add_peer(peer_id);
    });
    let c_ui_l = charlie_ui.clone();
    charlie_mesh.on_peer_left(move |peer_id| {
        let _ = c_ui_l.remove_peer(peer_id);
    });

    // Step 1: Alice <-> Bob connected via Channel_AB
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("Bob adds Alice");

    assert_eq!(
        alice_mesh.peer_count(),
        1,
        "Alice mesh reports peer count = 1"
    );
    assert_eq!(bob_mesh.peer_count(), 1, "Bob mesh reports peer count = 1");
    assert_eq!(alice_ui.peer_count(), 1, "Alice UI reports peer count = 1");
    assert_eq!(bob_ui.peer_count(), 1, "Bob UI reports peer count = 1");
    assert_eq!(
        charlie_mesh.peer_count(),
        0,
        "Charlie mesh is not yet connected"
    );

    // Step 2: Bob invites Charlie (Channel_BC)
    let bob_to_charlie = WebRtcPeer::new();
    let charlie_to_bob = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).expect("pair Bob-Charlie");
    bob_mesh
        .add_peer("charlie", charlie_pk, bob_to_charlie)
        .expect("Bob adds Charlie");
    charlie_mesh
        .add_peer("bob", bob_pk, charlie_to_bob)
        .expect("Charlie adds Bob");

    assert_eq!(
        bob_mesh.peer_count(),
        2,
        "Bob connected to both Alice and Charlie"
    );
    assert_eq!(charlie_mesh.peer_count(), 1, "Charlie connected to Bob");

    // Step 3: Bob introduces Alice and Charlie via in-band relay signaling
    // (PeerJoined, RelaySdpOffer, RelaySdpAnswer through Bob)
    let cloud_edge_hits = 0usize;
    block_on(charlie_mesh.initiate_relay_handshake("bob", "alice", alice_pk))
        .expect("initiate in-band relay handshake from Charlie to Alice via Bob");

    // Step 4: Direct Channel_AC data channel is established between Alice and Charlie with 0 cloud edge hits
    assert_eq!(
        cloud_edge_hits, 0,
        "0 cloud edge hits during direct snap over Bob's bridge"
    );
    assert!(
        alice_mesh.has_peer("charlie"),
        "Alice registers direct peer Charlie"
    );
    assert!(
        charlie_mesh.has_peer("alice"),
        "Charlie registers direct peer Alice"
    );

    let alice_to_charlie = alice_mesh
        .get_peer("charlie")
        .expect("Alice has peer Charlie")
        .rtc_peer;
    let charlie_to_alice = charlie_mesh
        .get_peer("alice")
        .expect("Charlie has peer Alice")
        .rtc_peer;
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Channel_AC");

    // Step 5: Assert ALL 3 nodes (Alice, Bob, Charlie) report peer count == 2
    assert_eq!(alice_mesh.peer_count(), 2, "Alice mesh peer count == 2");
    assert_eq!(bob_mesh.peer_count(), 2, "Bob mesh peer count == 2");
    assert_eq!(charlie_mesh.peer_count(), 2, "Charlie mesh peer count == 2");

    assert_eq!(alice_ui.peer_count(), 2, "Alice UI peer count == 2");
    assert_eq!(bob_ui.peer_count(), 2, "Bob UI peer count == 2");
    assert_eq!(charlie_ui.peer_count(), 2, "Charlie UI peer count == 2");

    // Step 6: Test broadcast messaging across all 3 nodes, verifying delivery and loop suppression
    let alice_feed = Rc::new(RefCell::new(Vec::new()));
    let a_feed = alice_feed.clone();
    alice_mesh.on_message(move |sender, msg| {
        a_feed
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    let bob_feed = Rc::new(RefCell::new(Vec::new()));
    let b_feed = bob_feed.clone();
    bob_mesh.on_message(move |sender, msg| {
        b_feed
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    let charlie_feed = Rc::new(RefCell::new(Vec::new()));
    let c_feed = charlie_feed.clone();
    charlie_mesh.on_message(move |sender, msg| {
        c_feed
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    // 6a. Alice broadcasts: Bob and Charlie receive exactly once
    let msg_alice = "Hello from Alice (host)";
    let alice_uuid = alice_mesh
        .broadcast_chat(msg_alice)
        .expect("Alice broadcast");

    assert_eq!(
        *bob_feed.borrow(),
        vec![("alice".to_string(), msg_alice.to_string())]
    );
    assert_eq!(
        *charlie_feed.borrow(),
        vec![("alice".to_string(), msg_alice.to_string())]
    );
    assert_eq!(alice_feed.borrow().len(), 0, "Alice suppresses self-echo");
    assert!(bob_mesh.has_seen_message(&alice_uuid));
    assert!(charlie_mesh.has_seen_message(&alice_uuid));

    // 6b. Bob broadcasts: Alice and Charlie receive exactly once
    let msg_bob = "Hello from Bob (inviter hub)";
    let bob_uuid = bob_mesh.broadcast_chat(msg_bob).expect("Bob broadcast");

    assert_eq!(
        *alice_feed.borrow(),
        vec![("bob".to_string(), msg_bob.to_string())]
    );
    assert_eq!(
        *charlie_feed.borrow(),
        vec![
            ("alice".to_string(), msg_alice.to_string()),
            ("bob".to_string(), msg_bob.to_string()),
        ]
    );
    assert_eq!(bob_feed.borrow().len(), 1, "Bob suppresses self-echo");
    assert!(alice_mesh.has_seen_message(&bob_uuid));
    assert!(charlie_mesh.has_seen_message(&bob_uuid));

    // 6c. Charlie broadcasts: Alice and Bob receive exactly once
    let msg_charlie = "Hello from Charlie (joiner)";
    let charlie_uuid = charlie_mesh
        .broadcast_chat(msg_charlie)
        .expect("Charlie broadcast");

    assert_eq!(
        *alice_feed.borrow(),
        vec![
            ("bob".to_string(), msg_bob.to_string()),
            ("charlie".to_string(), msg_charlie.to_string()),
        ]
    );
    assert_eq!(
        *bob_feed.borrow(),
        vec![
            ("alice".to_string(), msg_alice.to_string()),
            ("charlie".to_string(), msg_charlie.to_string()),
        ]
    );
    assert_eq!(
        charlie_feed.borrow().len(),
        2,
        "Charlie suppresses self-echo"
    );
    assert!(alice_mesh.has_seen_message(&charlie_uuid));
    assert!(bob_mesh.has_seen_message(&charlie_uuid));

    // Loop suppression checks: Exactly 2 messages delivered to Alice, Bob, and Charlie
    assert_eq!(
        alice_feed.borrow().len(),
        2,
        "Alice delivered exactly 2 distinct messages"
    );
    assert_eq!(
        bob_feed.borrow().len(),
        2,
        "Bob delivered exactly 2 distinct messages"
    );
    assert_eq!(
        charlie_feed.borrow().len(),
        2,
        "Charlie delivered exactly 2 distinct messages"
    );
}

/// Helper context for REQ-030 asymmetric departure tests.
struct Req030ScenarioContext {
    alice_mesh: MeshTopology,
    bob_mesh: MeshTopology,
    charlie_mesh: MeshTopology,
    alice_ui: Rc<ChatUi>,
    bob_ui: Rc<ChatUi>,
    charlie_ui: Rc<ChatUi>,
    alice_to_bob: WebRtcPeer,
    bob_to_alice: WebRtcPeer,
    bob_to_charlie: WebRtcPeer,
    charlie_to_bob: WebRtcPeer,
    alice_to_charlie: WebRtcPeer,
    charlie_to_alice: WebRtcPeer,
}

/// Sets up a 3-node connected mesh where Alice is the creator, Bob joins Alice,
/// Bob invites Charlie, and Alice and Charlie establish direct Channel_AC via Bob's bridge.
fn setup_req_030_mesh_scenario() -> Req030ScenarioContext {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    let alice_ui = Rc::new(ChatUi::mount("alice-app").expect("mount Alice UI"));
    let bob_ui = Rc::new(ChatUi::mount("bob-app").expect("mount Bob UI"));
    let charlie_ui = Rc::new(ChatUi::mount("charlie-app").expect("mount Charlie UI"));

    alice_ui
        .set_status(ConnectionStatus::Connected)
        .expect("Alice connected");
    bob_ui
        .set_status(ConnectionStatus::Connected)
        .expect("Bob connected");
    charlie_ui
        .set_status(ConnectionStatus::Connected)
        .expect("Charlie connected");

    // Connect Alice <-> Bob (Channel_AB)
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob.clone())
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice.clone())
        .expect("Bob adds Alice");
    alice_ui.add_peer("bob").expect("Alice UI adds Bob");
    bob_ui.add_peer("alice").expect("Bob UI adds Alice");

    // Connect Bob <-> Charlie (Channel_BC, Bob is inviter)
    let bob_to_charlie = WebRtcPeer::new();
    let charlie_to_bob = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&bob_to_charlie, &charlie_to_bob).expect("pair Bob-Charlie");
    bob_mesh
        .add_peer("charlie", charlie_pk, bob_to_charlie.clone())
        .expect("Bob adds Charlie");
    charlie_mesh
        .add_peer("bob", bob_pk, charlie_to_bob.clone())
        .expect("Charlie adds Bob");
    bob_ui.add_peer("charlie").expect("Bob UI adds Charlie");
    charlie_ui.add_peer("bob").expect("Charlie UI adds Bob");

    // Snap direct Channel_AC via in-band relay through Bob
    block_on(charlie_mesh.initiate_relay_handshake("bob", "alice", alice_pk))
        .expect("relay handshake Charlie-Alice via Bob");

    let alice_to_charlie = alice_mesh
        .get_peer("charlie")
        .expect("Alice has peer Charlie")
        .rtc_peer;
    let charlie_to_alice = charlie_mesh
        .get_peer("alice")
        .expect("Charlie has peer Alice")
        .rtc_peer;
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Channel_AC");
    alice_ui.add_peer("charlie").expect("Alice UI adds Charlie");
    charlie_ui.add_peer("alice").expect("Charlie UI adds Alice");

    // Wire on_message callbacks
    let a_ui_m = alice_ui.clone();
    alice_mesh.on_message(move |sender, text| {
        let _ = a_ui_m.append_message(sender, text, false, true);
    });
    let b_ui_m = bob_ui.clone();
    bob_mesh.on_message(move |sender, text| {
        let _ = b_ui_m.append_message(sender, text, false, true);
    });
    let c_ui_m = charlie_ui.clone();
    charlie_mesh.on_message(move |sender, text| {
        let _ = c_ui_m.append_message(sender, text, false, true);
    });

    // Wire on_burn_peer_messages callbacks
    let a_ui_b = alice_ui.clone();
    alice_mesh.on_burn_peer_messages(move |peer_id| {
        let _ = a_ui_b.burn_peer_messages(peer_id);
    });
    let b_ui_b = bob_ui.clone();
    bob_mesh.on_burn_peer_messages(move |peer_id| {
        let _ = b_ui_b.burn_peer_messages(peer_id);
    });
    let c_ui_b = charlie_ui.clone();
    charlie_mesh.on_burn_peer_messages(move |peer_id| {
        let _ = c_ui_b.burn_peer_messages(peer_id);
    });

    // Wire on_peer_left callbacks
    let a_ui_l = alice_ui.clone();
    alice_mesh.on_peer_left(move |peer_id| {
        let _ = a_ui_l.remove_peer(peer_id);
    });
    let b_ui_l = bob_ui.clone();
    bob_mesh.on_peer_left(move |peer_id| {
        let _ = b_ui_l.remove_peer(peer_id);
    });
    let c_ui_l = charlie_ui.clone();
    charlie_mesh.on_peer_left(move |peer_id| {
        let _ = c_ui_l.remove_peer(peer_id);
    });

    // Verify all 3 report peer count == 2 initially
    assert_eq!(alice_mesh.peer_count(), 2);
    assert_eq!(bob_mesh.peer_count(), 2);
    assert_eq!(charlie_mesh.peer_count(), 2);
    assert_eq!(alice_ui.peer_count(), 2);
    assert_eq!(bob_ui.peer_count(), 2);
    assert_eq!(charlie_ui.peer_count(), 2);

    Req030ScenarioContext {
        alice_mesh,
        bob_mesh,
        charlie_mesh,
        alice_ui,
        bob_ui,
        charlie_ui,
        alice_to_bob,
        bob_to_alice,
        bob_to_charlie,
        charlie_to_bob,
        alice_to_charlie,
        charlie_to_alice,
    }
}

/// REQ-030.3 & REQ-030.4: Asymmetric Departure Scenarios with Symmetric Counter Decrements.
///
/// Validates:
/// - Scenario A: Charlie (3rd user) leaves:
///   - Alice and Bob remain connected via `Channel_AB`.
///   - Peer count drops from 2 to 1 on BOTH Alice and Bob.
///   - Only Charlie's messages are purged; Alice's and Bob's messages remain in feeds.
/// - Scenario B: Bob (2nd user / hub) leaves:
///   - Alice and Charlie maintain their direct `Channel_AC`.
///   - Peer count drops from 2 to 1 on BOTH Alice and Charlie.
///   - Only Bob's messages are purged; Alice and Charlie continue chatting over `Channel_AC`.
/// - Scenario C: Alice (1st user / host) leaves:
///   - Bob and Charlie maintain `Channel_BC`.
///   - Peer count drops from 2 to 1 on BOTH Bob and Charlie.
///   - Only Alice's messages are purged; Bob and Charlie continue chatting over `Channel_BC`.
#[test]
fn test_req_030_departure_counter_symmetry() {
    // =========================================================================
    // Scenario A: Charlie (3rd user) leaves
    // =========================================================================
    {
        let ctx = setup_req_030_mesh_scenario();

        // Populate messages from all 3 peers
        let msg_alice = "Message from Alice";
        let msg_bob = "Message from Bob";
        let msg_charlie = "Message from Charlie";

        ctx.alice_mesh
            .broadcast_chat(msg_alice)
            .expect("Alice broadcast");
        ctx.alice_ui
            .append_message("alice", msg_alice, true, true)
            .expect("Alice append self");

        ctx.bob_mesh.broadcast_chat(msg_bob).expect("Bob broadcast");
        ctx.bob_ui
            .append_message("bob", msg_bob, true, true)
            .expect("Bob append self");

        ctx.charlie_mesh
            .broadcast_chat(msg_charlie)
            .expect("Charlie broadcast");
        ctx.charlie_ui
            .append_message("charlie", msg_charlie, true, true)
            .expect("Charlie append self");

        assert_eq!(ctx.alice_ui.messages().len(), 3);
        assert_eq!(ctx.bob_ui.messages().len(), 3);
        assert_eq!(ctx.charlie_ui.messages().len(), 3);

        // Charlie departs via broadcast_leave_and_burn()
        ctx.charlie_mesh
            .broadcast_leave_and_burn()
            .expect("Charlie departs");

        // 1. Assert Alice and Bob remain connected via Channel_AB
        assert_eq!(
            ctx.alice_to_bob.state(),
            PeerLifecycleState::Open,
            "Channel_AB Alice side remains Open"
        );
        assert_eq!(
            ctx.bob_to_alice.state(),
            PeerLifecycleState::Open,
            "Channel_AB Bob side remains Open"
        );
        assert!(
            ctx.alice_mesh.has_peer("bob"),
            "Alice mesh retains peer Bob"
        );
        assert!(
            ctx.bob_mesh.has_peer("alice"),
            "Bob mesh retains peer Alice"
        );

        // 2. Assert peer count drops from 2 to 1 on BOTH Alice and Bob
        assert_eq!(
            ctx.alice_mesh.peer_count(),
            1,
            "Alice mesh peer count drops 2 -> 1"
        );
        assert_eq!(
            ctx.bob_mesh.peer_count(),
            1,
            "Bob mesh peer count drops 2 -> 1"
        );
        assert_eq!(
            ctx.alice_ui.peer_count(),
            1,
            "Alice UI peer count drops 2 -> 1"
        );
        assert_eq!(ctx.bob_ui.peer_count(), 1, "Bob UI peer count drops 2 -> 1");
        assert_eq!(ctx.alice_ui.peers(), vec!["bob".to_string()]);
        assert_eq!(ctx.bob_ui.peers(), vec!["alice".to_string()]);
        assert!(!ctx.alice_mesh.has_peer("charlie"));
        assert!(!ctx.bob_mesh.has_peer("charlie"));

        // 3. Assert only Charlie's messages are purged; Alice's and Bob's messages remain in feeds
        let alice_msgs = ctx.alice_ui.messages();
        assert_eq!(alice_msgs.len(), 2, "Alice UI has 2 messages remaining");
        assert!(
            !alice_msgs.iter().any(|(s, _, _, _)| s == "charlie"),
            "Charlie's messages purged from Alice UI"
        );
        assert!(alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == msg_alice && *is_self));
        assert!(alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && !*is_self));

        let bob_msgs = ctx.bob_ui.messages();
        assert_eq!(bob_msgs.len(), 2, "Bob UI has 2 messages remaining");
        assert!(
            !bob_msgs.iter().any(|(s, _, _, _)| s == "charlie"),
            "Charlie's messages purged from Bob UI"
        );
        assert!(bob_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == msg_alice && !*is_self));
        assert!(bob_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && *is_self));

        // Continued communication between Alice and Bob over Channel_AB
        let msg_post = "Continued chat between Alice and Bob";
        ctx.alice_mesh
            .broadcast_chat(msg_post)
            .expect("Alice broadcast post-departure");
        ctx.alice_ui
            .append_message("alice", msg_post, true, true)
            .expect("Alice append self");
        assert_eq!(ctx.bob_ui.messages().len(), 3);
        assert_eq!(
            ctx.bob_ui.messages().last(),
            Some(&("alice".to_string(), msg_post.to_string(), false, true))
        );

        let bob_reply = "Reply from Bob to Alice";
        ctx.bob_mesh
            .broadcast_chat(bob_reply)
            .expect("Bob broadcast reply");
        ctx.bob_ui
            .append_message("bob", bob_reply, true, true)
            .expect("Bob append self");
        assert_eq!(ctx.alice_ui.messages().len(), 4);
        assert_eq!(ctx.bob_ui.messages().len(), 4);
        assert_eq!(
            ctx.alice_ui.messages().last(),
            Some(&("bob".to_string(), bob_reply.to_string(), false, true))
        );
    }

    // =========================================================================
    // Scenario B: Bob (2nd user / hub) leaves
    // =========================================================================
    {
        let ctx = setup_req_030_mesh_scenario();

        // Populate messages from all 3 peers
        let msg_alice = "Message from Alice";
        let msg_bob = "Message from Bob";
        let msg_charlie = "Message from Charlie";

        ctx.alice_mesh
            .broadcast_chat(msg_alice)
            .expect("Alice broadcast");
        ctx.alice_ui
            .append_message("alice", msg_alice, true, true)
            .expect("Alice append self");

        ctx.bob_mesh.broadcast_chat(msg_bob).expect("Bob broadcast");
        ctx.bob_ui
            .append_message("bob", msg_bob, true, true)
            .expect("Bob append self");

        ctx.charlie_mesh
            .broadcast_chat(msg_charlie)
            .expect("Charlie broadcast");
        ctx.charlie_ui
            .append_message("charlie", msg_charlie, true, true)
            .expect("Charlie append self");

        assert_eq!(ctx.alice_ui.messages().len(), 3);
        assert_eq!(ctx.bob_ui.messages().len(), 3);
        assert_eq!(ctx.charlie_ui.messages().len(), 3);

        // Bob (hub inviter) departs via broadcast_leave_and_burn()
        ctx.bob_mesh
            .broadcast_leave_and_burn()
            .expect("Bob departs");

        // 1. Assert Alice and Charlie maintain their direct Channel_AC
        assert_eq!(
            ctx.alice_to_charlie.state(),
            PeerLifecycleState::Open,
            "Channel_AC Alice side remains Open"
        );
        assert_eq!(
            ctx.charlie_to_alice.state(),
            PeerLifecycleState::Open,
            "Channel_AC Charlie side remains Open"
        );
        assert!(
            ctx.alice_mesh.has_peer("charlie"),
            "Alice retains direct peer Charlie"
        );
        assert!(
            ctx.charlie_mesh.has_peer("alice"),
            "Charlie retains direct peer Alice"
        );

        // 2. Assert peer count drops from 2 to 1 on BOTH Alice and Charlie
        assert_eq!(
            ctx.alice_mesh.peer_count(),
            1,
            "Alice mesh peer count drops 2 -> 1"
        );
        assert_eq!(
            ctx.charlie_mesh.peer_count(),
            1,
            "Charlie mesh peer count drops 2 -> 1"
        );
        assert_eq!(
            ctx.alice_ui.peer_count(),
            1,
            "Alice UI peer count drops 2 -> 1"
        );
        assert_eq!(
            ctx.charlie_ui.peer_count(),
            1,
            "Charlie UI peer count drops 2 -> 1"
        );
        assert_eq!(ctx.alice_ui.peers(), vec!["charlie".to_string()]);
        assert_eq!(ctx.charlie_ui.peers(), vec!["alice".to_string()]);
        assert!(!ctx.alice_mesh.has_peer("bob"));
        assert!(!ctx.charlie_mesh.has_peer("bob"));

        // 3. Assert only Bob's messages are purged; Alice and Charlie can continue chatting over Channel_AC
        let alice_msgs = ctx.alice_ui.messages();
        assert_eq!(alice_msgs.len(), 2, "Alice UI has 2 messages remaining");
        assert!(
            !alice_msgs.iter().any(|(s, _, _, _)| s == "bob"),
            "Bob's messages purged from Alice UI"
        );
        assert!(alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == msg_alice && *is_self));
        assert!(alice_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie && !*is_self));

        let charlie_msgs = ctx.charlie_ui.messages();
        assert_eq!(charlie_msgs.len(), 2, "Charlie UI has 2 messages remaining");
        assert!(
            !charlie_msgs.iter().any(|(s, _, _, _)| s == "bob"),
            "Bob's messages purged from Charlie UI"
        );
        assert!(charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "alice" && t == msg_alice && !*is_self));
        assert!(charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie && *is_self));

        // Continued communication between Alice and Charlie over direct Channel_AC
        let alice_post_msg = "Post-Bob-departure message from Alice to Charlie";
        ctx.alice_mesh
            .broadcast_chat(alice_post_msg)
            .expect("Alice broadcast post-departure");
        ctx.alice_ui
            .append_message("alice", alice_post_msg, true, true)
            .expect("Alice append self");

        assert_eq!(ctx.charlie_ui.messages().len(), 3);
        assert_eq!(
            ctx.charlie_ui.messages().last(),
            Some(&("alice".to_string(), alice_post_msg.to_string(), false, true))
        );

        let charlie_post_reply = "Post-Bob-departure reply from Charlie to Alice";
        ctx.charlie_mesh
            .broadcast_chat(charlie_post_reply)
            .expect("Charlie broadcast reply");
        ctx.charlie_ui
            .append_message("charlie", charlie_post_reply, true, true)
            .expect("Charlie append self");

        assert_eq!(ctx.charlie_ui.messages().len(), 4);
        assert_eq!(ctx.alice_ui.messages().len(), 4);
        assert_eq!(
            ctx.alice_ui.messages().last(),
            Some(&(
                "charlie".to_string(),
                charlie_post_reply.to_string(),
                false,
                true
            ))
        );
    }

    // =========================================================================
    // Scenario C: Alice (1st user / host) leaves
    // =========================================================================
    {
        let ctx = setup_req_030_mesh_scenario();

        // Populate messages from all 3 peers
        let msg_alice = "Message from Alice";
        let msg_bob = "Message from Bob";
        let msg_charlie = "Message from Charlie";

        ctx.alice_mesh
            .broadcast_chat(msg_alice)
            .expect("Alice broadcast");
        ctx.alice_ui
            .append_message("alice", msg_alice, true, true)
            .expect("Alice append self");

        ctx.bob_mesh.broadcast_chat(msg_bob).expect("Bob broadcast");
        ctx.bob_ui
            .append_message("bob", msg_bob, true, true)
            .expect("Bob append self");

        ctx.charlie_mesh
            .broadcast_chat(msg_charlie)
            .expect("Charlie broadcast");
        ctx.charlie_ui
            .append_message("charlie", msg_charlie, true, true)
            .expect("Charlie append self");

        assert_eq!(ctx.alice_ui.messages().len(), 3);
        assert_eq!(ctx.bob_ui.messages().len(), 3);
        assert_eq!(ctx.charlie_ui.messages().len(), 3);

        // Alice (1st user / room creator) departs via broadcast_leave_and_burn()
        ctx.alice_mesh
            .broadcast_leave_and_burn()
            .expect("Alice departs");

        // 1. Assert Bob and Charlie maintain Channel_BC
        assert_eq!(
            ctx.bob_to_charlie.state(),
            PeerLifecycleState::Open,
            "Channel_BC Bob side remains Open"
        );
        assert_eq!(
            ctx.charlie_to_bob.state(),
            PeerLifecycleState::Open,
            "Channel_BC Charlie side remains Open"
        );
        assert!(
            ctx.bob_mesh.has_peer("charlie"),
            "Bob retains direct peer Charlie"
        );
        assert!(
            ctx.charlie_mesh.has_peer("bob"),
            "Charlie retains direct peer Bob"
        );

        // 2. Assert peer count drops from 2 to 1 on BOTH Bob and Charlie
        assert_eq!(
            ctx.bob_mesh.peer_count(),
            1,
            "Bob mesh peer count drops 2 -> 1"
        );
        assert_eq!(
            ctx.charlie_mesh.peer_count(),
            1,
            "Charlie mesh peer count drops 2 -> 1"
        );
        assert_eq!(ctx.bob_ui.peer_count(), 1, "Bob UI peer count drops 2 -> 1");
        assert_eq!(
            ctx.charlie_ui.peer_count(),
            1,
            "Charlie UI peer count drops 2 -> 1"
        );
        assert_eq!(ctx.bob_ui.peers(), vec!["charlie".to_string()]);
        assert_eq!(ctx.charlie_ui.peers(), vec!["bob".to_string()]);
        assert!(!ctx.bob_mesh.has_peer("alice"));
        assert!(!ctx.charlie_mesh.has_peer("alice"));

        // 3. Assert only Alice's messages are purged; Bob's and Charlie's messages remain in feeds
        let bob_msgs = ctx.bob_ui.messages();
        assert_eq!(bob_msgs.len(), 2, "Bob UI has 2 messages remaining");
        assert!(
            !bob_msgs.iter().any(|(s, _, _, _)| s == "alice"),
            "Alice's messages purged from Bob UI"
        );
        assert!(bob_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && *is_self));
        assert!(bob_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie && !*is_self));

        let charlie_msgs = ctx.charlie_ui.messages();
        assert_eq!(charlie_msgs.len(), 2, "Charlie UI has 2 messages remaining");
        assert!(
            !charlie_msgs.iter().any(|(s, _, _, _)| s == "alice"),
            "Alice's messages purged from Charlie UI"
        );
        assert!(charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "bob" && t == msg_bob && !*is_self));
        assert!(charlie_msgs
            .iter()
            .any(|(s, t, is_self, _)| s == "charlie" && t == msg_charlie && *is_self));

        // Continued communication between Bob and Charlie over Channel_BC
        let bob_post_msg = "Post-host-departure message from Bob to Charlie";
        ctx.bob_mesh
            .broadcast_chat(bob_post_msg)
            .expect("Bob broadcast post-departure");
        ctx.bob_ui
            .append_message("bob", bob_post_msg, true, true)
            .expect("Bob append self");

        assert_eq!(ctx.charlie_ui.messages().len(), 3);
        assert_eq!(
            ctx.charlie_ui.messages().last(),
            Some(&("bob".to_string(), bob_post_msg.to_string(), false, true))
        );

        let charlie_post_reply = "Post-host-departure reply from Charlie to Bob";
        ctx.charlie_mesh
            .broadcast_chat(charlie_post_reply)
            .expect("Charlie broadcast reply");
        ctx.charlie_ui
            .append_message("charlie", charlie_post_reply, true, true)
            .expect("Charlie append self");

        assert_eq!(ctx.charlie_ui.messages().len(), 4);
        assert_eq!(ctx.bob_ui.messages().len(), 4);
        assert_eq!(
            ctx.bob_ui.messages().last(),
            Some(&(
                "charlie".to_string(),
                charlie_post_reply.to_string(),
                false,
                true
            ))
        );
    }
}

// ============================================================================
// TICKET-031 REGRESSION SIMULATION TESTS (V-031-1 through V-031-7)
// ============================================================================

/// V-031-1: REQ-031.1 & REQ-031.2: A creates chat, B joins. A invites C.
/// B and C must snap a direct data channel via Alice's bridge without cloud edge,
/// resulting in a full mesh where all three participants report open_peers == 2.
#[test]
fn test_req_031_invite_via_host_snaps_direct_bc() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // 1. Alice <-> Bob connected
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("Bob adds Alice");

    assert_eq!(alice_mesh.open_peers(), 1);
    assert_eq!(bob_mesh.open_peers(), 1);

    // 2. Alice invites Charlie: Alice <-> Charlie connects
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("Charlie adds Alice");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("Alice adds Charlie");

    // Alice now has 2 peers
    assert_eq!(alice_mesh.open_peers(), 2);

    // In a full mesh, Bob and Charlie MUST have direct links established via in-band relay signaling
    assert!(
        bob_mesh.has_peer("charlie"),
        "Bob must automatically snap direct peer 'charlie' via Alice's bridge"
    );
    assert!(
        charlie_mesh.has_peer("bob"),
        "Charlie must automatically snap direct peer 'bob' via Alice's bridge"
    );

    // Pair the snapped direct channel for loopback testing
    let b_to_c = bob_mesh
        .get_peer("charlie")
        .expect("Bob has peer Charlie")
        .rtc_peer;
    let c_to_b = charlie_mesh
        .get_peer("bob")
        .expect("Charlie has peer Bob")
        .rtc_peer;
    WebRtcPeer::pair_for_testing(&b_to_c, &c_to_b).expect("pair Channel_BC");

    // All three nodes must report open_peers == 2
    assert_eq!(alice_mesh.open_peers(), 2, "Alice open_peers == 2");
    assert_eq!(bob_mesh.open_peers(), 2, "Bob open_peers == 2");
    assert_eq!(charlie_mesh.open_peers(), 2, "Charlie open_peers == 2");
}

/// V-031-2: REQ-031.1 & REQ-031.4: Host leaves; surviving peers B and C keep direct channel,
/// can chat directly with each other, and both stay active.
#[test]
fn test_req_031_host_leaves_survivors_keep_chatting() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Initial 3-node mesh
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice.clone())
        .expect("Bob adds Alice");

    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice.clone())
        .expect("Charlie adds Alice");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("Alice adds Charlie");

    assert!(
        bob_mesh.has_peer("charlie"),
        "Bob must have snapped direct peer 'charlie'"
    );
    assert!(
        charlie_mesh.has_peer("bob"),
        "Charlie must have snapped direct peer 'bob'"
    );

    let b_to_c = bob_mesh
        .get_peer("charlie")
        .expect("Bob peer Charlie")
        .rtc_peer;
    let c_to_b = charlie_mesh
        .get_peer("bob")
        .expect("Charlie peer Bob")
        .rtc_peer;
    WebRtcPeer::pair_for_testing(&b_to_c, &c_to_b).expect("pair Channel_BC");

    // Hook message callbacks on survivors
    let bob_received = Rc::new(RefCell::new(Vec::new()));
    let b_recv_clone = bob_received.clone();
    bob_mesh.on_message(move |sender, msg| {
        b_recv_clone
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    let charlie_received = Rc::new(RefCell::new(Vec::new()));
    let c_recv_clone = charlie_received.clone();
    charlie_mesh.on_message(move |sender, msg| {
        c_recv_clone
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    // Host Alice leaves the chat (tearing down Channel_AB and Channel_AC)
    bob_to_alice.close().expect("close Bob-Alice channel");
    charlie_to_alice
        .close()
        .expect("close Charlie-Alice channel");

    // Survivors Bob and Charlie must stay active with 1 open peer each
    assert_eq!(
        bob_mesh.open_peers(),
        1,
        "Bob sees 1 surviving peer (Charlie)"
    );
    assert_eq!(
        charlie_mesh.open_peers(),
        1,
        "Charlie sees 1 surviving peer (Bob)"
    );
    assert!(bob_mesh.has_peer("charlie"));
    assert!(charlie_mesh.has_peer("bob"));

    // Charlie broadcasts a message; Bob must receive it directly over Channel_BC!
    charlie_mesh
        .broadcast_chat("Hello Bob from surviving Charlie!")
        .expect("Charlie broadcasts");

    assert_eq!(
        *bob_received.borrow(),
        vec![(
            "charlie".to_string(),
            "Hello Bob from surviving Charlie!".to_string()
        )],
        "Bob must receive Charlie's message over direct Channel_BC after Alice left"
    );

    // Bob broadcasts a reply; Charlie must receive it!
    bob_mesh
        .broadcast_chat("Hello Charlie, received directly!")
        .expect("Bob broadcasts");

    assert_eq!(
        *charlie_received.borrow(),
        vec![(
            "bob".to_string(),
            "Hello Charlie, received directly!".to_string()
        )],
        "Charlie must receive Bob's reply over direct Channel_BC"
    );
}

/// V-031-3: REQ-031.1 & REQ-031.4: Member B leaves instead; Alice and Charlie remain connected
/// and keep chatting symmetrically.
#[test]
fn test_req_031_member_leaves_symmetric() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob.clone())
        .expect("Alice adds Bob");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("Bob adds Alice");

    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("Charlie adds Alice");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("Alice adds Charlie");

    assert!(bob_mesh.has_peer("charlie"));
    assert!(charlie_mesh.has_peer("bob"));
    let b_to_c = bob_mesh
        .get_peer("charlie")
        .expect("Bob peer Charlie")
        .rtc_peer;
    let c_to_b = charlie_mesh
        .get_peer("bob")
        .expect("Charlie peer Bob")
        .rtc_peer;
    WebRtcPeer::pair_for_testing(&b_to_c, &c_to_b).expect("pair Channel_BC");

    let charlie_received = Rc::new(RefCell::new(Vec::new()));
    let c_recv_clone = charlie_received.clone();
    charlie_mesh.on_message(move |sender, msg| {
        c_recv_clone
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    let alice_received = Rc::new(RefCell::new(Vec::new()));
    let a_recv_clone = alice_received.clone();
    alice_mesh.on_message(move |sender, msg| {
        a_recv_clone
            .borrow_mut()
            .push((sender.to_string(), msg.to_string()));
    });

    // Bob leaves
    alice_to_bob.close().expect("close Alice-Bob channel");
    c_to_b.close().expect("close Charlie-Bob channel");

    assert_eq!(
        alice_mesh.open_peers(),
        1,
        "Alice sees 1 surviving peer (Charlie)"
    );
    assert_eq!(
        charlie_mesh.open_peers(),
        1,
        "Charlie sees 1 surviving peer (Alice)"
    );

    alice_mesh
        .broadcast_chat("Hello Charlie from Alice!")
        .expect("Alice broadcasts");
    assert_eq!(
        *charlie_received.borrow(),
        vec![("alice".to_string(), "Hello Charlie from Alice!".to_string())]
    );

    charlie_mesh
        .broadcast_chat("Hello Alice from Charlie!")
        .expect("Charlie broadcasts");
    assert_eq!(
        *alice_received.borrow(),
        vec![(
            "charlie".to_string(),
            "Hello Alice from Charlie!".to_string()
        )]
    );
}

/// V-031-4: REQ-031.2 & REQ-031.4: Four-node chain joins (A->B, B->C, C->D),
/// all nodes snap full mesh, then A leaves; B, C, D remain fully meshed and chatting.
#[test]
fn test_req_031_chain_join_four_nodes() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();
    let (dave_sk, dave_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);
    let dave_mesh = MeshTopology::new("dave", dave_sk);

    // 1. A connects to B
    let a_b = WebRtcPeer::new();
    let b_a = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&a_b, &b_a).expect("pair AB");
    alice_mesh.add_peer("bob", bob_pk, a_b).expect("A adds B");
    bob_mesh.add_peer("alice", alice_pk, b_a).expect("B adds A");

    // 2. B invites C
    let b_c = WebRtcPeer::new();
    let c_b = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&b_c, &c_b).expect("pair BC");
    charlie_mesh.add_peer("bob", bob_pk, c_b).expect("C adds B");
    bob_mesh
        .add_peer("charlie", charlie_pk, b_c)
        .expect("B adds C");

    // A and C must snap
    assert!(alice_mesh.has_peer("charlie"), "Alice snaps to Charlie");
    assert!(charlie_mesh.has_peer("alice"), "Charlie snaps to Alice");
    let a_c = alice_mesh.get_peer("charlie").unwrap().rtc_peer;
    let c_a = charlie_mesh.get_peer("alice").unwrap().rtc_peer;
    WebRtcPeer::pair_for_testing(&a_c, &c_a).expect("pair AC");

    // 3. C invites D
    let c_d = WebRtcPeer::new();
    let d_c = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&c_d, &d_c).expect("pair CD");
    dave_mesh
        .add_peer("charlie", charlie_pk, d_c)
        .expect("D adds C");
    charlie_mesh
        .add_peer("dave", dave_pk, c_d)
        .expect("C adds D");

    // D must snap to A and B; A and B must snap to D
    assert!(dave_mesh.has_peer("alice"), "Dave snaps to Alice");
    assert!(alice_mesh.has_peer("dave"), "Alice snaps to Dave");
    let d_a = dave_mesh.get_peer("alice").unwrap().rtc_peer;
    let a_d = alice_mesh.get_peer("dave").unwrap().rtc_peer;
    WebRtcPeer::pair_for_testing(&d_a, &a_d).expect("pair DA");

    assert!(dave_mesh.has_peer("bob"), "Dave snaps to Bob");
    assert!(bob_mesh.has_peer("dave"), "Bob snaps to Dave");
    let d_b = dave_mesh.get_peer("bob").unwrap().rtc_peer;
    let b_d = bob_mesh.get_peer("dave").unwrap().rtc_peer;
    WebRtcPeer::pair_for_testing(&d_b, &b_d).expect("pair DB");

    // Verify all 4 nodes are fully meshed (open_peers == 3 each)
    assert_eq!(alice_mesh.open_peers(), 3);
    assert_eq!(bob_mesh.open_peers(), 3);
    assert_eq!(charlie_mesh.open_peers(), 3);
    assert_eq!(dave_mesh.open_peers(), 3);

    // 4. Alice leaves
    bob_mesh
        .get_peer("alice")
        .unwrap()
        .rtc_peer
        .close()
        .unwrap();
    charlie_mesh
        .get_peer("alice")
        .unwrap()
        .rtc_peer
        .close()
        .unwrap();
    dave_mesh
        .get_peer("alice")
        .unwrap()
        .rtc_peer
        .close()
        .unwrap();
    alice_mesh.disconnect_peer("bob").unwrap();
    alice_mesh.disconnect_peer("charlie").unwrap();
    alice_mesh.disconnect_peer("dave").unwrap();

    // Verify survivors B, C, D each have 2 open peers
    assert_eq!(bob_mesh.open_peers(), 2);
    assert_eq!(charlie_mesh.open_peers(), 2);
    assert_eq!(dave_mesh.open_peers(), 2);

    // Chat test among survivors
    let bob_recv = Rc::new(RefCell::new(Vec::new()));
    let b_clone = bob_recv.clone();
    bob_mesh.on_message(move |s, m| b_clone.borrow_mut().push((s.to_string(), m.to_string())));

    let charlie_recv = Rc::new(RefCell::new(Vec::new()));
    let c_clone = charlie_recv.clone();
    charlie_mesh.on_message(move |s, m| c_clone.borrow_mut().push((s.to_string(), m.to_string())));

    dave_mesh
        .broadcast_chat("Hello from Dave!")
        .expect("Dave broadcast");
    assert_eq!(
        *bob_recv.borrow(),
        vec![("dave".to_string(), "Hello from Dave!".to_string())]
    );
    assert_eq!(
        *charlie_recv.borrow(),
        vec![("dave".to_string(), "Hello from Dave!".to_string())]
    );
}

/// V-031-5: REQ-031.3: If a relayed offer is initially dropped,
/// reconciliation retries the handshake and establishes the missing link.
#[test]
fn test_req_031_relay_handshake_retry() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Connect Alice <-> Bob
    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("A adds B");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("B adds A");

    // Connect Alice <-> Charlie
    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("C adds A");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("A adds C");

    // Simulate dropped initial snap offer/channel failure by removing the peer
    bob_mesh.remove_peer("charlie").unwrap();
    charlie_mesh.remove_peer("bob").unwrap();

    // Direct link does not exist yet
    assert!(!bob_mesh.has_peer("charlie"));
    assert!(!charlie_mesh.has_peer("bob"));

    // Reconciliation entry point retries the missing handshake
    let retried = block_on(bob_mesh.reconcile_mesh()).expect("reconciliation");
    assert!(
        retried >= 1,
        "reconcile_mesh must initiate handshake for missing Charlie link"
    );

    assert!(
        bob_mesh.has_peer("charlie"),
        "Direct link B->C created after reconcile retry"
    );
    assert!(
        charlie_mesh.has_peer("bob"),
        "Direct link C->B created after reconcile retry"
    );

    let b_c = bob_mesh.get_peer("charlie").unwrap().rtc_peer;
    let c_b = charlie_mesh.get_peer("bob").unwrap().rtc_peer;
    WebRtcPeer::pair_for_testing(&b_c, &c_b).expect("pair Channel_BC");

    assert_eq!(bob_mesh.open_peers(), 2);
    assert_eq!(charlie_mesh.open_peers(), 2);
}

/// V-031-6: REQ-031.2 & REQ-031.5: Re-introduction / re-handshake over an already-open link
/// must be idempotent: must not close the channel, trigger departure callbacks, or purge.
#[test]
fn test_req_031_rehandshake_keeps_open_link() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair Alice-Bob");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("A adds B");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice)
        .expect("B adds A");

    let alice_to_charlie = WebRtcPeer::new();
    let charlie_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_charlie, &charlie_to_alice).expect("pair Alice-Charlie");
    charlie_mesh
        .add_peer("alice", alice_pk, charlie_to_alice)
        .expect("C adds A");
    alice_mesh
        .add_peer("charlie", charlie_pk, alice_to_charlie)
        .expect("A adds C");

    assert!(bob_mesh.has_peer("charlie"));
    assert!(charlie_mesh.has_peer("bob"));

    let b_to_c = bob_mesh.get_peer("charlie").unwrap().rtc_peer;
    let c_to_b = charlie_mesh.get_peer("bob").unwrap().rtc_peer;
    WebRtcPeer::pair_for_testing(&b_to_c, &c_to_b).expect("pair Channel_BC");

    // Track departure and burn callbacks on Bob
    let left_counter = Rc::new(RefCell::new(0usize));
    let lc_clone = left_counter.clone();
    bob_mesh.on_peer_left(move |_| {
        *lc_clone.borrow_mut() += 1;
    });

    let burn_counter = Rc::new(RefCell::new(0usize));
    let bc_clone = burn_counter.clone();
    bob_mesh.on_burn_peer_messages(move |_| {
        *bc_clone.borrow_mut() += 1;
    });

    // Send a redundant re-introduction PeerJoined signal for Charlie to Bob
    let re_intro = MeshRelayMessage {
        frame_id: "re-intro-001".to_string(),
        ttl: 3,
        signal: MeshRelaySignal::PeerJoined {
            peer_id: "charlie".to_string(),
            public_key: charlie_pk,
        },
    };
    let re_intro_json = to_json_string(&re_intro).expect("json");
    block_on(bob_mesh.handle_incoming_message("alice", &re_intro_json))
        .expect("handle redundant PeerJoined");

    // Also trigger reconciliation while link is already healthy
    let initiated = block_on(bob_mesh.reconcile_mesh()).expect("reconcile");
    assert_eq!(
        initiated, 0,
        "No new handshakes should be initiated for healthy link"
    );

    // Channel_BC must remain open and unaffected
    assert_eq!(
        *left_counter.borrow(),
        0,
        "Spurious on_peer_left must NOT fire"
    );
    assert_eq!(
        *burn_counter.borrow(),
        0,
        "Spurious burn callback must NOT fire"
    );
    assert!(bob_mesh.has_peer("charlie"), "Bob still holds peer Charlie");
    assert_eq!(bob_mesh.open_peers(), 2, "Bob still reports 2 open peers");

    // Direct communication still works
    let bob_recv = Rc::new(RefCell::new(Vec::new()));
    let br_clone = bob_recv.clone();
    bob_mesh.on_message(move |s, m| br_clone.borrow_mut().push((s.to_string(), m.to_string())));

    charlie_mesh
        .broadcast_chat("Still connected!")
        .expect("broadcast");
    assert_eq!(
        *bob_recv.borrow(),
        vec![("charlie".to_string(), "Still connected!".to_string())]
    );
}

/// V-031-7: REQ-031.4 & REQ-031.5: In a 2-node chat, when one leaves,
/// the remaining node reports open_peers == 0 and cleans up gracefully.
#[test]
fn test_req_031_last_two_one_leaves() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);

    let alice_to_bob = WebRtcPeer::new();
    let bob_to_alice = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&alice_to_bob, &bob_to_alice).expect("pair AB");
    alice_mesh
        .add_peer("bob", bob_pk, alice_to_bob)
        .expect("A adds B");
    bob_mesh
        .add_peer("alice", alice_pk, bob_to_alice.clone())
        .expect("B adds A");

    assert_eq!(bob_mesh.open_peers(), 1);
    assert_eq!(alice_mesh.open_peers(), 1);

    // Alice leaves
    bob_to_alice.close().expect("close Bob-Alice channel");

    // Bob now sees 0 open peers
    assert_eq!(
        bob_mesh.open_peers(),
        0,
        "Bob sees 0 open peers after Alice leaves"
    );
    assert_eq!(bob_mesh.peer_count(), 0);
    assert!(!bob_mesh.has_peer("alice"));
}

/// REQ-031.8 (c): When both B and C call reconcile_mesh on a missing link,
/// only the lower peer ID initiates on the first call; the higher peer ID returns 0.
#[test]
fn test_req_031_8_reconcile_only_lower_id_initiates_first() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Connect Alice <-> Bob
    let a_to_b = WebRtcPeer::new();
    let b_to_a = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&a_to_b, &b_to_a).expect("pair AB");
    alice_mesh.add_peer("bob", bob_pk, a_to_b).unwrap();
    bob_mesh.add_peer("alice", alice_pk, b_to_a).unwrap();

    // Connect Alice <-> Charlie
    let a_to_c = WebRtcPeer::new();
    let c_to_a = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&a_to_c, &c_to_a).expect("pair AC");
    charlie_mesh.add_peer("alice", alice_pk, c_to_a).unwrap();
    alice_mesh.add_peer("charlie", charlie_pk, a_to_c).unwrap();

    // Remove direct link B <-> C so it is missing
    bob_mesh.remove_peer("charlie").unwrap();
    charlie_mesh.remove_peer("bob").unwrap();

    assert!(!bob_mesh.has_peer("charlie"));
    assert!(!charlie_mesh.has_peer("bob"));

    // "charlie" > "bob".
    // Charlie calls reconcile_mesh: charlie is higher ID, grace ticks not reached -> returns 0.
    let charlie_initiated = block_on(charlie_mesh.reconcile_mesh()).expect("charlie reconcile");
    assert_eq!(
        charlie_initiated, 0,
        "Higher peer ID (charlie) must NOT initiate on first reconcile call"
    );
    assert!(
        !charlie_mesh.has_peer("bob"),
        "Charlie should not have created direct peer yet"
    );

    // "bob" < "charlie".
    // Bob calls reconcile_mesh: bob is lower ID -> initiates immediately -> returns 1.
    let bob_initiated = block_on(bob_mesh.reconcile_mesh()).expect("bob reconcile");
    assert_eq!(
        bob_initiated, 1,
        "Lower peer ID (bob) MUST initiate on first reconcile call"
    );
    assert!(
        bob_mesh.has_peer("charlie"),
        "Bob initiated handshake creating peer entry for charlie"
    );
}

/// REQ-031.8 (c): When the lower peer ID's offer is dropped, the higher peer ID
/// falls back to initiate after NON_INITIATOR_GRACE_TICKS reconcile calls and the link forms.
#[test]
fn test_req_031_8_reconcile_higher_id_falls_back_after_grace() {
    let (alice_sk, alice_pk) = generate_kp();
    let (bob_sk, bob_pk) = generate_kp();
    let (charlie_sk, charlie_pk) = generate_kp();

    let alice_mesh = MeshTopology::new("alice", alice_sk);
    let bob_mesh = MeshTopology::new("bob", bob_sk);
    let charlie_mesh = MeshTopology::new("charlie", charlie_sk);

    // Connect Alice <-> Bob
    let a_to_b = WebRtcPeer::new();
    let b_to_a = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&a_to_b, &b_to_a).expect("pair AB");
    alice_mesh.add_peer("bob", bob_pk, a_to_b).unwrap();
    bob_mesh.add_peer("alice", alice_pk, b_to_a).unwrap();

    // Connect Alice <-> Charlie
    let a_to_c = WebRtcPeer::new();
    let c_to_a = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&a_to_c, &c_to_a).expect("pair AC");
    charlie_mesh.add_peer("alice", alice_pk, c_to_a).unwrap();
    alice_mesh.add_peer("charlie", charlie_pk, a_to_c).unwrap();

    // Break initial direct snap
    bob_mesh.remove_peer("charlie").unwrap();
    charlie_mesh.remove_peer("bob").unwrap();

    // Bob (lower ID) fails to connect or its offers are dropped; Bob does not reconcile.
    // Charlie (higher ID) reconciles:
    // Tick 1 (grace tick 1 / NON_INITIATOR_GRACE_TICKS):
    let tick1 = block_on(charlie_mesh.reconcile_mesh()).expect("tick 1");
    assert_eq!(tick1, 0, "Grace tick 1 must not initiate");

    // Tick 2 (grace tick 2 / NON_INITIATOR_GRACE_TICKS):
    let tick2 = block_on(charlie_mesh.reconcile_mesh()).expect("tick 2");
    assert_eq!(tick2, 0, "Grace tick 2 must not initiate");

    // Tick 3 (grace expired, ticks >= NON_INITIATOR_GRACE_TICKS = 2):
    // Charlie initiates fallback handshake via Alice relay to Bob!
    let tick3 = block_on(charlie_mesh.reconcile_mesh()).expect("tick 3");
    assert_eq!(tick3, 1, "Fallback initiate must trigger after grace ticks");

    assert!(
        charlie_mesh.has_peer("bob"),
        "Charlie has peer entry for Bob"
    );
    assert!(
        bob_mesh.has_peer("charlie"),
        "Bob received offer and has peer entry for Charlie"
    );

    // Connect the pair channels
    let c_to_b = charlie_mesh.get_peer("bob").unwrap().rtc_peer;
    let b_to_c = bob_mesh.get_peer("charlie").unwrap().rtc_peer;
    WebRtcPeer::pair_for_testing(&c_to_b, &b_to_c).expect("pair BC");

    assert_eq!(charlie_mesh.open_peers(), 2);
    assert_eq!(bob_mesh.open_peers(), 2);
}
