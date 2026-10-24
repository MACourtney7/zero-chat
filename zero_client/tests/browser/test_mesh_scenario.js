// Headless Test Harness for Browser Multi-Peer WebRTC Mesh
// Validates V-031-1 through V-031-7 and REQ-031.1 through REQ-031.6

function assert(condition, message) {
    if (!condition) {
        throw new Error("Assertion failed: " + message);
    }
}

function delay(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

async function runTests() {
    print("===============================================================================");
    print("Running Browser Full-Mesh Headless Regression Suite (V-031-1 .. V-031-7, REQ-031.8a, REQ-031.8b)");
    print("===============================================================================");

    // Test 1: V-031-1: 3-Client Mesh Formation (A-B joined, A invites C, B-C direct link forms)
    print("\n[Test 1] V-031-1: Full mesh establishment with 3 peers (A, B, C)...");
    {
        const engineA = new ZeroWebRtcEngine("peer-A");
        const engineB = new ZeroWebRtcEngine("peer-B");
        const engineC = new ZeroWebRtcEngine("peer-C");

        // Helper to pipe signals between engines via callbacks
        function wireEngine(eng, otherEngines) {
            eng.onMessageCb = async (text, incomingPeerId) => {
                let data = null;
                try { data = JSON.parse(text); } catch (_) {}
                if (!data || !data.signal) return;
                const signal = data.signal;

                // RelaySdpOffer
                if (signal.RelaySdpOffer) {
                    const { from_peer_id, to_peer_id, sealed_offer } = signal.RelaySdpOffer;
                    const myId = eng.localPeerId;
                    if (to_peer_id === myId) {
                        const offerSdp = new TextDecoder().decode(new Uint8Array(sealed_offer.ciphertext));
                        const answerSdp = await eng.create_peer_answer(from_peer_id, offerSdp);
                        const nonce = Array.from(crypto.getRandomValues(new Uint8Array(12)));
                        const ciphertext = Array.from(new TextEncoder().encode(answerSdp));
                        const answerMsg = {
                            frame_id: "ans-1",
                            ttl: 3,
                            signal: {
                                RelaySdpAnswer: {
                                    from_peer_id: myId,
                                    to_peer_id: from_peer_id,
                                    sealed_answer: { nonce, ciphertext }
                                }
                            }
                        };
                        eng.send_text(JSON.stringify(answerMsg), incomingPeerId);
                    } else {
                        // Forward
                        if (data.ttl > 1) {
                            const fwd = { ...data, ttl: data.ttl - 1 };
                            eng.send_text(JSON.stringify(fwd), to_peer_id);
                        }
                    }
                }
                // RelaySdpAnswer
                if (signal.RelaySdpAnswer) {
                    const { from_peer_id, to_peer_id, sealed_answer } = signal.RelaySdpAnswer;
                    const myId = eng.localPeerId;
                    if (to_peer_id === myId) {
                        const answerSdp = new TextDecoder().decode(new Uint8Array(sealed_answer.ciphertext));
                        await eng.accept_peer_answer(from_peer_id, answerSdp);
                    } else {
                        // Forward
                        if (data.ttl > 1) {
                            const fwd = { ...data, ttl: data.ttl - 1 };
                            eng.send_text(JSON.stringify(fwd), to_peer_id);
                        }
                    }
                }
                // PeerJoined
                if (signal.PeerJoined) {
                    const remotePeerId = signal.PeerJoined.peer_id;
                    const myId = eng.localPeerId;
                    if (remotePeerId !== myId) {
                        const peerObj = eng.peers.get(remotePeerId);
                        const isOpen = peerObj && peerObj.state === "open" && peerObj.dc && peerObj.dc.readyState === "open";
                        const isConn = peerObj && peerObj.state === "connecting";
                        if (!isOpen && !isConn && myId < remotePeerId) {
                            await eng.initiate_relay_handshake(incomingPeerId, remotePeerId);
                        }
                    }
                }
                // BroadcastChat
                if (signal.BroadcastChat) {
                    eng.receivedChats = eng.receivedChats || [];
                    const body = new TextDecoder().decode(new Uint8Array(signal.BroadcastChat.sealed_message.ciphertext));
                    eng.receivedChats.push({ from: incomingPeerId, text: body });
                }
            };

            eng.onPeerStateChangeCb = (peerId, state) => {
                if (state === "open") {
                    // Universal introduction
                    for (const [existingId, p] of eng.peers.entries()) {
                        if (existingId !== peerId && p.state === "open" && p.dc && p.dc.readyState === "open") {
                            const msgToNew = {
                                frame_id: "intro-1",
                                ttl: 3,
                                signal: { PeerJoined: { peer_id: existingId, public_key: new Array(32).fill(0) } }
                            };
                            const msgToExisting = {
                                frame_id: "intro-2",
                                ttl: 3,
                                signal: { PeerJoined: { peer_id: peerId, public_key: new Array(32).fill(0) } }
                            };
                            eng.send_text(JSON.stringify(msgToNew), peerId);
                            eng.send_text(JSON.stringify(msgToExisting), existingId);
                        }
                    }
                }
            };
        }

        wireEngine(engineA, [engineB, engineC]);
        wireEngine(engineB, [engineA, engineC]);
        wireEngine(engineC, [engineA, engineB]);

        // Step 1: Connect A <-> B
        const offerAB = await engineA.create_peer_offer("peer-B");
        const answerAB = await engineB.create_peer_answer("peer-A", offerAB);
        await engineA.accept_peer_answer("peer-B", answerAB);

        await delay(50);
        assert(engineA.openPeerCount === 1, "Engine A must have 1 open peer (B)");
        assert(engineB.openPeerCount === 1, "Engine B must have 1 open peer (A)");

        // Step 2: Connect A <-> C
        const offerAC = await engineA.create_peer_offer("peer-C");
        const answerAC = await engineC.create_peer_answer("peer-A", offerAC);
        await engineA.accept_peer_answer("peer-C", answerAC);

        // Allow in-band relay handshake between B and C via A
        await delay(100);

        print(`  Engine A open peers: ${engineA.openPeerCount}`);
        print(`  Engine B open peers: ${engineB.openPeerCount}`);
        print(`  Engine C open peers: ${engineC.openPeerCount}`);

        assert(engineA.openPeerCount === 2, "A must hold direct links to B and C");
        assert(engineB.openPeerCount === 2, "B must hold direct links to A and C");
        assert(engineC.openPeerCount === 2, "C must hold direct links to A and B");

        // Verify direct B <-> C channel exists and is open
        const bToC = engineB.peers.get("peer-C");
        assert(bToC && bToC.state === "open" && bToC.dc.readyState === "open", "B must have direct open DataChannel to C");
        const cToB = engineC.peers.get("peer-B");
        assert(cToB && cToB.state === "open" && cToB.dc.readyState === "open", "C must have direct open DataChannel to B");
        print("  PASS: V-031-1: Full mesh successfully established (all 3 peers linked directly).");

        // Test 2: V-031-2: A leaves; C sends; B receives directly
        print("\n[Test 2] V-031-2: Host A departs; C broadcasts; B receives directly...");
        engineA.close();
        await delay(50);

        assert(engineB.openPeerCount === 1, "B must retain 1 open peer (C)");
        assert(engineC.openPeerCount === 1, "C must retain 1 open peer (B)");

        // C sends chat message
        const chatText = "Hello B, this is C after A left!";
        const nonce = Array.from(crypto.getRandomValues(new Uint8Array(12)));
        const ciphertext = Array.from(new TextEncoder().encode(chatText));
        const broadcastMsg = {
            frame_id: "msg-c-1",
            ttl: 3,
            signal: {
                BroadcastChat: {
                    message_uuid: "uuid-c-1",
                    origin_peer_id: "peer-C",
                    ttl: 3,
                    sealed_message: { nonce, ciphertext }
                }
            }
        };

        engineC.broadcast_text(JSON.stringify(broadcastMsg));
        await delay(50);

        assert(engineB.receivedChats && engineB.receivedChats.length === 1, "B must receive chat message directly from C");
        assert(engineB.receivedChats[0].text === chatText, "Received text must match sent text");
        print("  PASS: V-031-2: B directly received C's message after A departed.");

        // Test 3: V-031-7: Remaining two clients; one leaves; other transitions to 0 open peers
        print("\n[Test 3] V-031-7: C departs; B reflects 0 open peers...");
        engineC.close();
        await delay(50);

        assert(engineB.openPeerCount === 0, "B must have exactly 0 open peers after last peer leaves");
        print("  PASS: V-031-7: Truthful 0 peer count when last participant departs.");
    }

    // Test 4: V-031-6: Idempotent snap / re-handshake over open link does not destroy link
    print("\n[Test 4] V-031-6: Idempotency check - duplicate offer/answer does not kill open link...");
    {
        const engine1 = new ZeroWebRtcEngine("peer-1");
        const engine2 = new ZeroWebRtcEngine("peer-2");

        const offer1 = await engine1.create_peer_offer("peer-2");
        const answer1 = await engine2.create_peer_answer("peer-1", offer1);
        await engine1.accept_peer_answer("peer-2", answer1);
        await delay(50);

        assert(engine1.openPeerCount === 1, "Link must be open");

        // Attempt second offer on existing open link
        let closedCallbackFired = false;
        engine1.onPeerStateChangeCb = (id, st) => {
            if (st === "closed") closedCallbackFired = true;
        };

        const reOffer = await engine1.create_peer_offer("peer-2");
        assert(!closedCallbackFired, "Re-offering over open link must NOT fire 'closed' callback (REQ-031.2)");
        assert(engine1.openPeerCount === 1, "Channel must remain open after idempotent re-offer");

        const reAnswer = await engine2.create_peer_answer("peer-1", reOffer || offer1);
        assert(engine2.openPeerCount === 1, "Channel on remote side must remain open after duplicate offer");
        print("  PASS: V-031-6: Duplicate handshake is completely idempotent and non-destructive.");
    }

    // Test 5: V-031-5: Reconciliation loop retries when initial relay handshake dropped
    print("\n[Test 5] V-031-5: Mesh reconciliation timer recovers dropped handshake...");
    {
        const engineA = new ZeroWebRtcEngine("peer-A");
        const engineB = new ZeroWebRtcEngine("peer-B");
        const engineC = new ZeroWebRtcEngine("peer-C");

        // Connect A <-> B and A <-> C
        const oAB = await engineA.create_peer_offer("peer-B");
        const aAB = await engineB.create_peer_answer("peer-A", oAB);
        await engineA.accept_peer_answer("peer-B", aAB);

        const oAC = await engineA.create_peer_offer("peer-C");
        const aAC = await engineC.create_peer_answer("peer-A", oAC);
        await engineA.accept_peer_answer("peer-C", aAC);
        await delay(50);

        // Simulate B knowing about C via PeerJoined, but initial handshake message was lost
        let relayOfferDelivered = false;

        engineB.onMessageCb = async (text, incId) => {
            const data = JSON.parse(text);
            if (data.signal && data.signal.RelaySdpAnswer) {
                const sdp = new TextDecoder().decode(new Uint8Array(data.signal.RelaySdpAnswer.sealed_answer.ciphertext));
                await engineB.accept_peer_answer("peer-C", sdp);
            }
        };

        engineC.onMessageCb = async (text, incId) => {
            const data = JSON.parse(text);
            if (data.signal && data.signal.RelaySdpOffer) {
                relayOfferDelivered = true;
                const sdp = new TextDecoder().decode(new Uint8Array(data.signal.RelaySdpOffer.sealed_offer.ciphertext));
                const ansSdp = await engineC.create_peer_answer("peer-B", sdp);
                const nonce = Array.from(crypto.getRandomValues(new Uint8Array(12)));
                const ciphertext = Array.from(new TextEncoder().encode(ansSdp));
                const ansMsg = {
                    frame_id: "ans-rec",
                    ttl: 3,
                    signal: {
                        RelaySdpAnswer: {
                            from_peer_id: "peer-C",
                            to_peer_id: "peer-B",
                            sealed_answer: { nonce, ciphertext }
                        }
                    }
                };
                // Deliver back to B
                engineA.send_text(JSON.stringify(ansMsg), "peer-B");
            }
        };

        engineA.onMessageCb = async (text, incId) => {
            const data = JSON.parse(text);
            if (data.signal && data.signal.RelaySdpOffer) {
                // Forward offer to C
                engineA.send_text(text, "peer-C");
            }
            if (data.signal && data.signal.RelaySdpAnswer) {
                // Forward answer to B
                engineA.send_text(text, "peer-B");
            }
        };

        assert(engineB.openPeerCount === 1, "B has only A initially");
        assert(engineC.openPeerCount === 1, "C has only A initially");

        // Trigger reconciliation by initiating relay handshake
        await engineB.initiate_relay_handshake("peer-A", "peer-C");
        await delay(100);

        assert(relayOfferDelivered, "Relay offer must be delivered through A to C");
        assert(engineB.openPeerCount === 2, "B must have 2 open peers after reconciliation");
        assert(engineC.openPeerCount === 2, "C must have 2 open peers after reconciliation");
        print("  PASS: V-031-5: Dropped handshake recovered; direct B-C link established.");
    }

    // Test 6: REQ-031.6: Send reliability feedback when 0 open channels exist
    print("\n[Test 6] REQ-031.6: Send feedback - 0 peers shows error system message and halts send...");
    {
        const engineIsolated = new ZeroWebRtcEngine("peer-solo");
        engine = engineIsolated;
        window.engine = engineIsolated;

        feed.children = [];
        msgInput.value = "Unsent test message";

        assert(engineIsolated.openPeerCount === 0, "No peers connected");
        doSendMessage();

        const lastChild = feed.children[feed.children.length - 1];
        const lastMsg = lastChild && lastChild.children && lastChild.children.find(c => c.className === "system-text");
        const systemText = lastMsg ? lastMsg.textContent : (lastChild ? lastChild.textContent : "");

        assert(systemText === "Message not sent: no connected peers",
            "System message must alert user that message was not sent due to no connected peers, got: " + systemText);
        assert(msgInput.value === "Unsent test message",
            "User's input must be preserved when delivery fails");
        print("  PASS: REQ-031.6: Attempting to send with 0 peers displays clear error feedback and preserves input.");
    }

    // Test 7: REQ-031.5: Truthful #peer-count UI display
    print("\n[Test 7] REQ-031.5: Truthful peer counter deriving strictly from open DataChannels...");
    {
        const engineTruth = new ZeroWebRtcEngine("peer-truth");
        engine = engineTruth;
        window.engine = engineTruth;
        const countEl = document.getElementById("peer-count");

        updatePeerCountDisplay();
        assert(countEl.textContent === "0", "Peer count element must display '0' when 0 open channels, got: " + countEl.textContent);

        // Add dummy entry to activeSessionPeers to verify it doesn't mask 0 channels
        activeSessionPeers.add("phantom-peer-1");
        activeSessionPeers.add("phantom-peer-2");
        updatePeerCountDisplay();
        assert(countEl.textContent === "0", "activeSessionPeers must NOT inflate #peer-count when openPeerCount is 0, got: " + countEl.textContent);

        print("  PASS: REQ-031.5: Counter strictly mirrors active open channels.");
    }

    // Test 8: REQ-031.8a: Crossed-offer glare resolution between simultaneous initiators
    print("\n[Test 8] REQ-031.8a: Crossed-offer glare resolution between simultaneous initiators...");
    {
        const engineA = new ZeroWebRtcEngine("peer-A");
        const engineB = new ZeroWebRtcEngine("peer-B");
        const engineC = new ZeroWebRtcEngine("peer-C");

        // Helper to forward signals across engines via their respective handleEngineMessage
        function wireEngines(engMap) {
            for (const [id, eng] of engMap.entries()) {
                eng.onMessageCb = async (text, incomingPeerId) => {
                    await handleEngineMessage(text, incomingPeerId, eng);
                };
            }
        }

        const engineMap = new Map([
            ["peer-A", engineA],
            ["peer-B", engineB],
            ["peer-C", engineC]
        ]);
        wireEngines(engineMap);

        // Connect A <-> B
        const offerAB = await engineA.create_peer_offer("peer-B");
        const answerAB = await engineB.create_peer_answer("peer-A", offerAB);
        await engineA.accept_peer_answer("peer-B", answerAB);

        // Connect A <-> C
        const offerAC = await engineA.create_peer_offer("peer-C");
        const answerAC = await engineC.create_peer_answer("peer-A", offerAC);
        await engineA.accept_peer_answer("peer-C", answerAC);

        await delay(50);
        assert(engineA.openPeerCount === 2, "A is connected to B and C");
        assert(engineB.openPeerCount === 1, "B has 1 open peer (A)");
        assert(engineC.openPeerCount === 1, "C has 1 open peer (A)");

        // Now both B and C simultaneously initiate relay handshake to each other via A!
        // Crossed RelaySdpOffer messages:
        const bHandshake = engineB.initiate_relay_handshake("peer-A", "peer-C");
        const cHandshake = engineC.initiate_relay_handshake("peer-A", "peer-B");
        await Promise.all([bHandshake, cHandshake]);

        // Simulated time budget (must resolve without waiting for 8s watchdog)
        await delay(150);

        // Assert direct B <-> C channel exists and is open
        assert(engineB.openPeerCount === 2, "Engine B must hold 2 open peers (A and C) after glare resolution");
        assert(engineC.openPeerCount === 2, "Engine C must hold 2 open peers (A and B) after glare resolution");

        const bToC = engineB.peers.get("peer-C");
        assert(bToC && bToC.state === "open" && bToC.dc && bToC.dc.readyState === "open", "B must have direct open DataChannel to C");
        const cToB = engineC.peers.get("peer-B");
        assert(cToB && cToB.state === "open" && cToB.dc && cToB.dc.readyState === "open", "C must have direct open DataChannel to B");

        // Assert that the lower-ID side's offer won:
        // peer-B < peer-C: B's outgoing offer was preserved (B's data channel created locally)
        assert(bToC.dc.remotePeerConnection === bToC.pc, "B (lower peer ID) must be the offering initiator");

        print("  PASS: REQ-031.8a: Crossed offers resolved deterministically; direct link established without 8s watchdog.");
    }

    // Test 9: REQ-031.8b: Watchdog cleans stalled connecting peer when zero open peers remain
    print("\n[Test 9] REQ-031.8b: Watchdog cleans stalled connecting peer when zero open peers remain...");
    {
        const engineWatchdog = new ZeroWebRtcEngine("peer-B");
        engine = engineWatchdog;
        window.engine = engineWatchdog;
        hasActiveSession = true;
        isBurned = false;
        peerDepartureHandled = false;
        feed.children = [];
        departedPeers.clear();
        knownSessionMembers.clear();
        activeSessionPeers.clear();
        memberHandshakeState.clear();
        pendingOffers.clear();

        // 1. Establish initial open peer A (relay)
        const pcA = new RTCPeerConnection();
        const dcA = pcA.createDataChannel("zero-chat");
        dcA.readyState = "open";
        engineWatchdog.peers.set("peer-A", { pc: pcA, dc: dcA, state: "open", peerId: "peer-A", createdAt: Date.now() });
        activeSessionPeers.add("peer-A");
        knownSessionMembers.add("peer-A");
        assert(engineWatchdog.openPeerCount === 1, "B has 1 open peer (A)");

        // 2. Add an in-flight connecting peer C
        const startTime = Date.now();
        const pcC = new RTCPeerConnection();
        const dcC = pcC.createDataChannel("zero-chat");
        dcC.readyState = "connecting";
        engineWatchdog.peers.set("peer-C", { pc: pcC, dc: dcC, state: "connecting", peerId: "peer-C", createdAt: startTime });
        pendingOffers.add("peer-C");
        knownSessionMembers.add("peer-C");
        memberHandshakeState.set("peer-C", {
            firstSeen: startTime,
            attempts: 1,
            lastAttempt: startTime,
            failedSurfaced: false
        });
        startReconciliationTimer();

        // 3. Relay peer A departs while B-C is still connecting with zero other open peers
        engineWatchdog.close_peer("peer-A");
        handlePeerDeparture("peer-A");

        assert(engineWatchdog.openPeerCount === 0, "B must have 0 open peers after A departs");
        assert(peerDepartureHandled === false, "UI must NOT transition to disconnected yet while peer-C is connecting");
        assert(msgInput.disabled === false, "Input must remain enabled while connection is pending");

        // Verify exactly 1 'Peer left chat' notice was appended for peer A
        let leftNotices = feed.children.filter(c => {
            const textChild = c.children && c.children.find(ch => ch.className === "system-text");
            const txt = textChild ? textChild.textContent : (c.textContent || "");
            return txt === "Peer left chat";
        });
        assert(leftNotices.length === 1, "Must have exactly 1 'Peer left chat' notice for peer A");

        // 4. Advance fake time past 8 seconds and invoke reconcileMesh()
        const origDateNow = Date.now;
        try {
            Date.now = () => startTime + 9000;
            await reconcileMesh();
        } finally {
            Date.now = origDateNow;
        }

        // 5. Assertions: connecting peer C cleaned, UI transitions to disconnected once
        assert(!engineWatchdog.peers.has("peer-C"), "Stalled connecting peer C must be cleaned from engine.peers");
        assert(!pendingOffers.has("peer-C"), "peer-C must be removed from pendingOffers");
        assert(peerDepartureHandled === true, "peerDepartureHandled must be true");
        assert(msgInput.disabled === true, "msgInput must be disabled in disconnected state");
        assert(msgInput.placeholder === "", "msgInput placeholder must be empty on peer departure");
        assert(reconciliationTimer === null, "Reconciliation timer must be stopped");

        // Verify NO duplicate 'Peer left chat' notice was appended
        leftNotices = feed.children.filter(c => {
            const textChild = c.children && c.children.find(ch => ch.className === "system-text");
            const txt = textChild ? textChild.textContent : (c.textContent || "");
            return txt === "Peer left chat";
        });
        assert(leftNotices.length === 1, `Must still have exactly 1 'Peer left chat' notice (no duplicates), found ${leftNotices.length}`);

        print("  PASS: REQ-031.8b: Stalled connecting peer cleaned; UI transitions to disconnected cleanly without duplicate notices.");
    }

    // Test 10: REQ-032.4: Scoped clearPendingInvite protects in-flight invite from secondary peer connection
    print("\n[Test 10] REQ-032.4: Scoped clearPendingInvite protects in-flight invite from secondary peer connection...");
    {
        const testHostEngine = new ZeroWebRtcEngine("peer-host-test");
        engine = testHostEngine;
        window.engine = testHostEngine;
        hasActiveSession = true;
        isBurned = false;

        // Clear any prior pending invite
        clearPendingInvite();

        // 1. Host initiates invite for a new peer
        await handleHeaderAddPeerClick();

        const pendingId = getPendingInvitePeerId();
        const pendingWs = getPendingInviteWs();
        assert(pendingId !== null, "pendingInvitePeerId must be populated");
        assert(pendingWs !== null, "pendingInviteWs must be open");

        // 2. An unrelated secondary peer connects / transitions to open
        clearPendingInvite("peer-unrelated");

        // Must NOT clear pending invite
        assert(getPendingInvitePeerId() === pendingId, "Unrelated peer must NOT clear pendingInvitePeerId");
        assert(getPendingInviteWs() === pendingWs, "Unrelated peer must NOT close pendingInviteWs");

        // 3. The invited peer connects / transitions to open
        clearPendingInvite(pendingId);

        // Must be cleared
        assert(getPendingInvitePeerId() === null, "Matching invited peer ID must clear pendingInvitePeerId");
        assert(getPendingInviteWs() === null, "Matching invited peer ID must close pendingInviteWs");

        print("  PASS: REQ-032.4: Scoped clearPendingInvite properly ignores unrelated peers.");
    }

    // Test 11: REQ-032.1, REQ-032.3, REQ-032.5: Pre-assigned peer IDs in invite payloads and clean host departure
    print("\n[Test 11] REQ-032.1, REQ-032.3, REQ-032.5: Pre-assigned peer IDs and clean host departure in 3-peer mesh...");
    {
        feed.children = [];
        departedPeers.clear();
        knownSessionMembers.clear();
        activeSessionPeers.clear();
        memberHandshakeState.clear();
        pendingOffers.clear();
        announcedJoinPeers.clear();
        hasActiveSession = true;
        isBurned = false;
        peerDepartureHandled = false;

        const hostId = "peer-c1-host";
        const assignedPeerId_2 = "peer-c2-joiner";
        const assignedPeerId_3 = "peer-c3-joiner";

        const engine1 = new ZeroWebRtcEngine(hostId);
        const engine2 = new ZeroWebRtcEngine(assignedPeerId_2);
        const engine3 = new ZeroWebRtcEngine(assignedPeerId_3);

        function wireEngines(engMap) {
            for (const [id, eng] of engMap.entries()) {
                eng.onMessageCb = async (text, incomingPeerId) => {
                    await handleEngineMessage(text, incomingPeerId, eng);
                };
                eng.onPeerStateChangeCb = (pId, st) => {
                    if (st === "open") {
                        // Universal introduction
                        for (const [existingId, p] of eng.peers.entries()) {
                            if (existingId !== pId && p.state === "open" && p.dc && p.dc.readyState === "open") {
                                const msgToNew = {
                                    frame_id: "intro-" + Math.random(),
                                    ttl: 3,
                                    signal: { PeerJoined: { peer_id: existingId, public_key: new Array(32).fill(0) } }
                                };
                                const msgToExisting = {
                                    frame_id: "intro-" + Math.random(),
                                    ttl: 3,
                                    signal: { PeerJoined: { peer_id: pId, public_key: new Array(32).fill(0) } }
                                };
                                try { eng.send_text(JSON.stringify(msgToNew), pId); } catch (_) {}
                                try { eng.send_text(JSON.stringify(msgToExisting), existingId); } catch (_) {}
                            }
                        }
                    }
                };
            }
        }

        const engineMap = new Map([
            [hostId, engine1],
            [assignedPeerId_2, engine2],
            [assignedPeerId_3, engine3]
        ]);
        wireEngines(engineMap);

        // Step 1: Client 1 creates room, Client 2 joins with assigned_peer_id_2
        const offer1_2 = await engine1.create_peer_offer(assignedPeerId_2);
        const invitePayload2 = {
            do_id: "room-mesh-32",
            invite_uuid: "uuid-2",
            host_peer_id: hostId,
            assigned_peer_id: assignedPeerId_2,
            sdp_offer: { sdp: offer1_2 }
        };
        assert(invitePayload2.assigned_peer_id === assignedPeerId_2, "Invite payload must contain assigned_peer_id");
        assert(invitePayload2.host_peer_id === hostId, "Invite payload must contain host_peer_id");

        // Client 2 adopts assigned ID and creates answer
        const client2AdoptedId = invitePayload2.assigned_peer_id;
        assert(client2AdoptedId === assignedPeerId_2, "Client 2 must adopt assigned_peer_id_2");
        const answer2 = await engine2.accept_offer_and_create_answer(invitePayload2.sdp_offer.sdp, invitePayload2.host_peer_id);
        const dropPayload2 = {
            invite_uuid: invitePayload2.invite_uuid,
            sdp_answer: { sdp: answer2 },
            joiner_peer_id: client2AdoptedId
        };
        await engine1.apply_answer(dropPayload2.sdp_answer.sdp, dropPayload2.joiner_peer_id);
        await delay(50);

        assert(engine1.openPeerCount === 1, "Client 1 must have 1 open peer (Client 2)");
        assert(engine2.openPeerCount === 1, "Client 2 must have 1 open peer (Client 1)");
        assert(engine1.peers.has(assignedPeerId_2), "Client 1 must hold peer under assigned ID without rename drift");

        // Step 2: Client 1 creates invite with assigned_peer_id_3, Client 3 joins with assigned_peer_id_3
        const offer1_3 = await engine1.create_peer_offer(assignedPeerId_3);
        const invitePayload3 = {
            do_id: "room-mesh-32",
            invite_uuid: "uuid-3",
            host_peer_id: hostId,
            assigned_peer_id: assignedPeerId_3,
            sdp_offer: { sdp: offer1_3 }
        };
        assert(invitePayload3.assigned_peer_id === assignedPeerId_3, "Invite payload must contain assigned_peer_id_3");

        const client3AdoptedId = invitePayload3.assigned_peer_id;
        assert(client3AdoptedId === assignedPeerId_3, "Client 3 must adopt assigned_peer_id_3");
        const answer3 = await engine3.accept_offer_and_create_answer(invitePayload3.sdp_offer.sdp, invitePayload3.host_peer_id);
        const dropPayload3 = {
            invite_uuid: invitePayload3.invite_uuid,
            sdp_answer: { sdp: answer3 },
            joiner_peer_id: client3AdoptedId
        };
        await engine1.apply_answer(dropPayload3.sdp_answer.sdp, dropPayload3.joiner_peer_id);

        // Allow direct relay handshake between Client 2 and Client 3 via Client 1
        await delay(150);

        // Step 3: All 3 clients report #peer-count === 2
        assert(engine1.openPeerCount === 2, "Client 1 must have 2 open peers");
        assert(engine2.openPeerCount === 2, "Client 2 must have 2 open peers (Client 1 and Client 3)");
        assert(engine3.openPeerCount === 2, "Client 3 must have 2 open peers (Client 1 and Client 2)");

        const countEl = document.getElementById("peer-count");
        engine = engine1; updatePeerCountDisplay();
        assert(countEl.textContent === "2", "Client 1 #peer-count must display '2'");
        engine = engine2; updatePeerCountDisplay();
        assert(countEl.textContent === "2", "Client 2 #peer-count must display '2'");
        engine = engine3; updatePeerCountDisplay();
        assert(countEl.textContent === "2", "Client 3 #peer-count must display '2'");

        // Verify pairwise direct DataChannels exist between Client 2 and Client 3
        const c2ToC3 = engine2.peers.get(assignedPeerId_3);
        assert(c2ToC3 && c2ToC3.state === "open" && c2ToC3.dc.readyState === "open", "Direct channel 2->3 must be open");
        const c3ToC2 = engine3.peers.get(assignedPeerId_2);
        assert(c3ToC2 && c3ToC2.state === "open" && c3ToC2.dc.readyState === "open", "Direct channel 3->2 must be open");

        // Step 4: Client 1 departs
        // Client 2 is the active global UI client for feed testing
        engine = engine2;
        window.engine = engine2;
        feed.children = [];
        departedPeers.clear();

        const leaveMsg = {
            frame_id: "leave-host-c1",
            ttl: 3,
            signal: {
                PeerLeftAndBurn: {
                    peer_id: hostId
                }
            }
        };
        const leaveJson = JSON.stringify(leaveMsg);
        engine1.broadcast_text(leaveJson);
        engine1.close();
        await delay(50);

        // Client 2 and Client 3 handle leave message
        await handleEngineMessage(leaveJson, hostId, engine2);
        await handleEngineMessage(leaveJson, hostId, engine3);

        // Step 5: Client 2 and Client 3 report #peer-count === 1
        assert(engine2.openPeerCount === 1, "Client 2 must have 1 open peer (Client 3)");
        assert(engine3.openPeerCount === 1, "Client 3 must have 1 open peer (Client 2)");

        engine = engine2; updatePeerCountDisplay();
        assert(countEl.textContent === "1", "Client 2 #peer-count must display '1'");
        engine = engine3; updatePeerCountDisplay();
        assert(countEl.textContent === "1", "Client 3 #peer-count must display '1'");

        // Step 6: Verify exact single "Peer left chat" notice emitted on Client 2
        let c2LeftNotices = feed.children.filter(c => {
            const textChild = c.children && c.children.find(ch => ch.className === "system-text");
            const txt = textChild ? textChild.textContent : (c.textContent || "");
            return txt === "Peer left chat";
        });
        assert(c2LeftNotices.length === 1, `Exact single 'Peer left chat' notice emitted on Client 2, got ${c2LeftNotices.length}`);

        // Duplicate leave signal does not trigger duplicate notice
        await handleEngineMessage(leaveJson, hostId, engine2);
        c2LeftNotices = feed.children.filter(c => {
            const textChild = c.children && c.children.find(ch => ch.className === "system-text");
            const txt = textChild ? textChild.textContent : (c.textContent || "");
            return txt === "Peer left chat";
        });
        assert(c2LeftNotices.length === 1, `Exact single 'Peer left chat' notice preserved after duplicate leave signal, got ${c2LeftNotices.length}`);

        // Step 7: Client 2 sends a message -> Client 3 receives and displays it
        const chatText = "Hello Client 3 from Client 2 after host departed!";
        const nonce = Array.from(crypto.getRandomValues(new Uint8Array(12)));
        const ciphertext = Array.from(new TextEncoder().encode(chatText));
        const chatUuid = "chat-uuid-" + Math.random().toString(36).slice(2, 8);
        const broadcastMsg = {
            frame_id: chatUuid,
            ttl: 3,
            signal: {
                BroadcastChat: {
                    message_uuid: chatUuid,
                    origin_peer_id: assignedPeerId_2,
                    ttl: 3,
                    sealed_message: { nonce, ciphertext }
                }
            }
        };

        // Switch active global UI to Client 3 to verify Client 3's feed receives and displays message
        engine = engine3;
        window.engine = engine3;
        feed.children = [];

        engine2.broadcast_text(JSON.stringify(broadcastMsg));
        await delay(50);

        const bubbles = feed.children.filter(c => c.classList && c.classList.contains("message-bubble"));
        assert(bubbles.length >= 1, "Client 3 feed must contain received message bubble");
        const lastBubble = bubbles[bubbles.length - 1];
        const bubbleTextEl = lastBubble.children && lastBubble.children.find(c => c.className === "message-text");
        const receivedText = bubbleTextEl ? bubbleTextEl.textContent : "";
        assert(receivedText === chatText, `Client 3 must display sent text '${chatText}', got '${receivedText}'`);

        print("  PASS: REQ-032.1, REQ-032.3, REQ-032.5: Pre-assigned peer IDs and host departure verified cleanly.");
    }

    // Test 12: REQ-032.1 E2E: createRoomAsHost and joinRoomFromHash via signaling exchange
    print("\n[Test 12] REQ-032.1 E2E: createRoomAsHost and joinRoomFromHash via signaling exchange...");
    {
        // Reset state
        feed.children = [];
        departedPeers.clear();
        knownSessionMembers.clear();
        activeSessionPeers.clear();
        memberHandshakeState.clear();
        pendingOffers.clear();
        announcedJoinPeers.clear();
        clearPendingInvite();

        // 1. Host creates room
        activeHostInviteUrl = null;
        setLocalPeerId("peer-e2e-host");
        await createRoomAsHost();

        const inviteUrl = getActiveHostInviteUrl();
        assert(inviteUrl !== null, "Host must generate activeHostInviteUrl");
        assert(inviteUrl.includes("#payload="), "Invite URL must include #payload=");

        // Extract payload
        const match = inviteUrl.match(/payload=([^&]+)/);
        assert(match !== null, "Payload match must succeed");
        const jsonStr = atob(match[1].replace(/-/g, "+").replace(/_/g, "/"));
        const invitePayload = JSON.parse(jsonStr);

        assert(invitePayload.host_peer_id === "peer-e2e-host", "Invite payload host_peer_id must be host's localPeerId");
        assert(typeof invitePayload.assigned_peer_id === "string" && invitePayload.assigned_peer_id.startsWith("peer-"),
            "Invite payload must include pre-assigned canonical assigned_peer_id");

        const hostEngine = engine;
        assert(hostEngine.peers.has(invitePayload.assigned_peer_id),
            "Host engine must create offer directly under assigned_peer_id");

        // 2. Joiner joins room from hash
        // In this simulated join, joiner creates its answer and POSTs to drop endpoint
        const joinerEngine = new ZeroWebRtcEngine(invitePayload.assigned_peer_id);
        const joinerAnswer = await joinerEngine.accept_offer_and_create_answer(invitePayload.sdp_offer.sdp, invitePayload.host_peer_id);

        const dropPayload = {
            invite_uuid: invitePayload.invite_uuid,
            token: invitePayload.token,
            sdp_answer: { sdp: joinerAnswer },
            joiner_public_key: Array.from(crypto.getRandomValues(new Uint8Array(32))),
            joiner_peer_id: invitePayload.assigned_peer_id,
            peer_id: invitePayload.assigned_peer_id
        };

        const dropUrl = `https://network.zero-chat.workers.dev/drop/${invitePayload.do_id}/${invitePayload.invite_uuid}`;
        await fetch(dropUrl, {
            method: "POST",
            body: JSON.stringify(dropPayload)
        });

        // Allow WebSocket delivery and WebRTC channel snap
        await delay(100);

        assert(hostEngine.openPeerCount === 1, "Host must have 1 open peer after signaling drop");
        assert(joinerEngine.openPeerCount === 1, "Joiner must have 1 open peer after signaling drop");
        assert(hostEngine.peers.has(invitePayload.assigned_peer_id),
            "Host peers map must retain exact pre-assigned peer ID without rename or identity drift");

        // Cleanup
        hostEngine.close();
        joinerEngine.close();
        clearPendingInvite();

        print("  PASS: REQ-032.1 E2E: createRoomAsHost and joinRoomFromHash completed without peer identity drift.");
    }

    // Test 13: REQ-033.1 .. REQ-033.4: Simplified Icon-Only Bottom Button & Temporary System Message Lifecycle
    print("\n[Test 13] REQ-033.1 - REQ-033.4: Icon-only button and temporary system messages lifecycle...");
    {
        // 1. Reset to home and verify Stage 1 initial state (REQ-033.1, V-033.1)
        resetToHome();
        assert(btnCreateRoom !== null, "btnCreateRoom must exist");
        assert(btnCreateRoom.classList.contains("btn-emerald"), "Stage 1: button must have btn-emerald class");
        assert(!btnCreateRoom.classList.contains("btn-waiting"), "Stage 1: button must NOT have btn-waiting class");
        assert(btnCreateRoom.getAttribute("aria-label") === "Create Chat", "Stage 1: button aria-label must be 'Create Chat'");
        assert(btnCreateRoom.textContent.trim() === "", `Stage 1: button text must be empty, got '${btnCreateRoom.textContent}'`);
        assert(btnCreateRoom.querySelector("span") === null, "Stage 1: button must not contain text spans");
        assert(btnCreateRoom.innerHTML.includes("<svg") && btnCreateRoom.innerHTML.includes("<line"),
            "Stage 1: button must display speech bubble + icon SVG");

        // 2. Click to Stage 2 & Temporary Messages (REQ-033.1, REQ-033.2, V-033.1, V-033.2)
        setLocalPeerId("peer-ui-host");
        await createRoomAsHost();
        await delay(500);

        assert(btnCreateRoom.classList.contains("btn-waiting"), "Stage 2: button must transition to btn-waiting class");
        assert(!btnCreateRoom.classList.contains("btn-emerald"), "Stage 2: button must NOT have btn-emerald class");
        assert(btnCreateRoom.getAttribute("aria-label") === "Copy invite link", "Stage 2: button aria-label must be 'Copy invite link'");
        assert(btnCreateRoom.textContent.trim() === "", `Stage 2: button text must be empty, got '${btnCreateRoom.textContent}'`);
        assert(btnCreateRoom.querySelector("span") === null, "Stage 2: button must not contain text spans");
        assert(btnCreateRoom.innerHTML.includes("<rect"), "Stage 2: button must display overlapping rectangles copy icon SVG");

        // Verify temporary system messages in feed
        const tempMessages = feed.querySelectorAll(".system-temporary");
        assert(tempMessages.length === 2, `Feed must contain exactly 2 temporary system messages, got ${tempMessages.length}`);

        // Message 1: "Invite link copied to clipboard"
        const msg1 = tempMessages[0];
        const msg1Text = msg1.querySelector(".system-text") ? msg1.querySelector(".system-text").textContent : "";
        assert(msg1Text === "Invite link copied to clipboard", `First temp message must be 'Invite link copied to clipboard', got '${msg1Text}'`);
        assert(msg1.querySelector(".system-timestamp") === null, "Temporary message 1 must NOT contain timestamp");

        // Message 2: "Waiting for peer" with ticking dots (REQ-034.3)
        const msg2 = tempMessages[1];
        const msg2Text = msg2.querySelector(".system-text") ? msg2.querySelector(".system-text").textContent : "";
        assert(msg2Text === "Waiting for peer", `Second temp message must be 'Waiting for peer', got '${msg2Text}'`);
        assert(msg2.querySelector(".system-timestamp") === null, "Temporary message 2 must NOT contain timestamp");
        assert(msg2.querySelector(".ticking-dots") !== null, "Second temp message must contain .ticking-dots element");

        // Verify permanent "Secure chat created" is NOT emitted
        const allSystemMessages = feed.querySelectorAll(".system-message");
        const secureChatEmitted = Array.from(allSystemMessages).some(m => {
            const t = m.querySelector(".system-text");
            return t && t.textContent.includes("Secure chat created");
        });
        assert(!secureChatEmitted, "'Secure chat created' permanent message must NOT be emitted");

        // 3. Re-click in Stage 2 clears and re-displays temporary messages (REQ-033.2, V-033.3)
        await createRoomAsHost();
        await delay(500);
        assert(btnCreateRoom.classList.contains("btn-waiting"), "Re-click: button must remain in Stage 2");
        assert(btnCreateRoom.textContent.trim() === "", "Re-click: button must remain icon-only");

        const tempMessagesAfterReclick = feed.querySelectorAll(".system-temporary");
        assert(tempMessagesAfterReclick.length === 2,
            `Re-clicking bottom button must clear and re-display exactly 2 temporary messages, got ${tempMessagesAfterReclick.length}`);

        // 4. Permanent system message clears all active temporary messages (REQ-033.2, V-033.4)
        appendSystemMessage("Peer joined chat");
        const remainingTemps = feed.querySelectorAll(".system-temporary");
        assert(remainingTemps.length === 0, `Permanent message must clear all temporary messages, remaining: ${remainingTemps.length}`);

        const permanentMessages = feed.querySelectorAll(".system-message");
        assert(permanentMessages.length === 1, "Only permanent message should remain in feed");
        assert(permanentMessages[0].querySelector(".system-timestamp") === null, "Permanent message must NOT include timestamp");
        assert(permanentMessages[0].querySelector(".system-text").textContent === "Peer joined chat",
            "Permanent message text must be 'Peer joined chat'");

        // 5. Header [+] button 5-second self-clearing notice (REQ-033.3, V-033.5)
        setHasActiveSession(true);
        const headerBtnBefore = headerAddBtn.className;
        const headerHtmlBefore = headerAddBtn.innerHTML;

        await handleHeaderAddPeerClick();

        // Verify [+] button itself is completely untouched
        assert(headerAddBtn.className === headerBtnBefore, "Header [+] button classes must NOT change");
        assert(headerAddBtn.innerHTML === headerHtmlBefore, "Header [+] button innerHTML must NOT change");

        // Verify temporary notice appeared in feed with .notice-pulse and no timestamp
        const headerNotices = feed.querySelectorAll(".header-add-notice");
        assert(headerNotices.length === 1, "Feed must contain 1 header-add-notice temporary element");
        const noticeEl = headerNotices[0];
        assert(noticeEl.classList.contains("system-temporary"), "Header notice must have .system-temporary class");
        assert(noticeEl.classList.contains("notice-pulse"), "Header notice must have .notice-pulse class");
        assert(noticeEl.querySelector(".system-timestamp") === null, "Header notice must NOT contain timestamp");
        assert(noticeEl.querySelector(".system-text").textContent === "Invite link copied to clipboard",
            "Header notice text must be 'Invite link copied to clipboard'");
        assert(getHeaderAddFeedbackTimer() !== null, "5-second timer must be active");

        // Re-click within 5s window restarts timer and re-triggers pulse without duplicating notice
        await delay(50);
        await handleHeaderAddPeerClick();

        const headerNoticesAfterReclick = feed.querySelectorAll(".header-add-notice");
        assert(headerNoticesAfterReclick.length === 1, "Re-clicking [+] must NOT duplicate the notice element");
        assert(headerNoticesAfterReclick[0].classList.contains("notice-pulse"), "Notice must re-trigger .notice-pulse");
        assert(getHeaderAddFeedbackTimer() !== null, "5-second timer must be restarted");

        // Wait for 5s timer to expire and verify notice self-clears
        await delay(5100);
        const expiredNotices = feed.querySelectorAll(".header-add-notice");
        assert(expiredNotices.length === 0, "Header temporary notice must self-clear after 5,000ms");
        assert(getHeaderAddFeedbackTimer() === null, "Header timer must be cleared");

        // 6. Reset to Home restores Stage 1
        resetToHome();
        assert(btnCreateRoom.classList.contains("btn-emerald"), "After reset: button must return to btn-emerald");
        assert(!btnCreateRoom.classList.contains("btn-waiting"), "After reset: button must NOT have btn-waiting");
        assert(btnCreateRoom.getAttribute("aria-label") === "Create Chat", "After reset: aria-label must be 'Create Chat'");
        assert(btnCreateRoom.textContent.trim() === "", "After reset: text must remain empty");

        print("  PASS: REQ-033.1 .. REQ-033.4: Icon-only button and temporary system messages verified cleanly.");
    }

    // Test 14: REQ-034.1 - REQ-034.4: Staggered delivery, ellipsis fix, slashed person icon, empty placeholder
    print("\n[Test 14] REQ-034.1 - REQ-034.4: Slashed person icon, empty placeholder, staggered notices & ellipsis fix...");
    {
        // 1. Assert #peer-count in home view contains slashed person icon SVG
        resetToHome();
        const peerCountEl = document.getElementById("peer-count");
        assert(peerCountEl !== null, "#peer-count element must exist");
        assert(peerCountEl.innerHTML.includes("peer-slashed-user-icon") && peerCountEl.innerHTML.includes("<svg"),
            "Home View: #peer-count must contain slashed person icon SVG");
        assert(msgInput.placeholder === "", "Home View: msgInput placeholder must be empty string");

        // 2. Trigger create room: assert #peer-count transitions to '0'
        setLocalPeerId("peer-ui-t14");
        await createRoomAsHost();

        assert(peerCountEl.textContent === "0",
            `Room View (empty session): #peer-count must transition to '0', got '${peerCountEl.textContent}'`);

        // Assert Notice 1 appended immediately
        let tempMessages = feed.querySelectorAll(".system-temporary");
        assert(tempMessages.length === 1,
            `Immediately after create room, exactly 1 temporary notice must exist, got ${tempMessages.length}`);
        const notice1Text = tempMessages[0].querySelector(".system-text") ? tempMessages[0].querySelector(".system-text").textContent : "";
        assert(notice1Text === "Invite link copied to clipboard",
            `Notice 1 must be 'Invite link copied to clipboard', got '${notice1Text}'`);

        // Assert Notice 2 is NOT yet appended immediately (staggered by 450ms)
        assert(tempMessages.length < 2,
            "Notice 2 ('Waiting for peer') must NOT be appended immediately");

        // Advance timers / wait 500ms
        await delay(500);

        // Assert Notice 2 is now appended
        tempMessages = feed.querySelectorAll(".system-temporary");
        assert(tempMessages.length === 2,
            `After 500ms delay, exactly 2 temporary notices must exist, got ${tempMessages.length}`);
        const notice2 = tempMessages[1];
        const notice2Text = notice2.querySelector(".system-text") ? notice2.querySelector(".system-text").textContent : "";
        assert(notice2Text === "Waiting for peer",
            `Notice 2 .system-text must strictly be 'Waiting for peer' without literal dots, got '${notice2Text}'`);
        assert(notice2.querySelector(".ticking-dots") !== null,
            "Notice 2 must contain .ticking-dots element");
        assert(window.getComputedStyle(notice2).gap === "0px" || window.getComputedStyle(notice2).gap === "0",
            "Notice 2 (.system-temporary) must have gap: 0 to eliminate ellipsis gap");
        assert(window.getComputedStyle(notice2.querySelector(".ticking-dots")).marginLeft === "0px" || window.getComputedStyle(notice2.querySelector(".ticking-dots")).marginLeft === "0",
            ".ticking-dots must have marginLeft: 0");

        // 3. Re-click button: assert timer reset and staggered sequence restarts cleanly
        await createRoomAsHost();
        tempMessages = feed.querySelectorAll(".system-temporary");
        assert(tempMessages.length === 1,
            `Immediately after re-clicking create room button, only notice 1 should exist, got ${tempMessages.length}`);

        await delay(500);
        tempMessages = feed.querySelectorAll(".system-temporary");
        assert(tempMessages.length === 2,
            `After 500ms following re-click, notice 2 must be appended, got ${tempMessages.length}`);
        assert(tempMessages[1].querySelector(".system-text").textContent === "Waiting for peer",
            "Notice 2 text after re-click must be 'Waiting for peer'");

        // 4. Test stagger timer cancellation on clearTemporarySystemMessages
        await createRoomAsHost();
        assert(feed.querySelectorAll(".system-temporary").length === 1, "Immediate notice 1 before clear");
        clearTemporarySystemMessages();
        assert(feed.querySelectorAll(".system-temporary").length === 0, "Cleared temp notices immediately");
        await delay(500);
        assert(feed.querySelectorAll(".system-temporary").length === 0,
            "Staggered timer must be cancelled by clearTemporarySystemMessages; no notice should appear after 500ms");

        // 5. Simulate active session and peer departure
        setHasActiveSession(true);
        handlePeerDeparture("departing-peer-t14");
        assert(msgInput.disabled === true, "msgInput must be disabled on peer departure");
        assert(msgInput.placeholder === "", `msgInput placeholder must be empty on peer departure, got '${msgInput.placeholder}'`);
        assert(peerCountEl.textContent === "0", "#peer-count must remain '0' in room view when peer departs");

        // 6. Simulate resetToHome()
        resetToHome();
        assert(peerCountEl.innerHTML.includes("peer-slashed-user-icon") && peerCountEl.innerHTML.includes("<svg"),
            "resetToHome() must restore slashed person icon SVG to #peer-count");
        assert(msgInput.placeholder === "", "msgInput placeholder must remain empty after resetToHome()");

        print("  PASS: REQ-034.1 - REQ-034.4: Slashed person icon, empty placeholder, staggered notices & ellipsis fix verified.");
    }

    // Test 15: REQ-035.1 - REQ-035.5: Enhanced Burn Behavior (90s TTL, aging fade, per-message burn, minimal dissolve, quiet room)
    print("\n[Test 15] REQ-035.1 - REQ-035.5: Enhanced Burn Behavior suite...");
    {
        // 1. Verify CSS styles for .message-bubble, .btn-message-burn, .message-aging, .message-dissolve
        function readCssFile() {
            if (typeof read === "function") {
                try { return read("zero_client/style.css"); } catch (_) {}
            }
            if (typeof require !== "undefined") {
                try { return require("fs").readFileSync("zero_client/style.css", "utf-8"); } catch (_) {}
            }
            return "";
        }
        const cssContent = readCssFile();
        if (cssContent) {
            assert(cssContent.includes(".message-bubble") && cssContent.includes("min-width: 135px;") && cssContent.includes("min-height: 44px;"),
                "style.css: .message-bubble must define min-width: 135px and min-height: 44px");
            assert(cssContent.includes(".btn-message-burn"),
                "style.css: must define .btn-message-burn");
            assert(cssContent.includes(".message-aging") && cssContent.includes("opacity: 0.8 !important;") && cssContent.includes("transition: opacity 20s linear !important;"),
                "style.css: .message-aging must define 0.8 opacity and 20s linear transition");
            assert(cssContent.includes(".message-dissolve") && cssContent.includes("opacity: 0 !important;") && cssContent.includes("scale(0.98)"),
                "style.css: .message-dissolve must define 0 opacity and scale(0.98)");
        }

        resetToHome();
        setIsInRoomSession(true);

        // 2. Verify .btn-message-burn rendered in both outgoing and incoming message bubbles with min-size
        const outBubble = appendMessageBubble("You", "Short", true, "self", "test-msg-out-035");
        assert(outBubble !== null, "Outgoing bubble must be created");
        assert(outBubble.getAttribute("data-message-id") === "test-msg-out-035", "Outgoing bubble must have data-message-id");
        const outBurnBtn = outBubble.querySelector(".btn-message-burn");
        assert(outBurnBtn !== null, "Outgoing bubble must contain .btn-message-burn");
        assert(outBurnBtn.getAttribute("aria-label") === "Burn message", "Burn button must have aria-label='Burn message'");
        assert(outBurnBtn.querySelector("svg") !== null, "Burn button must contain flame SVG icon");

        const computedStyle = getComputedStyle(outBubble);
        assert(computedStyle.minWidth === "135px", `Bubble minWidth must be 135px, got ${computedStyle.minWidth}`);
        assert(computedStyle.minHeight === "44px", `Bubble minHeight must be 44px, got ${computedStyle.minHeight}`);

        const inBubble = appendMessageBubble("Peer", "Hello from peer", false, "peer-B", "test-msg-in-035");
        assert(inBubble !== null, "Incoming bubble must be created");
        assert(inBubble.getAttribute("data-message-id") === "test-msg-in-035", "Incoming bubble must have data-message-id");
        const inBurnBtn = inBubble.querySelector(".btn-message-burn");
        assert(inBurnBtn !== null, "Incoming bubble must contain .btn-message-burn");
        assert(inBurnBtn.getAttribute("aria-label") === "Burn message", "Burn button on incoming bubble must have aria-label='Burn message'");

        // 3. Verify local click on burn button triggers .message-dissolve and removes bubble after 200ms
        assert(!outBubble.classList.contains("message-dissolve"), "Before click: outBubble must not have .message-dissolve");
        outBurnBtn.click();
        assert(outBubble.classList.contains("message-dissolve"), "Immediately after click: outBubble must have .message-dissolve class");

        advanceTimersByTime(200);
        await delay(50);
        assert(outBubble.parentNode === null, "200ms after click: outBubble must be removed from the DOM");

        // 4. Verify 3-peer distributed burn: Peer A has bubble, Peer B broadcasts BurnMessage -> Peer A dissolves
        const distBubble = appendMessageBubble("Peer", "Distributed message test", false, "peer-B", "dist-burn-uuid-123");
        assert(feed.querySelector('.message-bubble[data-message-id="dist-burn-uuid-123"]') !== null,
            "distBubble must exist in feed before distributed burn");

        const mockEngine = new ZeroWebRtcEngine("peer-A");
        let broadcastedFrame = null;
        mockEngine.broadcast_text = function(text) {
            broadcastedFrame = JSON.parse(text);
        };

        const burnSignalData = {
            frame_id: "burn-frame-001",
            ttl: 3,
            signal: {
                BurnMessage: {
                    message_uuid: "dist-burn-uuid-123"
                }
            }
        };

        await handleEngineMessage(JSON.stringify(burnSignalData), "peer-B", mockEngine);

        assert(distBubble.classList.contains("message-dissolve"),
            "On BurnMessage receipt: target bubble must receive .message-dissolve class");
        assert(broadcastedFrame !== null, "Engine must forward BurnMessage frame with decremented TTL");
        assert(broadcastedFrame.ttl === 2, `Forwarded BurnMessage must have ttl=2, got ${broadcastedFrame.ttl}`);
        assert(broadcastedFrame.signal && broadcastedFrame.signal.BurnMessage.message_uuid === "dist-burn-uuid-123",
            "Forwarded frame must contain BurnMessage with matching uuid");

        advanceTimersByTime(200);
        await delay(50);
        assert(distBubble.parentNode === null, "200ms after BurnMessage: distBubble must be removed from DOM");

        // 5. Verify 70s aging transition (.message-aging) and 90s auto-dissolve for chat bubbles and system messages
        const agingChat = appendMessageBubble("Peer", "Aging chat message", false, "peer-C", "aging-chat-001");
        const agingSystem = appendSystemMessage("Aging system notice");

        assert(!agingChat.classList.contains("message-aging"), "At 0s: chat bubble must not have .message-aging");
        assert(!agingSystem.classList.contains("message-aging"), "At 0s: system message must not have .message-aging");
        assert(!agingChat.classList.contains("message-dissolve"), "At 0s: chat bubble must not have .message-dissolve");

        // Advance 70 seconds: aging transition to 0.8 opacity starts
        advanceTimersByTime(70000);
        assert(agingChat.classList.contains("message-aging"), "At 70s: chat bubble must have .message-aging");
        assert(agingSystem.classList.contains("message-aging"), "At 70s: system message must have .message-aging");
        assert(!agingChat.classList.contains("message-dissolve"), "At 70s: chat bubble must not yet have .message-dissolve");

        // Advance 20 more seconds (90s total): triggers soft dissolve
        advanceTimersByTime(20000);
        assert(agingChat.classList.contains("message-dissolve"), "At 90s: chat bubble must have .message-dissolve");
        assert(agingSystem.classList.contains("message-dissolve"), "At 90s: system message must have .message-dissolve");

        // Advance 200ms: element removal
        advanceTimersByTime(200);
        await delay(50);
        assert(agingChat.parentNode === null, "At 90.2s: agingChat must be removed from DOM");
        assert(agingSystem.parentNode === null, "At 90.2s: agingSystem must be removed from DOM");

        // 6. Verify quiet room invariant: when all messages in an active room dissolve, #welcome-splash remains hidden
        assert(Boolean(isInRoomSession) === true, "Must be in active room session");
        assert(feed.querySelectorAll(".message-bubble, .system-message").length === 0,
            "Feed must be completely empty after messages dissolve");

        showWelcomeSplash();
        const splashEl = document.getElementById("welcome-splash");
        if (splashEl) {
            assert(splashEl.classList.contains("hidden"),
                "Quiet Room Invariant: #welcome-splash must remain hidden when feed becomes empty in active room");
        }

        // 7. Verify resetToHome() clears room state and restores welcome splash
        resetToHome();
        assert(Boolean(isInRoomSession) === false, "After resetToHome(): isInRoomSession must be false");
        const homeSplash = document.getElementById("welcome-splash");
        if (homeSplash) {
            assert(!homeSplash.classList.contains("hidden"),
                "After resetToHome(): #welcome-splash must be visible in home view");
        }

        print("  PASS: REQ-035.1 - REQ-035.5: Enhanced Burn Behavior verified cleanly (90s TTL, aging fade, per-message distributed burn, dissolve animation, quiet room invariant).");
    }

    // Test 16: REQ-036.1 .. REQ-036.6: Limbo Recovery, In-Room Plus Invite, Staggered Notice Flow & 90s TTL Exemption
    print("\n[Test 16] REQ-036.1 - REQ-036.6: Limbo recovery, in-room invite button, two-stage morphing & TTL exemption...");
    {
        // Reset state
        resetToHome();
        feed.children = [];
        departedPeers.clear();
        knownSessionMembers.clear();
        activeSessionPeers.clear();
        memberHandshakeState.clear();
        pendingOffers.clear();
        announcedJoinPeers.clear();
        clearPendingInvite();

        // 1 & 2. Client A (Host) creates room and Client B (Joiner) connects
        setLocalPeerId("peer-limbo-host");
        activeHostInviteUrl = null;
        await createRoomAsHost();

        const inviteUrl = getActiveHostInviteUrl();
        assert(inviteUrl !== null, "Host must generate activeHostInviteUrl");
        const match = inviteUrl.match(/payload=([^&]+)/);
        assert(match !== null, "Payload match must succeed");
        const jsonStr = atob(match[1].replace(/-/g, "+").replace(/_/g, "/"));
        const invitePayload = JSON.parse(jsonStr);

        const hostEngine = engine;
        const joinerBEngine = new ZeroWebRtcEngine(invitePayload.assigned_peer_id);
        const joinerBAnswer = await joinerBEngine.accept_offer_and_create_answer(invitePayload.sdp_offer.sdp, invitePayload.host_peer_id);

        const dropPayloadB = {
            invite_uuid: invitePayload.invite_uuid,
            token: invitePayload.token,
            sdp_answer: { sdp: joinerBAnswer },
            joiner_public_key: Array.from(crypto.getRandomValues(new Uint8Array(32))),
            joiner_peer_id: invitePayload.assigned_peer_id,
            peer_id: invitePayload.assigned_peer_id
        };

        const dropUrlB = `https://network.zero-chat.workers.dev/drop/${invitePayload.do_id}/${invitePayload.invite_uuid}`;
        await fetch(dropUrlB, {
            method: "POST",
            body: JSON.stringify(dropPayloadB)
        });

        await delay(100);
        assert(hostEngine.openPeerCount === 1, "Host must have 1 connected peer after joiner B links");
        assert(messageForm.classList.contains("hidden") === false, "messageForm must be visible when peer connects");
        assert(msgInput.disabled === false, "msgInput must be enabled when peer connects");
        assert(sendBtn.disabled === false, "sendBtn must be enabled when peer connects");
        assert(createRoomBar.classList.contains("hidden") === true, "createRoomBar must be hidden when peer connects");
        assert(headerAddBtn.classList.contains("hidden") === false, "headerAddBtn must be visible when peer connects");

        // 3. Client B disconnects -> Client A detects openPeerCount === 0 (limbo state)
        joinerBEngine.close();
        handlePeerDeparture(invitePayload.assigned_peer_id);
        assert(hostEngine.openPeerCount === 0, "Host must detect 0 open peers after peer departure");

        // 4. Verify limbo state UI on Client A (REQ-036.1)
        assert(messageForm.classList.contains("hidden") === true,
            "Limbo state: #message-form must be hidden (classList.contains('hidden') === true)");
        assert(createRoomBar.classList.contains("hidden") === false,
            "Limbo state: #create-room-bar must be visible (classList.contains('hidden') === false)");
        assert(btnCreateRoom.classList.contains("btn-emerald") === true,
            "Limbo state: #btn-create-room must have btn-emerald class");
        assert(!btnCreateRoom.classList.contains("btn-waiting"),
            "Limbo state: #btn-create-room must not have btn-waiting class");
        assert(btnCreateRoom.getAttribute("aria-label") === "Add peer",
            `Limbo state: #btn-create-room aria-label must be 'Add peer', got '${btnCreateRoom.getAttribute("aria-label")}'`);
        assert(btnCreateRoom.innerHTML.includes(ADD_PEER_ICON_SVG) || (btnCreateRoom.innerHTML.includes("<line x1=\"12\" y1=\"5\"") && btnCreateRoom.innerHTML.includes("stroke-width=\"2.5\"")),
            "Limbo state: #btn-create-room must contain ADD_PEER_ICON_SVG (+) icon");
        assert(headerAddBtn.classList.contains("hidden") === true,
            "Limbo state: #header-add-btn must be hidden");

        // 5. Client A clicks #btn-create-room (REQ-036.2 & REQ-036.3)
        _lastCopiedText = null;
        await btnCreateRoom.click();

        assert(btnCreateRoom.classList.contains("btn-waiting") === true,
            "Stage 2: #btn-create-room must transition to btn-waiting class");
        assert(!btnCreateRoom.classList.contains("btn-emerald"),
            "Stage 2: #btn-create-room must NOT have btn-emerald class");
        assert(btnCreateRoom.getAttribute("aria-label") === "Copy invite link",
            `Stage 2: #btn-create-room aria-label must be 'Copy invite link', got '${btnCreateRoom.getAttribute("aria-label")}'`);
        assert(btnCreateRoom.innerHTML.includes("<rect"),
            "Stage 2: #btn-create-room must display COPY_ICON_SVG");
        assert(_lastCopiedText !== null && _lastCopiedText.includes("#payload="),
            "Clipboard must contain the newly generated invite URL containing #payload=");

        // Verify immediate Notice 1
        let tempNotices = feed.querySelectorAll(".system-temporary");
        assert(tempNotices.length === 1,
            `Immediately after click, exactly 1 temporary notice expected, got ${tempNotices.length}`);
        assert(tempNotices[0].querySelector(".system-text").textContent === "Invite link copied to clipboard",
            `Notice 1 text must be 'Invite link copied to clipboard', got '${tempNotices[0].querySelector(".system-text").textContent}'`);

        // Advance timers by 500ms: verify Notice 2 with .ticking-dots
        advanceTimersByTime(500);
        await delay(50);
        tempNotices = feed.querySelectorAll(".system-temporary");
        assert(tempNotices.length === 2,
            `After 500ms, exactly 2 temporary notices expected, got ${tempNotices.length}`);
        assert(tempNotices[1].querySelector(".system-text").textContent === "Waiting for peer",
            `Notice 2 text must be 'Waiting for peer', got '${tempNotices[1].querySelector(".system-text").textContent}'`);
        assert(tempNotices[1].querySelector(".ticking-dots") !== null,
            "Notice 2 must contain animated .ticking-dots element");
        assert(window.getComputedStyle(tempNotices[1]).gap === "0px" || window.getComputedStyle(tempNotices[1]).gap === "0",
            "Notice 2 (.system-temporary) must have gap: 0 to eliminate ellipsis gap");

        // Verify transient-status also has zero gap (REQ-036.7)
        showTransientStatus("Establishing secure connection");
        const transientEl = feed.querySelector(".transient-status");
        assert(transientEl !== null, "Transient status element must be in feed");
        assert(window.getComputedStyle(transientEl).gap === "0px" || window.getComputedStyle(transientEl).gap === "0",
            ".transient-status must have gap: 0 to eliminate ellipsis gap");
        assert(transientEl.innerHTML === '<span class="transient-text">Establishing secure connection</span><span class="ticking-dots"></span>',
            "Transient status must have zero whitespace separation between text span and ticking dots span");
        clearTransientStatus();

        // 6. Verify 90s TTL exemption (REQ-036.5)
        advanceTimersByTime(95000);
        await delay(50);
        const tempNoticesAfter95s = feed.querySelectorAll(".system-temporary");
        assert(tempNoticesAfter95s.length === 2,
            `After 95s, temporary notices must NOT dissolve (TTL exempt), found ${tempNoticesAfter95s.length}`);
        assert(!tempNoticesAfter95s[0].classList.contains("message-aging") && !tempNoticesAfter95s[0].classList.contains("message-dissolve"),
            "Notice 1 must not age-fade or dissolve");
        assert(!tempNoticesAfter95s[1].classList.contains("message-aging") && !tempNoticesAfter95s[1].classList.contains("message-dissolve"),
            "Notice 2 must not age-fade or dissolve");

        // 7. Re-click #btn-create-room: verify clipboard re-copy & staggered notices restarted (REQ-036.3)
        _lastCopiedText = null;
        await btnCreateRoom.click();
        assert(_lastCopiedText !== null && _lastCopiedText.includes("#payload="),
            "Re-click while waiting must re-copy invite link to clipboard");
        assert(btnCreateRoom.classList.contains("btn-waiting"),
            "Re-click: #btn-create-room must remain in Stage 2 waiting state");

        tempNotices = feed.querySelectorAll(".system-temporary");
        assert(tempNotices.length === 1,
            `Immediately after re-click, only notice 1 should exist in feed, got ${tempNotices.length}`);
        assert(tempNotices[0].querySelector(".system-text").textContent === "Invite link copied to clipboard",
            "Notice 1 must restart immediately on re-click");

        advanceTimersByTime(500);
        await delay(50);
        tempNotices = feed.querySelectorAll(".system-temporary");
        assert(tempNotices.length === 2,
            `After 500ms following re-click, notice 2 must be appended, got ${tempNotices.length}`);
        assert(tempNotices[1].querySelector(".system-text").textContent === "Waiting for peer",
            "Notice 2 must reappear after 450ms stagger on re-click");

        // 8. Client C joins via the generated in-room invite (REQ-036.4)
        const matchC = _lastCopiedText.match(/payload=([^&]+)/);
        assert(matchC !== null, "In-room invite URL must contain payload");
        const jsonStrC = atob(matchC[1].replace(/-/g, "+").replace(/_/g, "/"));
        const invitePayloadC = JSON.parse(jsonStrC);

        const joinerCEngine = new ZeroWebRtcEngine(invitePayloadC.assigned_peer_id);
        const joinerCAnswer = await joinerCEngine.accept_offer_and_create_answer(invitePayloadC.sdp_offer.sdp, invitePayloadC.host_peer_id);

        const dropPayloadC = {
            invite_uuid: invitePayloadC.invite_uuid,
            token: invitePayloadC.token,
            sdp_answer: { sdp: joinerCAnswer },
            joiner_public_key: Array.from(crypto.getRandomValues(new Uint8Array(32))),
            joiner_peer_id: invitePayloadC.assigned_peer_id,
            peer_id: invitePayloadC.assigned_peer_id
        };

        const dropUrlC = `https://network.zero-chat.workers.dev/drop/${invitePayloadC.do_id}/${invitePayloadC.invite_uuid}`;
        await fetch(dropUrlC, {
            method: "POST",
            body: JSON.stringify(dropPayloadC)
        });

        await delay(100);
        assert(hostEngine.openPeerCount === 1, "Host must have 1 open peer after Client C connects");

        assert(createRoomBar.classList.contains("hidden") === true,
            "After Client C connects: #create-room-bar must be hidden");
        assert(messageForm.classList.contains("hidden") === false,
            "After Client C connects: #message-form must be restored and unhidden");
        assert(msgInput.disabled === false,
            "After Client C connects: #message-input must be enabled");
        assert(sendBtn.disabled === false,
            "After Client C connects: #send-button must be enabled");
        assert(feed.querySelectorAll(".system-temporary").length === 0,
            "After Client C connects: temporary system notices must be cleared");
        assert(headerAddBtn.classList.contains("hidden") === false,
            "After Client C connects: #header-add-btn must be visible");

        // 9. Reset / leave chat restores default Stage 1 (REQ-036.4)
        joinerCEngine.close();
        hostEngine.close();
        resetToHome();

        assert(btnCreateRoom.classList.contains("btn-emerald") === true,
            "Home View: #btn-create-room must restore to btn-emerald");
        assert(!btnCreateRoom.classList.contains("btn-waiting"),
            "Home View: #btn-create-room must NOT have btn-waiting class");
        assert(btnCreateRoom.getAttribute("aria-label") === "Create Chat",
            `Home View: #btn-create-room aria-label must restore to 'Create Chat', got '${btnCreateRoom.getAttribute("aria-label")}'`);
        assert(btnCreateRoom.innerHTML.includes("<svg") && btnCreateRoom.innerHTML.includes("<path"),
            "Home View: #btn-create-room must restore to CREATE_CHAT_ICON_SVG");

        print("  PASS: REQ-036.1 - REQ-036.6: Limbo recovery, in-room invite button, two-stage morphing & TTL exemption verified cleanly.");
    }

    // =========================================================================
    // TEST 17: Unified UI Rules, Minimalist Badges, Clean Burn Icon & Immediate Failure Recovery
    // =========================================================================
    {
        print("\n--- Test 17: Unified UI Rules, Minimalist Badges, Clean Burn Icon & Failure Recovery ---");
        function readCssFile() {
            if (typeof read === "function") {
                try { return read("zero_client/style.css"); } catch (_) {}
            }
            if (typeof require !== "undefined") {
                try { return require("fs").readFileSync("zero_client/style.css", "utf-8"); } catch (_) {}
            }
            return "";
        }
        resetToHome();

        // 1. Verify CSS styles for .brand-icon, badges, and .btn-message-burn
        const cssContent = readCssFile();
        if (cssContent) {
            assert(cssContent.includes(".btn-message-burn"), "Must define .btn-message-burn");
            assert(cssContent.includes(".feature-icon"), "Must define .feature-icon");
        }

        // 2. Home view: peer count displays slashed-user, leave button hidden, bottom button has create-chat icon
        assert(peerCountEl.innerHTML.includes("<svg"), "Home: peer count must display slashed user SVG");
        assert(headerLeaveBtn.classList.contains("hidden") === true, "Home: #header-leave-btn must be hidden");
        assert(headerAddBtn.classList.contains("hidden") === true, "Home: #header-add-btn must be hidden");
        assert(createRoomBar.classList.contains("hidden") === false, "Home: #create-room-bar must be visible");
        assert(btnCreateRoom.getAttribute("aria-label") === "Create Chat", "Home: #btn-create-room must be 'Create Chat'");

        // 3. Welcome splash subtitle: "Create -> Share -> Chat" and blue SVG icons
        const splash = document.getElementById("welcome-splash");
        assert(splash !== null, "Welcome splash must exist");
        assert(splash.innerHTML.includes("Create -&gt; Share -&gt; Chat") || splash.innerHTML.includes("Create -> Share -> Chat"),
            "Splash subtitle must be 'Create -> Share -> Chat'");
        assert(!splash.innerHTML.includes("Zero accounts. Zero servers. Zero history."),
            "Splash must omit 'Zero accounts. Zero servers. Zero history.'");
        const featureIcons = splash.querySelectorAll(".feature-icon");
        assert(featureIcons.length === 3, "Must have 3 feature icons");
        featureIcons.forEach((fi) => {
            assert(fi.querySelector("svg") !== null, "Feature icon must contain SVG, not emoji");
        });

        // 4. In-room 0 peers (Host waiting): peer count "0", leave button visible, bottom button plus icon
        await createRoomAsHost();
        assert(peerCountEl.textContent === "0", "Host waiting: peer count must be '0'");
        assert(headerLeaveBtn.classList.contains("hidden") === false, "Host waiting: #header-leave-btn must be visible");
        assert(headerAddBtn.classList.contains("hidden") === true, "Host waiting: #header-add-btn must be hidden");
        assert(messageForm.classList.contains("hidden") === true, "Host waiting: #message-form must be hidden");
        assert(createRoomBar.classList.contains("hidden") === false, "Host waiting: #create-room-bar must be visible");

        // 5. Leave button resets to home
        headerLeaveBtn.click();
        assert(peerCountEl.innerHTML.includes("<svg"), "After leave: peer count must restore to slashed user SVG");
        assert(headerLeaveBtn.classList.contains("hidden") === true, "After leave: #header-leave-btn must be hidden");

        // 6. Join failure immediate recovery: displays "Failed to establish connection", leave button visible, plus icon visible
        await joinRoomFromHash("#payload=invalid_nonexistent_payload_for_testing");
        assert(headerLeaveBtn.classList.contains("hidden") === false,
            "Failed connection: #header-leave-btn must be visible");
        assert(createRoomBar.classList.contains("hidden") === false,
            "Failed connection: #create-room-bar must be immediately visible without delay");
        assert(btnCreateRoom.classList.contains("btn-emerald") === true,
            "Failed connection: #btn-create-room must be btn-emerald");
        assert(btnCreateRoom.getAttribute("aria-label") === "Add peer" || btnCreateRoom.innerHTML.includes("line"),
            "Failed connection: #btn-create-room must display plus icon");

        // 7. Reset to home
        resetToHome();
        print("  PASS: REQ-037.1 - REQ-037.9 verified cleanly.");
    }

    print("\n===============================================================================");
    print("ALL BROWSER HEADLESS MESH TESTS PASSED SUCCESSFULLY!");
    print("===============================================================================");
}

runTests().then(() => {
    if (typeof quit === "function") {
        quit(0);
    }
}).catch((err) => {
    print("TEST SUITE FAILED: " + (err.message || err));
    if (err.stack) print(err.stack);
    if (typeof quit === "function") {
        quit(1);
    }
    throw err;
});
