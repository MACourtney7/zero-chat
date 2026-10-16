use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use zero_client::{
    bundle_sdp_with_candidates, extract_ice_candidates, validate_sdp_answer, validate_sdp_offer,
    DataChannelConfig, DataChannelMessage, PeerLifecycleState, WebRtcError, WebRtcPeer,
    DEFAULT_DATA_CHANNEL_LABEL,
};

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

#[test]
fn test_req_002_2_offer_generation_and_data_channel_config() {
    let alice = WebRtcPeer::new();
    assert_eq!(alice.state(), PeerLifecycleState::Connecting);
    assert_eq!(
        alice.config(),
        DataChannelConfig {
            label: DEFAULT_DATA_CHANNEL_LABEL.to_string(),
            ordered: true,
        }
    );

    let observed_states = Rc::new(RefCell::new(Vec::new()));
    let states_clone = observed_states.clone();
    alice.on_state_change(move |state| {
        states_clone.borrow_mut().push(state);
    });

    let offer = block_on(alice.create_offer()).expect("Alice should generate valid SDP offer");
    assert_eq!(alice.state(), PeerLifecycleState::Connecting);
    assert_eq!(
        *observed_states.borrow(),
        vec![PeerLifecycleState::Connecting]
    );
    assert_eq!(alice.local_offer().as_ref(), Some(&offer));

    // Verify SDP offer structure and bundled ICE candidates
    assert!(offer.sdp.contains("v=0\r\n"));
    assert!(offer
        .sdp
        .contains("m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n"));
    assert!(offer
        .sdp
        .contains("a=dcmap:0 label=\"zero-chat\" ordered=true\r\n"));
    assert!(offer.sdp.contains("a=candidate:"));
    assert!(offer.sdp.ends_with("a=end-of-candidates\r\n"));

    let candidates = extract_ice_candidates(&offer.sdp);
    assert!(
        !candidates.is_empty(),
        "SDP offer must contain at least one bundled ICE candidate"
    );
    assert_eq!(validate_sdp_offer(&offer.sdp), Ok(offer));
}

#[test]
fn test_req_002_3_accept_offer_and_apply_answer_handshake() {
    let alice = WebRtcPeer::new();
    let bob = WebRtcPeer::new();

    let offer = block_on(alice.create_offer()).expect("Alice offer generation");
    let answer = block_on(bob.accept_offer_and_create_answer(&offer.sdp))
        .expect("Bob should accept offer and create answer");

    assert_eq!(bob.remote_offer().as_ref(), Some(&offer));
    assert_eq!(bob.local_answer().as_ref(), Some(&answer));
    assert!(answer.sdp.contains("v=0\r\n"));
    assert!(answer
        .sdp
        .contains("m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n"));
    assert!(answer.sdp.contains("a=setup:active\r\n"));
    assert!(answer.sdp.contains("a=candidate:"));
    assert_eq!(validate_sdp_answer(&answer.sdp), Ok(answer.clone()));

    block_on(alice.apply_answer(&answer.sdp)).expect("Alice should apply Bob's SDP answer cleanly");
    assert_eq!(alice.remote_answer().as_ref(), Some(&answer));
    assert_eq!(alice.state(), PeerLifecycleState::Open);
}

#[test]
fn test_req_002_3_malformed_sdp_rejection() {
    let peer = WebRtcPeer::new();

    // Empty offer
    assert!(matches!(
        block_on(peer.accept_offer_and_create_answer("")),
        Err(WebRtcError::InvalidSdp(_))
    ));

    // Missing m=application
    assert!(matches!(
        block_on(peer.accept_offer_and_create_answer("v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\n")),
        Err(WebRtcError::InvalidSdp(_))
    ));

    // Missing bundled ICE candidates in offer
    let offer_without_candidates =
        "v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n";
    assert!(matches!(
        block_on(peer.accept_offer_and_create_answer(offer_without_candidates)),
        Err(WebRtcError::InvalidSdp(_))
    ));

    // Invalid answer applied to Alice
    assert!(matches!(
        block_on(peer.apply_answer("invalid-sdp-payload")),
        Err(WebRtcError::InvalidSdp(_))
    ));
}

