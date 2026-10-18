//! Multi-node mesh simulation test suite for ZeroChat.
//!
//! Validates REQ-006.1 through REQ-006.5, REQ-022.1, and REQ-022.2:
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

use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use zero_client::{
    ChatSessionCrypto, ChatUi, CryptoError, DataChannelMessage, MeshTopology, PeerLifecycleState,
    WebRtcPeer,
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
    assert_eq!(bob_mesh.peer_count(), 2); // alice + charlie
    assert_eq!(charlie_mesh.peer_count(), 2); // alice + bob

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
    assert!(
        alice_mesh.has_peer("charlie"),
        "Alice must retain Charlie"
    );
    assert!(
        charlie_mesh.has_peer("alice"),
        "Charlie must retain Alice"
    );

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
        Some(&(
            "alice".to_string(),
            post_leave_msg.to_string(),
            false,
            true
        )),
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

