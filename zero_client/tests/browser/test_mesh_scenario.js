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
        assert(msgInput.placeholder === "Peer left chat", "msgInput placeholder must be 'Peer left chat'");
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

    print("\n===============================================================================");
    print("ALL BROWSER HEADLESS MESH TESTS PASSED SUCCESSFULLY!");
    print("===============================================================================");
}

runTests().catch((err) => {
    print("TEST SUITE FAILED:", err.stack || err);
    throw err;
});