#[test]
fn test_req_002_4_bidirectional_text_and_binary_messaging() {
    let alice = WebRtcPeer::new();
    let bob = WebRtcPeer::new();

    let offer = block_on(alice.create_offer()).expect("Alice offer");
    let answer = block_on(bob.accept_offer_and_create_answer(&offer.sdp)).expect("Bob answer");
    block_on(alice.apply_answer(&answer.sdp)).expect("Alice apply answer");
    WebRtcPeer::pair_for_testing(&alice, &bob).expect("Pair Alice and Bob in-memory");

    assert_eq!(alice.state(), PeerLifecycleState::Open);
    assert_eq!(bob.state(), PeerLifecycleState::Open);

    let alice_received_text = Rc::new(RefCell::new(Vec::<String>::new()));
    let bob_received_text = Rc::new(RefCell::new(Vec::<String>::new()));
    let alice_received_bin = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
    let bob_received_bin = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));

    {
        let bob_text = bob_received_text.clone();
        bob.on_text_message(move |msg| {
            bob_text.borrow_mut().push(msg.to_string());
        });
    }
    {
        let alice_text = alice_received_text.clone();
        alice.on_text_message(move |msg| {
            alice_text.borrow_mut().push(msg.to_string());
        });
    }
    {
        let bob_bin = bob_received_bin.clone();
        bob.on_binary_message(move |bytes| {
            bob_bin.borrow_mut().push(bytes.to_vec());
        });
    }
    {
        let alice_bin = alice_received_bin.clone();
        alice.on_binary_message(move |bytes| {
            alice_bin.borrow_mut().push(bytes.to_vec());
        });
    }

    // Alice -> Bob UTF-8 text
    alice
        .send_text("hello from Alice")
        .expect("Alice send_text");
    assert_eq!(*bob_received_text.borrow(), vec!["hello from Alice"]);

    // Bob -> Alice UTF-8 text
    bob.send_text("hello from Bob").expect("Bob send_text");
    assert_eq!(*alice_received_text.borrow(), vec!["hello from Bob"]);

    // Alice -> Bob binary packet
    let binary_packet_a = vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0xff];
    alice
        .send_binary(&binary_packet_a)
        .expect("Alice send_binary");
    assert_eq!(*bob_received_bin.borrow(), vec![binary_packet_a.clone()]);

    // Bob -> Alice binary packet
    let binary_packet_b = vec![0x01, 0x02, 0x03, 0x04, 0xfe];
    bob.send_binary(&binary_packet_b).expect("Bob send_binary");
    assert_eq!(*alice_received_bin.borrow(), vec![binary_packet_b.clone()]);

    assert_eq!(
        alice.outbound_messages(),
        vec![
            DataChannelMessage::Text("hello from Alice".to_string()),
            DataChannelMessage::Binary(binary_packet_a),
        ]
    );
    assert_eq!(
        bob.outbound_messages(),
        vec![
            DataChannelMessage::Text("hello from Bob".to_string()),
            DataChannelMessage::Binary(binary_packet_b),
        ]
    );
}

#[test]
fn test_lifecycle_state_machine_and_error_guards() {
    let peer = WebRtcPeer::new();

    // Sending before Open must fail with ChannelNotOpen(Connecting)
    assert_eq!(
        peer.send_text("too early"),
        Err(WebRtcError::ChannelNotOpen(PeerLifecycleState::Connecting))
    );
    assert_eq!(
        peer.send_binary(&[1, 2, 3]),
        Err(WebRtcError::ChannelNotOpen(PeerLifecycleState::Connecting))
    );

    let transitions = Rc::new(RefCell::new(Vec::new()));
    let transitions_clone = transitions.clone();
    peer.on_state_change(move |state| {
        transitions_clone.borrow_mut().push(state);
    });

    let remote = WebRtcPeer::new();
    WebRtcPeer::pair_for_testing(&peer, &remote).expect("Pair transitions to Open");
    assert_eq!(peer.state(), PeerLifecycleState::Open);

    peer.close()
        .expect("Close transitions through Closing to Closed");
    assert_eq!(peer.state(), PeerLifecycleState::Closed);
    assert_eq!(
        *transitions.borrow(),
        vec![
            PeerLifecycleState::Open,
            PeerLifecycleState::Closing,
            PeerLifecycleState::Closed,
        ]
    );

    // Sending after Closed must fail with ChannelNotOpen(Closed)
    assert_eq!(
        peer.send_text("after close"),
        Err(WebRtcError::ChannelNotOpen(PeerLifecycleState::Closed))
    );

    // Verify invalid state transition rejection directly on PeerLifecycleState
    let mut state = PeerLifecycleState::Closed;
    assert_eq!(
        state.transition_to(PeerLifecycleState::Open),
        Err(WebRtcError::InvalidStateTransition {
            from: PeerLifecycleState::Closed,
            to: PeerLifecycleState::Open,
        })
    );
}

#[test]
fn test_bundle_sdp_with_candidates_deduplication_and_crlf() {
    let raw_sdp = "v=0\no=- 1 1 IN IP4 127.0.0.1\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\na=candidate:1 1 UDP 2130706431 127.0.0.1 50000 typ host\n";
    let extra_candidates = vec![
        "a=candidate:1 1 UDP 2130706431 127.0.0.1 50000 typ host".to_string(),
        "candidate:2 1 UDP 1694498815 192.168.1.10 50001 typ srflx".to_string(),
    ];

    let bundled =
        bundle_sdp_with_candidates(raw_sdp, &extra_candidates).expect("Bundle candidates");

    // Every line must be CRLF-terminated
    for line in bundled.split_inclusive('\n') {
        assert!(
            line.ends_with("\r\n"),
            "Expected CRLF line ending, got: {line:?}"
        );
    }

    let extracted = extract_ice_candidates(&bundled);
    assert_eq!(
        extracted,
        vec![
            "a=candidate:1 1 UDP 2130706431 127.0.0.1 50000 typ host".to_string(),
            "a=candidate:2 1 UDP 1694498815 192.168.1.10 50001 typ srflx".to_string(),
        ]
    );
    assert!(bundled.ends_with("a=end-of-candidates\r\n"));
}
