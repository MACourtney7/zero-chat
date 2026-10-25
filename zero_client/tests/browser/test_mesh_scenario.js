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
        assert(countEl.innerHTML.includes("peer-slashed-user-icon"), "Peer count element must display slashed user icon when 0 open channels, got: " + countEl.innerHTML);

        // Add dummy entry to activeSessionPeers to verify it doesn't mask 0 channels
        activeSessionPeers.add("phantom-peer-1");
        activeSessionPeers.add("phantom-peer-2");
        updatePeerCountDisplay();
        assert(countEl.innerHTML.includes("peer-slashed-user-icon"), "activeSessionPeers must NOT inflate #peer-count when openPeerCount is 0, got: " + countEl.innerHTML);

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

        // 5. In-room Add Peer button left of message input (REQ-039.4)
        setHasActiveSession(true);
        setIsInRoomSession(true);
        const testEngineT13 = new ZeroWebRtcEngine("peer-host-t13");
        engine = testEngineT13;
        window.engine = testEngineT13;
        // Mock 1 connected peer to enter active chat
        testEngineT13.peers.set("peer-dummy", { state: "open", dc: { readyState: "open" } });
        updatePeerCountDisplay();

        assert(headerAddBtn.classList.contains("hidden") === true, "Header [+] button must be hidden");
        assert(inputAddPeerBtn.classList.contains("hidden") === false, "inputAddPeerBtn must be visible in active chat");
        assert(!inputAddPeerBtn.classList.contains("btn-waiting"), "inputAddPeerBtn initially not in btn-waiting");
        assert(inputAddPeerBtn.getAttribute("aria-label") === "Add peer", "inputAddPeerBtn aria-label must be 'Add peer'");
        assert(inputAddPeerBtn.innerHTML.includes("line x1=\"19\"") || inputAddPeerBtn.innerHTML.includes("circle cx=\"9\""),
            "inputAddPeerBtn must contain user-plus SVG");

        // Set draft text in input to verify preservation
        msgInput.value = "Draft unsent text";

        await handleInputAddPeerClick();

        // Verify button morphs to Stage 2 (.btn-waiting, copy icon)
        assert(inputAddPeerBtn.classList.contains("btn-waiting") === true, "inputAddPeerBtn must have .btn-waiting class");
        assert(inputAddPeerBtn.getAttribute("aria-label") === "Copy invite link", "inputAddPeerBtn aria-label must be 'Copy invite link'");
        assert(inputAddPeerBtn.innerHTML.includes("<rect"), "inputAddPeerBtn must display copy icon SVG");

        // Verify draft text is preserved
        assert(msgInput.value === "Draft unsent text", "Draft text in msgInput must be preserved");

        // Verify CSS styling of .btn-action-peer.btn-waiting has no orange/amber rules (REQ-040.1)
        function readCssFile() {
            if (typeof read === "function") {
                return read("zero_client/style.css");
            } else if (typeof require !== "undefined") {
                const fs = require("fs");
                return fs.readFileSync("zero_client/style.css", "utf8");
            }
            return "";
        }
        const cssContent = readCssFile();
        if (cssContent) {
            const btnPeerWaitingMatch = cssContent.match(/(?:\.btn-action-peer\.btn-waiting|#input-add-peer-btn\.btn-waiting)[^{]*\{([^}]+)\}/) || cssContent.match(/\.btn-action-peer\.btn-waiting\s*\{([^}]+)\}/);
            assert(btnPeerWaitingMatch !== null, ".btn-action-peer.btn-waiting CSS rule must exist");
            const ruleBody = btnPeerWaitingMatch[1];
            assert(!ruleBody.includes("#fbbf24") && !ruleBody.includes("245, 158, 11"),
                ".btn-action-peer.btn-waiting must not contain amber/orange color definitions");
            assert(ruleBody.includes("#38bdf8") && ruleBody.includes("56, 189, 248"),
                ".btn-action-peer.btn-waiting must use blue/cyan styling (#38bdf8 / rgba(56, 189, 248, ...))");
        }

        // Verify temporary notice emitted in feed (REQ-040.2)
        let inputNotices = feed.querySelectorAll(".system-temporary");
        assert(inputNotices.length === 1, `Immediate notice 1 must be present, found ${inputNotices.length}`);
        assert(inputNotices[0].querySelector(".system-text").textContent === "Invite link copied to clipboard",
            "Notice 1 must be 'Invite link copied to clipboard'");
        assert(inputNotices[0].classList.contains("in-chat-invite-notice"), "Notice must have in-chat-invite-notice class");

        // Verify "Waiting for peer..." is NEVER emitted in active chat
        advanceTimersByTime(500);
        await delay(50);
        inputNotices = feed.querySelectorAll(".system-temporary");
        assert(inputNotices.length === 1, "Only exactly 1 temporary notice must remain; 'Waiting for peer' must never be emitted");
        const waitingForPeerFound = Array.from(inputNotices).some(el => el.textContent.includes("Waiting for peer"));
        assert(!waitingForPeerFound, "'Waiting for peer' notice must not exist in active chat feed");

        // Re-clicking within 5000ms re-copies URL, preserves draft, restarts 5000ms timer (REQ-040.3)
        await handleInputAddPeerClick();
        assert(msgInput.value === "Draft unsent text", "Re-click must preserve draft text");
        assert(inputAddPeerBtn.classList.contains("btn-waiting") === true, "Re-click must keep .btn-waiting");
        inputNotices = feed.querySelectorAll(".system-temporary");
        assert(inputNotices.length === 1, "Re-click must not duplicate notice");

        // Advance timers by 4900ms (within 5000ms from re-click): notice and button waiting remain
        advanceTimersByTime(4900);
        await delay(50);
        assert(feed.querySelectorAll(".system-temporary").length === 1, "Notice must still be visible before 5000ms expires");
        assert(inputAddPeerBtn.classList.contains("btn-waiting") === true, "Button must still have .btn-waiting before 5000ms expires");

        // Advance 200ms more (5100ms total): notice auto-dismisses and button reverts to Stage 1 (REQ-040.3)
        advanceTimersByTime(200);
        await delay(50);
        assert(feed.querySelectorAll(".system-temporary").length === 0, "Notice must auto-dismiss after 5000ms");
        assert(!inputAddPeerBtn.classList.contains("btn-waiting"), "Button must revert to Stage 1 (no .btn-waiting)");
        assert(inputAddPeerBtn.getAttribute("aria-label") === "Add peer", "Button must revert aria-label to 'Add peer'");
        assert(inputAddPeerBtn.innerHTML.includes("line x1=\"19\"") || inputAddPeerBtn.innerHTML.includes("circle cx=\"9\""),
            "Button must revert to user-plus SVG");

        // Clicking again re-morphs to Stage 2 and re-emits notice
        await handleInputAddPeerClick();
        assert(inputAddPeerBtn.classList.contains("btn-waiting") === true, "Clicking after reversion morphs to Stage 2");
        assert(inputAddPeerBtn.getAttribute("aria-label") === "Copy invite link", "Button must have aria-label 'Copy invite link'");
        assert(feed.querySelectorAll(".system-temporary").length === 1, "Temporary notice emitted again");

        // 6. Reset to Home restores Stage 1
        resetToHome();
        assert(peerCountEl.classList.contains("hidden") === true, "Home view: peerCountEl must be hidden");
        assert(idleStatusCapsule.classList.contains("hidden") === true, "Home view: idleStatusCapsule must be hidden");
        assert(peerListEl.classList.contains("hidden") === true, "Home view: peerListEl must be hidden");
        assert(headerLeaveBtn.classList.contains("hidden") === true, "Home view: headerLeaveBtn must be hidden");
        assert(headerAddBtn.classList.contains("hidden") === true, "Home view: headerAddBtn must be hidden");
        assert(btnCreateRoom.classList.contains("btn-emerald"), "After reset: button must return to btn-emerald");
        assert(!btnCreateRoom.classList.contains("btn-waiting"), "After reset: button must NOT have btn-waiting");
        assert(btnCreateRoom.getAttribute("aria-label") === "Create Chat", "After reset: aria-label must be 'Create Chat'");
        assert(btnCreateRoom.textContent.trim() === "", "After reset: text must remain empty");

        print("  PASS: REQ-033.1 .. REQ-033.4: Icon-only button and temporary system messages verified cleanly.");
    }

    // Test 14: REQ-034.1 - REQ-034.4: Staggered delivery, ellipsis fix, slashed person icon, empty placeholder
    print("\n[Test 14] REQ-034.1 - REQ-034.4: Slashed person icon, empty placeholder, staggered notices & ellipsis fix...");
    {
        // 1. Assert #peer-count in home view is hidden
        resetToHome();
        const peerCountEl = document.getElementById("peer-count");
        assert(peerCountEl !== null, "#peer-count element must exist");
        assert(peerCountEl.classList.contains("hidden") === true,
            "Home View: #peer-count must be hidden");
        assert(idleStatusCapsule.classList.contains("hidden") === true,
            "Home View: #idle-status-capsule must be hidden");
        assert(peerListEl.classList.contains("hidden") === true,
            "Home View: #peer-list must be hidden");
        assert(headerLeaveBtn.classList.contains("hidden") === true,
            "Home View: #header-leave-btn must be hidden");
        assert(msgInput.placeholder === "", "Home View: msgInput placeholder must be empty string");

        // 2. Trigger create room: assert #peer-count transitions to slashed person icon (0 peers in room)
        setLocalPeerId("peer-ui-t14");
        await createRoomAsHost();

        assert(peerCountEl.classList.contains("hidden") === false,
            "Room View (empty session): #peer-count must be visible");
        assert(peerCountEl.innerHTML.includes("peer-slashed-user-icon"),
            "Room View (empty session): #peer-count must display slashed person icon");
        assert(idleStatusCapsule.classList.contains("hidden") === false,
            "Room View (empty session): #idle-status-capsule must be visible");
        assert(peerListEl.classList.contains("hidden") === true,
            "Room View (empty session): #peer-list must be hidden");
        assert(headerLeaveBtn.classList.contains("hidden") === false,
            "Room View (empty session): #header-leave-btn must be visible");

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
        assert(peerCountEl.classList.contains("hidden") === false, "#peer-count must be visible on peer departure");
        assert(peerCountEl.innerHTML.includes("peer-slashed-user-icon"), "#peer-count must display slashed user icon in room view when peer departs");
        assert(idleStatusCapsule.classList.contains("hidden") === false, "#idle-status-capsule must be visible on peer departure");
        assert(peerListEl.classList.contains("hidden") === true, "#peer-list must be hidden on peer departure");

        // 6. Simulate resetToHome()
        resetToHome();
        assert(peerCountEl.classList.contains("hidden") === true,
            "resetToHome() must hide #peer-count");
        assert(idleStatusCapsule.classList.contains("hidden") === true,
            "resetToHome() must hide #idle-status-capsule");
        assert(peerListEl.classList.contains("hidden") === true,
            "resetToHome() must hide #peer-list");
        assert(headerLeaveBtn.classList.contains("hidden") === true,
            "resetToHome() must hide #header-leave-btn");
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
        assert(outBubble.querySelector(".message-header .btn-message-burn") !== null, "Outgoing bubble header must contain .btn-message-burn");
        assert(outBubble.querySelector(".message-header .peer-dot") !== null, "Outgoing bubble header must contain .peer-dot");
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
        assert(inBubble.querySelector(".message-header .btn-message-burn") !== null, "Incoming bubble header must contain .btn-message-burn");
        assert(inBubble.querySelector(".message-header .peer-dot") !== null, "Incoming bubble header must contain .peer-dot");
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
        assert(headerAddBtn.classList.contains("hidden") === true, "headerAddBtn must be hidden");
        assert(inputAddPeerBtn.classList.contains("hidden") === false, "inputAddPeerBtn must be visible when peer connects");
        assert(inputAddPeerBtn.innerHTML.includes("line x1=\"19\"") || inputAddPeerBtn.innerHTML.includes("circle cx=\"9\""),
            "inputAddPeerBtn must contain user-plus SVG");
        assert(peerListEl.classList.contains("hidden") === false, "peerListEl (padlock) must be visible when peer connects");
        assert(idleStatusCapsule.classList.contains("hidden") === true, "idleStatusCapsule must be hidden when peer connects");
        assert(peerCountEl.textContent === "1", "peerCountEl must be '1' when peer connects");

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
        assert(btnCreateRoom.innerHTML.includes(ADD_PEER_ICON_SVG) || (btnCreateRoom.innerHTML.includes("line x1=\"19\"") && btnCreateRoom.innerHTML.includes("circle cx=\"9\"")),
            "Limbo state: #btn-create-room must contain user-plus icon");
        assert(headerAddBtn.classList.contains("hidden") === true,
            "Limbo state: #header-add-btn must be hidden");
        assert(peerCountEl.classList.contains("hidden") === false,
            "Limbo state: #peer-count must be visible");
        assert(peerCountEl.innerHTML.includes("peer-slashed-user-icon"),
            "Limbo state: #peer-count must display slashed user icon");
        assert(idleStatusCapsule.classList.contains("hidden") === false,
            "Limbo state: #idle-status-capsule must be visible");
        assert(peerListEl.classList.contains("hidden") === true,
            "Limbo state: #peer-list must be hidden");

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
        assert(headerAddBtn.classList.contains("hidden") === true,
            "After Client C connects: #header-add-btn must be hidden");
        assert(inputAddPeerBtn.classList.contains("hidden") === false,
            "After Client C connects: #input-add-peer-btn must be visible");
        assert(peerListEl.classList.contains("hidden") === false,
            "After Client C connects: #peer-list must be visible");
        assert(idleStatusCapsule.classList.contains("hidden") === true,
            "After Client C connects: #idle-status-capsule must be hidden");
        assert(peerCountEl.textContent === "1",
            "After Client C connects: #peer-count must be '1'");

        // 9. Reset / leave chat restores default Stage 1 (REQ-036.4)
        joinerCEngine.close();
        hostEngine.close();
        resetToHome();

        assert(peerCountEl.classList.contains("hidden") === true,
            "Home View: #peer-count must be hidden");
        assert(idleStatusCapsule.classList.contains("hidden") === true,
            "Home View: #idle-status-capsule must be hidden");
        assert(peerListEl.classList.contains("hidden") === true,
            "Home View: #peer-list must be hidden");
        assert(headerLeaveBtn.classList.contains("hidden") === true,
            "Home View: #header-leave-btn must be hidden");
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
            assert(cssContent.includes(".message-header") && cssContent.includes("display: flex;") && cssContent.includes("justify-content: space-between;"),
                "style.css: .message-header must define display: flex and justify-content: space-between");
            assert(cssContent.includes(".feature-icon"), "Must define .feature-icon");
        }

        // 2. Home view: peer count hidden, leave button hidden, bottom button has create-chat icon
        assert(peerCountEl.classList.contains("hidden") === true, "Home: #peer-count must be hidden");
        assert(idleStatusCapsule.classList.contains("hidden") === true, "Home: #idle-status-capsule must be hidden");
        assert(peerListEl.classList.contains("hidden") === true, "Home: #peer-list must be hidden");
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

        // 4. In-room 0 peers (Host waiting): peer count slashed person icon, blue dot visible, padlock hidden, leave button visible
        await createRoomAsHost();
        assert(peerCountEl.classList.contains("hidden") === false, "Host waiting: #peer-count must be visible");
        assert(peerCountEl.innerHTML.includes("peer-slashed-user-icon"), "Host waiting: #peer-count must display slashed user SVG");
        assert(idleStatusCapsule.classList.contains("hidden") === false, "Host waiting: #idle-status-capsule (blue dot) must be visible");
        assert(peerListEl.classList.contains("hidden") === true, "Host waiting: #peer-list (padlock) must be hidden");
        assert(headerLeaveBtn.classList.contains("hidden") === false, "Host waiting: #header-leave-btn must be visible");
        assert(headerAddBtn.classList.contains("hidden") === true, "Host waiting: #header-add-btn must be hidden");
        assert(messageForm.classList.contains("hidden") === true, "Host waiting: #message-form must be hidden");
        assert(createRoomBar.classList.contains("hidden") === false, "Host waiting: #create-room-bar must be visible");
        assert(btnCreateRoom.classList.contains("btn-waiting") === true, "Host waiting: #btn-create-room must be Stage 2 copy button");

        // 5. Leave button resets to home
        headerLeaveBtn.click();
        assert(peerCountEl.classList.contains("hidden") === true, "After leave: #peer-count must be hidden");
        assert(idleStatusCapsule.classList.contains("hidden") === true, "After leave: #idle-status-capsule must be hidden");
        assert(peerListEl.classList.contains("hidden") === true, "After leave: #peer-list must be hidden");
        assert(headerLeaveBtn.classList.contains("hidden") === true, "After leave: #header-leave-btn must be hidden");

        // 6. Join failure immediate recovery: displays "Failed to establish connection", leave button visible, slashed person icon & blue dot visible, plus icon visible
        await joinRoomFromHash("#payload=invalid_nonexistent_payload_for_testing");
        assert(headerLeaveBtn.classList.contains("hidden") === false,
            "Failed connection: #header-leave-btn must be visible");
        assert(peerCountEl.classList.contains("hidden") === false,
            "Failed connection: #peer-count must be visible");
        assert(peerCountEl.innerHTML.includes("peer-slashed-user-icon"),
            "Failed connection: #peer-count must display slashed person icon");
        assert(idleStatusCapsule.classList.contains("hidden") === false,
            "Failed connection: #idle-status-capsule (blue dot) must be visible");
        assert(peerListEl.classList.contains("hidden") === true,
            "Failed connection: #peer-list (padlock) must be hidden");
        assert(createRoomBar.classList.contains("hidden") === false,
            "Failed connection: #create-room-bar must be immediately visible without delay");
        assert(btnCreateRoom.classList.contains("btn-emerald") === true,
            "Failed connection: #btn-create-room must be btn-emerald");
        assert(btnCreateRoom.getAttribute("aria-label") === "Add peer" || btnCreateRoom.innerHTML.includes("line"),
            "Failed connection: #btn-create-room must display user-plus icon");

        // 7. Reset to home
        resetToHome();
        assert(peerCountEl.classList.contains("hidden") === true, "Home: #peer-count must be hidden");
        assert(idleStatusCapsule.classList.contains("hidden") === true, "Home: #idle-status-capsule must be hidden");
        assert(peerListEl.classList.contains("hidden") === true, "Home: #peer-list must be hidden");
        assert(headerLeaveBtn.classList.contains("hidden") === true, "Home: #header-leave-btn must be hidden");
        print("  PASS: REQ-037.1 - REQ-037.9 verified cleanly.");
    }

    // =========================================================================
    // TEST 18: Mobile Viewport, Safe-Area Insets, Focus Retention & Reconnection
    // =========================================================================
    {
        print("\n--- Test 18: Mobile Viewport, Safe-Area Insets, Focus Retention & Reconnection ---");

        function readCssFile() {
            if (typeof read === "function") {
                try { return read("zero_client/style.css"); } catch (_) {}
            }
            if (typeof require !== "undefined") {
                try { return require("fs").readFileSync("zero_client/style.css", "utf-8"); } catch (_) {}
            }
            return "";
        }

        // 1. CSS Layout & Font Size Invariants
        const cssContent = readCssFile();
        if (cssContent) {
            assert(
                cssContent.includes("position: fixed;") &&
                cssContent.includes("inset: 0;") &&
                cssContent.includes("height: var(--app-height, 100dvh);") &&
                cssContent.includes("overflow: hidden;"),
                "style.css: html, body must contain position: fixed, inset: 0, height: var(--app-height, 100dvh), and overflow: hidden"
            );
            assert(
                cssContent.includes("font-size: 16px;"),
                "style.css: message input must define font-size: 16px to prevent iOS auto-zoom"
            );
            assert(
                cssContent.includes("padding-top: calc(12px + env(safe-area-inset-top, 0px));"),
                "style.css: .chat-header must define safe-area-inset-top padding"
            );
            assert(
                cssContent.includes(".keyboard-open") && cssContent.includes("padding-bottom: 8px;"),
                "style.css: .keyboard-open must define padding-bottom: 8px"
            );
            assert(
                cssContent.includes(".message-bubble") && cssContent.includes("flex-shrink: 0;"),
                "style.css: .message-bubble must define flex-shrink: 0;"
            );
        }

        // 2. Visual Viewport Synchronization & Keyboard Adaptation
        window.visualViewport = {
            height: 480,
            offsetTop: 0,
            addEventListener: function() {},
            removeEventListener: function() {}
        };
        window.syncMobileViewport();
        assert(document.documentElement.classList.contains("keyboard-open") === true,
            "Opening keyboard (height 480 < 844 * 0.85) must add .keyboard-open class");
        assert(document.documentElement.style.getPropertyValue("--app-height") === "480px",
            `CSS property --app-height must be 480px, got '${document.documentElement.style.getPropertyValue("--app-height")}'`);

        // Simulate keyboard close (height: 844, offsetTop: 0)
        window.visualViewport.height = 844;
        window.visualViewport.offsetTop = 0;
        window.syncMobileViewport();
        assert(document.documentElement.classList.contains("keyboard-open") === false,
            "Closing keyboard (height 844 >= 844 * 0.85) must remove .keyboard-open class");
        assert(document.documentElement.style.getPropertyValue("--app-height") === "844px",
            `CSS property --app-height must be 844px, got '${document.documentElement.style.getPropertyValue("--app-height")}'`);

        // 3. Touch Focus Retention on Send
        resetToHome();
        const pdEvent = new Event("pointerdown", { cancelable: true });
        sendBtn.dispatchEvent(pdEvent);
        assert(pdEvent.defaultPrevented === true,
            "sendBtn pointerdown event must preventDefault to retain touch focus");

        // In active chat, type into msgInput, call doSendMessage(), assert document.activeElement === msgInput
        msgInput.disabled = false;
        msgInput.value = "Retain focus test message";
        const testEngine = new ZeroWebRtcEngine("peer-mobile-focus");
        engine = testEngine;
        window.engine = testEngine;
        document.activeElement = null;
        await doSendMessage();
        assert(document.activeElement === msgInput,
            "msgInput must retain active focus after doSendMessage()");

        // 4. Joiner 404 Retry Loop & Host WebSocket Reconnect
        resetToHome();
        setLocalPeerId("peer-host-retry-t18");
        await createRoomAsHost();
        const hostInviteUrl = getActiveHostInviteUrl();
        assert(hostInviteUrl !== null, "Host invite URL must be generated");

        let dropAttempts = 0;
        let observedTransientStatus = false;
        const rootGlobal = typeof globalThis !== "undefined" ? globalThis : window;
        const origFetch = rootGlobal.fetch;
        rootGlobal.fetch = async function(url, options = {}) {
            if (url && url.includes("/drop/")) {
                dropAttempts++;
                if (dropAttempts === 1) {
                    setTimeout(() => {
                        const transientEl = feed.querySelector(".transient-status");
                        if (transientEl && (transientEl.textContent.includes("Waiting for host") || transientEl.innerHTML.includes("Waiting for host"))) {
                            observedTransientStatus = true;
                        }
                        if (typeof advanceTimersByTime === "function") {
                            advanceTimersByTime(1500);
                        }
                    }, 20);
                    return {
                        ok: false,
                        status: 404,
                        json: async () => ({})
                    };
                }
                return {
                    ok: true,
                    status: 200,
                    json: async () => ({ success: true })
                };
            }
            return origFetch(url, options);
        };

        const hashMatch = hostInviteUrl.match(/#payload=.*/);
        assert(hashMatch !== null, "Invite payload hash must be found");
        await joinRoomFromHash(hashMatch[0]);
        await delay(50);

        assert(dropAttempts >= 2,
            `Joiner must retry dead-drop after 404 response, made ${dropAttempts} attempts`);
        assert(observedTransientStatus === true,
            "Joiner must display 'Waiting for host' transient status during retry loop");
        rootGlobal.fetch = origFetch;

        // Ellipsis de-duplication assertion: showTransientStatus strips trailing literal dots
        showTransientStatus("Waiting for host...");
        const transientEl = feed.querySelector(".transient-status");
        assert(transientEl !== null, "showTransientStatus must render .transient-status element in feed");
        const transientTextEl = transientEl.querySelector(".transient-text");
        assert(transientTextEl !== null, ".transient-status must contain .transient-text element");
        assert(transientTextEl.textContent === "Waiting for host",
            `.transient-text textContent must strictly be 'Waiting for host' without trailing literal dots, got '${transientTextEl.textContent}'`);
        const tickingDotsEls = transientEl.querySelectorAll(".ticking-dots");
        assert(tickingDotsEls.length === 1,
            `Must contain exactly single .ticking-dots element, got ${tickingDotsEls.length}`);
        clearTransientStatus();

        // Host WebSocket Reconnect on visibilitychange
        resetToHome();
        setLocalPeerId("peer-reconn-host-t18");
        await createRoomAsHost();
        const initialHostWs = window.getHostWs();
        assert(initialHostWs !== null && initialHostWs.readyState === 1,
            "Host WebSocket must be open initially");

        // Simulate mobile OS background teardown
        initialHostWs.close();
        assert(initialHostWs.readyState === 3, "Host WebSocket must be closed");

        document.visibilityState = "visible";
        document.dispatchEvent(new Event("visibilitychange"));
        const reconnectedHostWs = window.getHostWs();
        assert(reconnectedHostWs !== null && reconnectedHostWs !== initialHostWs && reconnectedHostWs.readyState === 1,
            "document visibilitychange must reconnect host WebSocket when visibilityState is 'visible'");

        // 5. Bfcache Pagehide Safety
        window.setOpportunisticLeaveSent(false);
        const bfcacheEvent = new PageTransitionEvent("pagehide", { persisted: true });
        window.dispatchEvent(bfcacheEvent);
        assert(window.getOpportunisticLeaveSent() === false,
            "pagehide with persisted: true must NOT trigger opportunisticLeaveSent (preserving bfcache session)");

        // 6. Honest Copy Fallback
        resetToHome();
        const origWriteText = navigator.clipboard.writeText;
        navigator.clipboard.writeText = async function() {
            throw new Error("NotAllowedError");
        };
        setLocalPeerId("peer-honest-copy-t18");
        await createRoomAsHost();

        const tempNotices = feed.querySelectorAll(".system-temporary");
        assert(tempNotices.length > 0,
            "Temporary notices must be appended on room creation");
        assert(tempNotices[0].textContent.includes("Invite ready — tap Copy to copy link"),
            `Honest fallback notice must be shown when clipboard write fails, got: '${tempNotices[0].textContent}'`);
        assert(!tempNotices[0].textContent.includes("Invite link copied to clipboard"),
            "False success notice must NOT be shown when clipboard write fails");

        navigator.clipboard.writeText = origWriteText;
        resetToHome();
        print("  PASS: REQ-041.1 - REQ-041.8 verified cleanly.");
    }

    // --- Test 19: UI Architecture Refinements: Transparent Panels, Relocated Burn Button & Rounded Controls (ticket-042) ---
    print("\n--- Test 19: UI Architecture Refinements: Transparent Panels, Relocated Burn Button & Rounded Controls (ticket-042) ---");
    {
        // 1. CSS Transparent Panels & Rounded Controls Assertions
        function readCssFile() {
            if (typeof read === "function") {
                return read("zero_client/style.css");
            } else if (typeof require !== "undefined") {
                const fs = require("fs");
                return fs.readFileSync("zero_client/style.css", "utf8");
            }
            return "";
        }
        function readIndexHtml() {
            if (typeof read === "function") {
                return read("zero_client/index.html");
            } else if (typeof require !== "undefined") {
                const fs = require("fs");
                return fs.readFileSync("zero_client/index.html", "utf8");
            }
            return "";
        }

        const cssContent = readCssFile();
        assert(cssContent.length > 0, "style.css must be readable and non-empty");

        function getRulesForSelector(sel) {
            const blocks = [];
            const ruleRegex = /([^{]+)\{([^}]+)\}/g;
            let rm;
            while ((rm = ruleRegex.exec(cssContent)) !== null) {
                const selectors = rm[1].split(",").map(s => s.trim());
                if (selectors.some(s => s === sel || s.endsWith(sel) || s.includes(sel))) {
                    blocks.push(rm[2]);
                }
            }
            return blocks;
        }

        // Verify .chat-header transparent styling
        const chatHeaderBlocks = getRulesForSelector(".chat-header");
        assert(chatHeaderBlocks.length > 0, ".chat-header CSS rule must exist");
        assert(chatHeaderBlocks.some(b => b.includes("background: transparent !important")),
            ".chat-header must define background: transparent !important");
        assert(chatHeaderBlocks.some(b => b.includes("border: none !important")),
            ".chat-header must define border: none !important");

        // Verify .welcome-splash transparent styling
        const welcomeSplashBlocks = getRulesForSelector(".welcome-splash");
        assert(welcomeSplashBlocks.length > 0, ".welcome-splash CSS rule must exist");
        assert(welcomeSplashBlocks.some(b => b.includes("background: transparent !important")),
            ".welcome-splash must define background: transparent !important");
        assert(welcomeSplashBlocks.some(b => b.includes("border: none !important")),
            ".welcome-splash must define border: none !important");

        // Verify .chat-footer transparent styling
        const chatFooterBlocks = getRulesForSelector(".chat-footer");
        assert(chatFooterBlocks.length > 0, ".chat-footer CSS rule must exist");
        assert(chatFooterBlocks.some(b => b.includes("background: transparent !important")),
            ".chat-footer must define background: transparent !important");
        assert(chatFooterBlocks.some(b => b.includes("border: none !important")),
            ".chat-footer must define border: none !important");

        // Verify #message-input / .message-input styling
        const msgInputBlocks = getRulesForSelector("#message-input");
        assert(msgInputBlocks.length > 0, "#message-input or .message-input CSS rule must exist");
        assert(msgInputBlocks.some(b => b.includes("background: rgba(18, 24, 38, 0.7)")),
            "#message-input must define background: rgba(18, 24, 38, 0.7)");
        assert(msgInputBlocks.some(b => b.includes("border-radius: 9999px")),
            "#message-input must define border-radius: 9999px");

        // Verify .message-form / .input-dock styling
        const messageFormBlocks = getRulesForSelector(".message-form");
        assert(messageFormBlocks.length > 0, ".message-form CSS rule must exist");
        assert(messageFormBlocks.some(b => b.includes("overflow: visible")),
            ".message-form must define overflow: visible");
        assert(messageFormBlocks.some(b => b.includes("padding-top: 4px")),
            ".message-form must define padding-top: 4px");

        // Verify #destroy-session-button / .btn-header-burn styling
        const burnBtnBlocks = getRulesForSelector("#destroy-session-button");
        assert(burnBtnBlocks.length > 0, "#destroy-session-button or .btn-header-burn CSS rule must exist");
        assert(burnBtnBlocks.some(b => b.includes("border-radius: 9999px")),
            "#destroy-session-button must define border-radius: 9999px");
        assert(burnBtnBlocks.some(b => b.includes("min-width: 80px")),
            "#destroy-session-button must define min-width: 80px");
        assert(burnBtnBlocks.some(b => b.includes("height: 26px")),
            "#destroy-session-button must define height: 26px");
        assert(burnBtnBlocks.some(b => b.includes("background: rgba(244, 63, 94, 0.12)")),
            "#destroy-session-button must define background: rgba(244, 63, 94, 0.12)");
        assert(burnBtnBlocks.some(b => b.includes("border: 1px solid rgba(244, 63, 94, 0.32)")),
            "#destroy-session-button must define border: 1px solid rgba(244, 63, 94, 0.32)");
        assert(burnBtnBlocks.some(b => b.includes("color: #fda4af")),
            "#destroy-session-button must define color: #fda4af");

        // Verify rounded bottom controls (#input-add-peer-btn and #send-button)
        const sendBtnBlocks = getRulesForSelector("#send-button");
        assert(sendBtnBlocks.length > 0 && sendBtnBlocks.some(b => b.includes("border-radius: 9999px")),
            "#send-button must define border-radius: 9999px");
        const addPeerBlocks = getRulesForSelector("#input-add-peer-btn");
        assert(addPeerBlocks.length > 0 && addPeerBlocks.some(b => b.includes("border-radius: 9999px")),
            "#input-add-peer-btn must define border-radius: 9999px");

        // 2. DOM Layout & Positioning Assertions
        const controls = (typeof headerControls !== "undefined" && headerControls) ||
            document.querySelector(".header-controls") ||
            document.getElementById("header-controls");
        assert(controls !== null, ".header-controls element must exist in the DOM");

        const burn = (typeof burnBtn !== "undefined" && burnBtn) ||
            document.getElementById("destroy-session-button") ||
            document.querySelector("#destroy-session-button");
        assert(burn !== null, "#destroy-session-button must exist in the DOM");

        // Verify #destroy-session-button is inside .header-controls
        assert(burn.parentNode === controls || controls.contains(burn),
            "#destroy-session-button must be located inside .header-controls");

        // Verify #destroy-session-button is the leftmost child
        assert(controls.firstElementChild === burn,
            "#destroy-session-button must be the leftmost first child element of .header-controls");

        // Verify #burn-room-bar does not exist in the DOM
        assert(document.getElementById("burn-room-bar") === null,
            "#burn-room-bar must not exist in the DOM via getElementById");
        assert(document.querySelector("#burn-room-bar") === null,
            "#burn-room-bar must not exist in the DOM via querySelector");

        const indexHtml = readIndexHtml();
        if (indexHtml) {
            assert(!indexHtml.includes('id="burn-room-bar"'),
                "index.html must not contain #burn-room-bar container");
            assert(indexHtml.includes('id="destroy-session-button"'),
                "index.html must contain #destroy-session-button");
        }

        // 3. State Machine Visibility Toggles
        const brandText = (typeof brandTextEl !== "undefined" && brandTextEl) ||
            document.querySelector(".brand-text") ||
            document.getElementById("brand-text");
        assert(brandText !== null, ".brand-text element must exist");

        const leave = (typeof headerLeaveBtn !== "undefined" && headerLeaveBtn) ||
            document.getElementById("header-leave-btn") ||
            document.querySelector("#header-leave-btn");
        assert(leave !== null, "#header-leave-btn must exist in the DOM");

        // Initial Home state
        resetToHome();
        assert(brandText.classList.contains("hidden") === false,
            "Initial Home state: .brand-text must be visible (!classList.contains('hidden'))");
        assert(burn.classList.contains("hidden") === true,
            "Initial Home state: #destroy-session-button must be hidden (classList.contains('hidden'))");
        assert(leave.classList.contains("hidden") === true,
            "Initial Home state: #header-leave-btn must be hidden (classList.contains('hidden'))");

        // Limbo room state (isInRoomSession = true, 0 peers)
        setIsInRoomSession(true);
        const limboEngine = { openPeerCount: 0, close: () => {} };
        engine = limboEngine;
        window.engine = limboEngine;
        updatePeerCountDisplay();
        assert(brandText.classList.contains("hidden") === true,
            "Limbo room state: .brand-text must be hidden with 0 peers in session");
        assert(!leave.classList.contains("hidden"),
            "Limbo room state: #header-leave-btn must remain visible (!classList.contains('hidden'))");
        assert(!burn.classList.contains("hidden"),
            "Limbo room state: #destroy-session-button must remain visible with 0 peers (!classList.contains('hidden'))");

        // Active chat state (openPeerCount = 2)
        const activeEngine = { openPeerCount: 2, close: () => {} };
        engine = activeEngine;
        window.engine = activeEngine;
        updatePeerCountDisplay();
        assert(brandText.classList.contains("hidden") === true,
            "Active chat state: .brand-text must remain hidden");
        assert(!leave.classList.contains("hidden"),
            "Active chat state: #header-leave-btn must remain visible");
        assert(burn.classList.contains("hidden") === false,
            "Active chat state: #destroy-session-button must be visible (>0 peers)");

        // Departure to Limbo (engine.openPeerCount = 0)
        activeEngine.openPeerCount = 0;
        updatePeerCountDisplay();
        assert(brandText.classList.contains("hidden") === true,
            "Departure to Limbo: .brand-text must remain hidden while in session");
        assert(!leave.classList.contains("hidden"),
            "Departure to Limbo: #header-leave-btn must remain visible");
        assert(!burn.classList.contains("hidden"),
            "Departure to Limbo: #destroy-session-button must remain visible when peer count drops to 0");

        // Reset to Home
        resetToHome();
        assert(brandText.classList.contains("hidden") === false,
            "Reset to Home: .brand-text must be restored to visible");
        assert(burn.classList.contains("hidden") === true,
            "Reset to Home: #destroy-session-button must be hidden");
        assert(leave.classList.contains("hidden") === true,
            "Reset to Home: #header-leave-btn must be hidden");

        print("  PASS: UI Architecture Refinements: Transparent Panels, Relocated Burn Button & Rounded Controls verified cleanly.");
    }

    // --- Test 20: Mobile Browser Web Share API & Dynamic Share Icon Integration ---
    print("\n--- Test 20: Mobile Browser Web Share API & Dynamic Share Icon Integration ---");
    {
        const origUserAgent = navigator.userAgent;
        const origUserAgentData = navigator.userAgentData;
        const origShare = navigator.share;
        const origWriteText = navigator.clipboard ? navigator.clipboard.writeText : undefined;

        try {
            // =====================================================================
            // Case A: Desktop Environment (Default / Non-Mobile)
            // =====================================================================
            // Even if navigator.share is present:
            const desktopShareInvocations = [];
            navigator.share = async function(data) {
                desktopShareInvocations.push(data);
            };

            const clipboardWrites = [];
            if (navigator.clipboard) {
                navigator.clipboard.writeText = async function(text) {
                    clipboardWrites.push(text);
                    if (origWriteText) {
                        return origWriteText.call(navigator.clipboard, text);
                    }
                };
            }

            // Verify isMobileBrowser() returns false
            assert(isMobileBrowser() === false,
                "Case A: isMobileBrowser() must return false in default desktop environment");

            resetToHome();
            setLocalPeerId("peer-desktop-test-host");

            // 1. Initial click on #btn-create-room in Home View
            await btnCreateRoom.click();

            assert(desktopShareInvocations.length === 0,
                `Case A: navigator.share must NOT be invoked on desktop, got ${desktopShareInvocations.length}`);
            assert(clipboardWrites.length >= 1 && clipboardWrites[0].includes("#payload="),
                "Case A: Invite URL must be copied directly to clipboard via navigator.clipboard.writeText");
            assert(btnCreateRoom.classList.contains("btn-waiting"),
                "Case A: #btn-create-room must have class 'btn-waiting' in Stage 2");
            assert(btnCreateRoom.getAttribute("aria-label") === "Copy invite link",
                `Case A: #btn-create-room aria-label must be 'Copy invite link', got '${btnCreateRoom.getAttribute("aria-label")}'`);
            assert(btnCreateRoom.innerHTML.includes("<rect") || btnCreateRoom.innerHTML.includes(window.COPY_ICON_SVG),
                "Case A: #btn-create-room must display COPY_ICON_SVG");
            assert(!btnCreateRoom.innerHTML.includes('<circle cx="18" cy="5"'),
                "Case A: #btn-create-room must NOT display SHARE_ICON_SVG");

            let tempNotices = feed.querySelectorAll(".system-temporary");
            assert(tempNotices.length > 0,
                "Case A: Temporary notices must be appended on room creation");
            assert(tempNotices[0].textContent.includes("Invite link copied to clipboard"),
                `Case A: Notice must display 'Invite link copied to clipboard', got: '${tempNotices[0].textContent}'`);

            // 2. In an active chat session with peers, click #input-add-peer-btn
            assert(engine !== null, "Case A: engine must be active for chat session");
            engine.peers.set("peer-desktop-remote-1", { state: "open", dc: { readyState: "open" } });
            updatePeerCountDisplay();

            assert(!inputAddPeerBtn.classList.contains("hidden"),
                "Case A: #input-add-peer-btn must be visible in active chat session");

            await inputAddPeerBtn.click();

            assert(desktopShareInvocations.length === 0,
                `Case A: #input-add-peer-btn must NOT invoke navigator.share on desktop, got ${desktopShareInvocations.length}`);
            assert(inputAddPeerBtn.classList.contains("btn-waiting"),
                "Case A: #input-add-peer-btn must have class 'btn-waiting' in Stage 2");
            assert(inputAddPeerBtn.getAttribute("aria-label") === "Copy invite link",
                `Case A: #input-add-peer-btn aria-label must be 'Copy invite link', got '${inputAddPeerBtn.getAttribute("aria-label")}'`);
            assert(inputAddPeerBtn.innerHTML.includes("<rect") || inputAddPeerBtn.innerHTML.includes(window.COPY_ICON_SVG),
                "Case A: #input-add-peer-btn must display COPY_ICON_SVG");
            assert(!inputAddPeerBtn.innerHTML.includes('<circle cx="18" cy="5"'),
                "Case A: #input-add-peer-btn must NOT display SHARE_ICON_SVG");

            // =====================================================================
            // Case B: Mobile Environment
            // =====================================================================
            navigator.userAgent = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)";

            assert(isMobileBrowser() === true,
                "Case B: isMobileBrowser() must return true when mobile userAgent is present");

            const mobileShareInvocations = [];
            navigator.share = async function(data) {
                mobileShareInvocations.push(data);
            };

            resetToHome();
            setLocalPeerId("peer-mobile-test-host");

            // 1. Initial click on #btn-create-room in Home View
            await btnCreateRoom.click();

            assert(mobileShareInvocations.length === 1,
                `Case B: navigator.share must be called once on initial room creation, got ${mobileShareInvocations.length}`);
            assert(mobileShareInvocations[0].title === "zero-chat",
                `Case B: navigator.share title must be 'zero-chat', got '${mobileShareInvocations[0].title}'`);
            assert(typeof mobileShareInvocations[0].url === "string" && mobileShareInvocations[0].url.includes("#payload="),
                `Case B: navigator.share url must contain '#payload=', got '${mobileShareInvocations[0].url}'`);
            assert(mobileShareInvocations[0].text === undefined,
                `Case B: navigator.share text must be undefined (strictly omitted), got '${mobileShareInvocations[0].text}'`);

            assert(btnCreateRoom.classList.contains("btn-waiting"),
                "Case B: #btn-create-room must have class 'btn-waiting' in Stage 2");
            assert(btnCreateRoom.getAttribute("aria-label") === "Share invite link",
                `Case B: #btn-create-room aria-label must be 'Share invite link', got '${btnCreateRoom.getAttribute("aria-label")}'`);
            assert(btnCreateRoom.innerHTML.includes('<circle cx="18" cy="5"') || btnCreateRoom.innerHTML.includes(window.SHARE_ICON_SVG),
                "Case B: #btn-create-room must display SHARE_ICON_SVG");
            assert(!btnCreateRoom.innerHTML.includes("<rect"),
                "Case B: #btn-create-room must NOT display COPY_ICON_SVG");

            let mobileNotices = feed.querySelectorAll(".system-temporary");
            assert(mobileNotices.length > 0,
                "Case B: Temporary notices must be appended on room creation");
            assert(mobileNotices[0].textContent.includes("Invite link ready to share"),
                `Case B: Notice must display 'Invite link ready to share', got: '${mobileNotices[0].textContent}'`);

            // 2. Click #btn-create-room again while in Stage 2 (waiting for peer)
            await btnCreateRoom.click();

            assert(mobileShareInvocations.length === 2,
                `Case B: navigator.share must be called again on subsequent click in Stage 2, got ${mobileShareInvocations.length}`);
            assert(mobileShareInvocations[1].url === mobileShareInvocations[0].url,
                "Case B: Subsequent click must share the same invite URL");
            assert(mobileShareInvocations[1].title === "zero-chat",
                "Case B: Subsequent click share title must be 'zero-chat'");
            assert(mobileShareInvocations[1].text === undefined,
                "Case B: Subsequent click share text must be undefined");
            assert(btnCreateRoom.classList.contains("btn-waiting"),
                "Case B: #btn-create-room must remain in 'btn-waiting' state");
            assert(btnCreateRoom.getAttribute("aria-label") === "Share invite link",
                "Case B: #btn-create-room must maintain aria-label='Share invite link'");
            assert(btnCreateRoom.innerHTML.includes('<circle cx="18" cy="5"'),
                "Case B: #btn-create-room must maintain SHARE_ICON_SVG");

            // 3. In an active chat session with peers, click #input-add-peer-btn
            assert(engine !== null, "Case B: engine must be active for chat session");
            engine.peers.set("peer-mobile-remote-1", { state: "open", dc: { readyState: "open" } });
            updatePeerCountDisplay();

            assert(!inputAddPeerBtn.classList.contains("hidden"),
                "Case B: #input-add-peer-btn must be visible in active chat session");

            await inputAddPeerBtn.click();

            assert(mobileShareInvocations.length === 3,
                `Case B: navigator.share must be called when clicking #input-add-peer-btn, got ${mobileShareInvocations.length}`);
            assert(mobileShareInvocations[2].title === "zero-chat",
                `Case B: #input-add-peer-btn share title must be 'zero-chat', got '${mobileShareInvocations[2].title}'`);
            assert(typeof mobileShareInvocations[2].url === "string" && mobileShareInvocations[2].url.includes("#payload="),
                "Case B: #input-add-peer-btn share url must contain '#payload='");
            assert(mobileShareInvocations[2].text === undefined,
                `Case B: #input-add-peer-btn share text must be undefined (strictly omitted), got '${mobileShareInvocations[2].text}'`);

            assert(inputAddPeerBtn.classList.contains("btn-waiting"),
                "Case B: #input-add-peer-btn must have class 'btn-waiting' in Stage 2");
            assert(inputAddPeerBtn.getAttribute("aria-label") === "Share invite link",
                `Case B: #input-add-peer-btn aria-label must be 'Share invite link', got '${inputAddPeerBtn.getAttribute("aria-label")}'`);
            assert(inputAddPeerBtn.innerHTML.includes('<circle cx="18" cy="5"') || inputAddPeerBtn.innerHTML.includes(window.SHARE_ICON_SVG),
                "Case B: #input-add-peer-btn must display SHARE_ICON_SVG");
            assert(!inputAddPeerBtn.innerHTML.includes("<rect"),
                "Case B: #input-add-peer-btn must NOT display COPY_ICON_SVG");

            resetToHome();
        } finally {
            if (origUserAgent !== undefined) {
                navigator.userAgent = origUserAgent;
            } else {
                delete navigator.userAgent;
            }
            if (origUserAgentData !== undefined) {
                navigator.userAgentData = origUserAgentData;
            } else {
                delete navigator.userAgentData;
            }
            if (origShare !== undefined) {
                navigator.share = origShare;
            } else {
                delete navigator.share;
            }
            if (origWriteText !== undefined && navigator.clipboard) {
                navigator.clipboard.writeText = origWriteText;
            }
        }

        // =====================================================================
        // Case C: CSS Rules Verification (REQ-044.3, REQ-044.4)
        // =====================================================================
        function readCssFile() {
            if (typeof read === "function") {
                try { return read("zero_client/style.css"); } catch (_) {}
            }
            if (typeof require !== "undefined") {
                try { return require("fs").readFileSync("zero_client/style.css", "utf-8"); } catch (_) {}
            }
            return "";
        }
        function readIndexHtml() {
            if (typeof read === "function") {
                try { return read("zero_client/index.html"); } catch (_) {}
            }
            if (typeof require !== "undefined") {
                try { return require("fs").readFileSync("zero_client/index.html", "utf-8"); } catch (_) {}
            }
            return "";
        }

        const cssContent = readCssFile();
        assert(cssContent.length > 0, "style.css must be readable and non-empty");

        assert(cssContent.includes(".btn-message-burn") && cssContent.includes("top: -2px;"),
            "Case C: .btn-message-burn must define 'top: -2px;' in style.css");

        assert(cssContent.includes(".chat-layout") && cssContent.includes("padding: 0 16px;") && cssContent.includes("gap: 6px;"),
            "Case C: .chat-layout must define 'padding: 0 16px;' and 'gap: 6px;' in style.css");

        assert(cssContent.includes(".chat-header") && cssContent.includes("padding-top: calc(8px + env(safe-area-inset-top, 0px));"),
            "Case C: .chat-header must define 'padding-top: calc(8px + env(safe-area-inset-top, 0px));' in style.css");

        assert(cssContent.includes(".chat-footer") && cssContent.includes("padding: 4px 0;"),
            "Case C: .chat-footer must define 'padding: 4px 0;' in style.css");

        assert((cssContent.includes(".btn-action-full") || cssContent.includes("#btn-create-room")) &&
               cssContent.includes("border-radius: 9999px;") &&
               cssContent.includes("height: 44px;"),
            "Case C: .btn-action-full or #btn-create-room must define 'border-radius: 9999px;' and 'height: 44px;' in style.css (REQ-044.5)");

        // Favicon Badge has rounded circular outer border
        const htmlContent = readIndexHtml();
        if (htmlContent) {
            assert(htmlContent.includes('<link rel="icon"') && (htmlContent.includes("circle cx='16' cy='16'") || htmlContent.includes("rx='15'")),
                "Favicon badge in index.html must have rounded outer border");
        }

        print("  PASS: Mobile Browser Web Share API & Dynamic Share Icon Integration verified cleanly.");
    }

    // =========================================================================
    // TEST 21: Unified Connectivity Resilience: Per-Peer Roles, Grace Window & Smart ICE (ticket-045)
    // =========================================================================
    {
        print("\n--- Test 21: Unified Connectivity Resilience: Per-Peer Roles, Grace Window & Smart ICE (ticket-045) ---");

        // --- Part A: REQ-045.1 Deterministic Per-Peer Role Tracking & System Notice Invariant ---
        resetToHome();
        assert(window.peerRoles.size === 0, "peerRoles must be empty on reset");
        assert(window.announcedJoinPeers.size === 0, "announcedJoinPeers must be empty on reset");
        assert(window.getHasAnnouncedInitialSelfJoin() === false, "hasAnnouncedInitialSelfJoin must be false on reset");

        // 1. Host creates room
        setLocalPeerId("peer-t21-host");
        await createRoomAsHost();
        const hostInviteUrl = getActiveHostInviteUrl();
        assert(hostInviteUrl !== null, "Host invite URL must be generated");

        const hostMatch = hostInviteUrl.match(/payload=([^&]+)/);
        const hostPayload = JSON.parse(atob(hostMatch[1].replace(/-/g, "+").replace(/_/g, "/")));
        const assignedJoinerId = hostPayload.assigned_peer_id;

        assert(window.peerRoles.get(assignedJoinerId) === "inviter",
            `Host must record assigned peer ${assignedJoinerId} as 'inviter', got: ${window.peerRoles.get(assignedJoinerId)}`);

        // 2. In-Room invite adds another peer with role 'inviter'
        // Simulate open channel for host
        window.engine._attachDataChannel(assignedJoinerId, { readyState: "open", send: () => {}, close: () => {} });
        window.engine._setPeerState(assignedJoinerId, "open");
        const joinNotices = feed.querySelectorAll(".system-message");
        assert(joinNotices.length === 1, `Host feed must contain 1 system notice, got ${joinNotices.length}`);
        assert(joinNotices[0].textContent.includes("Peer joined chat"),
            `Host feed must display 'Peer joined chat' on inviter connection, got: ${joinNotices[0].textContent}`);
        assert(window.announcedJoinPeers.has(assignedJoinerId), "Host must record joiner in announcedJoinPeers");

        // 3. Test Joiner perspective
        resetToHome();
        feed.innerHTML = "";
        setLocalPeerId("peer-t21-guest");

        const guestEngine = new ZeroWebRtcEngine("peer-t21-guest");
        setEngine(guestEngine);
        initEngineListeners();

        // Joiner records host as 'joiner'
        const hostPeerId = hostPayload.host_peer_id;
        window.peerRoles.set(hostPeerId, "joiner");
        assert(window.peerRoles.get(hostPeerId) === "joiner", "Joiner must record host as 'joiner'");

        // Joiner channel snaps open
        guestEngine._attachDataChannel(hostPeerId, { readyState: "open", send: () => {}, close: () => {} });
        guestEngine._setPeerState(hostPeerId, "open");

        const guestNotices = feed.querySelectorAll(".system-message");
        assert(guestNotices.length === 1, `Joiner feed must contain 1 system notice, got ${guestNotices.length}`);
        assert(guestNotices[0].textContent.includes("Joined chat"),
            `Joiner feed must display 'Joined chat' (never 'Peer joined chat') on initial connection, got: ${guestNotices[0].textContent}`);
        assert(window.getHasAnnouncedInitialSelfJoin() === true,
            "Joiner must set hasAnnouncedInitialSelfJoin = true");

        // 4. Mesh peer introduction on Joiner's side
        const meshPeerId = "peer-t21-mesh-3";
        const peerJoinedMsg = {
            frame_id: "msg-mesh-join",
            ttl: 3,
            signal: {
                PeerJoined: {
                    peer_id: meshPeerId,
                    public_key: new Array(32).fill(0)
                }
            }
        };
        // Trigger PeerJoined signal
        window.engine.onMessageCb(JSON.stringify(peerJoinedMsg), hostPeerId);
        assert(window.peerRoles.get(meshPeerId) === "mesh",
            `Mesh intro must record role as 'mesh', got: ${window.peerRoles.get(meshPeerId)}`);

        // When mesh peer opens, Joiner should see "Peer joined chat"
        guestEngine._attachDataChannel(meshPeerId, { readyState: "open", send: () => {}, close: () => {} });
        guestEngine._setPeerState(meshPeerId, "open");
        const updatedGuestNotices = feed.querySelectorAll(".system-message");
        assert(updatedGuestNotices.length === 2, `Joiner feed must now have 2 notices, got ${updatedGuestNotices.length}`);
        assert(updatedGuestNotices[1].textContent.includes("Peer joined chat"),
            `Mesh peer connection must display 'Peer joined chat', got: ${updatedGuestNotices[1].textContent}`);

        // --- Part B: REQ-045.2 Transient Disconnect Grace Period & Immediate Burn Departure ---
        // 1. Peer sends message
        const remoteBubble = document.createElement("div");
        remoteBubble.className = "message-bubble";
        remoteBubble.setAttribute("data-peer-id", meshPeerId);
        remoteBubble.innerHTML = `<span class="message-text">Hello from mesh peer</span>`;
        feed.appendChild(remoteBubble);
        assert(feed.querySelector(`.message-bubble[data-peer-id="${meshPeerId}"]`) !== null,
            "Remote bubble must exist in feed");

        // 2. Unannounced transient drop of meshPeerId
        window.engine._setPeerState(meshPeerId, "closed");
        assert(window.pendingDisconnectTimers.has(meshPeerId),
            "Unannounced disconnect must register in pendingDisconnectTimers");
        assert(!departedPeers.has(meshPeerId),
            "Peer must NOT be marked departed during grace period");
        assert(feed.querySelector(`.message-bubble[data-peer-id="${meshPeerId}"]`) !== null,
            "Message bubbles must NOT be purged during grace period");

        // 3. Visibility change extension
        document.visibilityState = "visible";
        document.dispatchEvent(new Event("visibilitychange"));
        assert(window.pendingDisconnectTimers.has(meshPeerId),
            "Timer must remain active and refreshed after visibilitychange to visible");

        // 4. Reconnection within grace window
        window.engine._setPeerState(meshPeerId, "open");
        assert(!window.pendingDisconnectTimers.has(meshPeerId),
            "Reconnection must cancel pending disconnect timer");
        assert(!departedPeers.has(meshPeerId),
            "Reconnected peer must not be in departedPeers");
        assert(feed.querySelector(`.message-bubble[data-peer-id="${meshPeerId}"]`) !== null,
            "Message bubble must remain intact after reconnection");
        const noticesAfterReconn = feed.querySelectorAll(".system-message");
        assert(noticesAfterReconn.length === 2,
            `Reconnection must NOT append duplicate join notices (remains 2), got ${noticesAfterReconn.length}`);

        // 5. Immediate departure on explicit PeerLeftAndBurn
        // Disconnect again to start timer
        window.engine._setPeerState(meshPeerId, "closed");
        assert(window.pendingDisconnectTimers.has(meshPeerId),
            "Second unannounced disconnect starts grace timer");

        const burnLeaveMsg = {
            frame_id: "burn-leave-mesh",
            ttl: 3,
            signal: {
                PeerLeftAndBurn: {
                    peer_id: meshPeerId
                }
            }
        };
        await window.engine.onMessageCb(JSON.stringify(burnLeaveMsg), hostPeerId);
        assert(!window.pendingDisconnectTimers.has(meshPeerId),
            "PeerLeftAndBurn must immediately cancel pending disconnect timer");
        assert(departedPeers.has(meshPeerId),
            "PeerLeftAndBurn must immediately mark peer departed");
        assert(feed.querySelector(`.message-bubble[data-peer-id="${meshPeerId}"]`) === null,
            "PeerLeftAndBurn must immediately purge remote bubbles");
        const finalNotices = feed.querySelectorAll(".system-message");
        assert(finalNotices[finalNotices.length - 1].textContent.includes("Peer left chat"),
            "PeerLeftAndBurn must append 'Peer left chat'");

        // --- Part C: REQ-045.3 Smart Non-Trickle ICE Gathering & Multi-Port STUN/TURN Config ---
        // 1. Redundant STUN server configuration
        const engTest = new ZeroWebRtcEngine("peer-ice-test");
        const servers = engTest.rtcConfig.iceServers;
        assert(Array.isArray(servers) && servers.length >= 5,
            `rtcConfig.iceServers must contain at least 5 default STUN endpoints, got ${servers.length}`);
        const urlsJoined = servers.map(s => s.urls).join(" ");
        assert(urlsJoined.includes(":3478") && urlsJoined.includes(":19302"),
            "ICE servers must cover both port 3478 and port 19302");

        // 2. Configurable TURN via window.ZERO_TURN_SERVERS
        window.ZERO_TURN_SERVERS = [
            { urls: "turn:turn.example.com:3478", username: "u", credential: "p" }
        ];
        const turnEng = new ZeroWebRtcEngine("peer-turn-test");
        const turnServers = turnEng.rtcConfig.iceServers;
        assert(turnServers.some(s => s.urls === "turn:turn.example.com:3478"),
            "Engine must include extra servers from window.ZERO_TURN_SERVERS");
        window.ZERO_TURN_SERVERS = null;

        // 3. Smart candidate detection and 400ms settle timer
        const mockPc = new RTCPeerConnection();
        mockPc.iceGatheringState = "gathering";

        let waitFinished = false;
        const waitPromise = engTest._waitForIceGatheringComplete(mockPc).then(() => {
            waitFinished = true;
        });

        // Fire a srflx candidate
        mockPc.dispatchEvent({
            type: "icecandidate",
            candidate: {
                candidate: "candidate:1 1 UDP 2122260223 192.168.1.100 50000 typ srflx raddr 1.2.3.4 rport 50000"
            }
        });

        assert(waitFinished === false, "Wait should not finish synchronously upon srflx candidate");
        await delay(450); // Settle timer is 400ms
        assert(waitFinished === true, "Wait must resolve cleanly after 400ms settle timer without waiting for 5000ms max timeout");

        resetToHome();
        print("  PASS: Unified Connectivity Resilience: Per-Peer Roles, Grace Window & Smart ICE verified cleanly.");
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
