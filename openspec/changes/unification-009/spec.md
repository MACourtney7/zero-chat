# Change Proposal: unification-009

```yaml
---
ticket_id: "unification-009"
title: "Extract Reusable ZeroChatWidget from zero_chat and Mount Inside virtual_dog_walker Proximity Chat"
state: "IN_PROGRESS"
created_at: "2026-11-03T20:15:00Z"
target_repos:
  - "../zero_chat"
  - "../virtual_dog_walker"
max_review_cycles: 3
current_review_cycle: 0
---
```

## 1. Problem Statement & Motivation
Currently, chat UI and message presentation logic are duplicated with disjoint lifecycles across the workspace:
1. In `zero_chat`, the glassmorphic chat interface, 70s/90s ephemeral aging/dissolve lifecycle, per-message flame burn button (`BurnMessage` signal), peer-color dots (`PEER_COLOR_PALETTE`), padlock/peer-count header pill, and WebRTC/WasmCryptoSession message handling live inside an inline `<script type="module">` in `zero_client/index.html` and global rules in `zero_client/style.css`.
2. In `virtual_dog_walker`, `src/ui/proximity_chat.js` combines two distinct concerns:
   - Spatial proximity sensing & hysteresis (160px enter, 200px exit, 5-minute retention).
   - A bespoke, duplicate chat DOM (`#proximity-chat-panel`, `#proximity-chat-messages`, `#proximity-chat-input`) that lacks `zero_chat`'s glassmorphic styling, per-message flame burn animation, ephemeral aging, and distributed burn mechanics.
3. Note: `zero_client/tests/browser/run_test.py` regex-extracts the inline `<script>` from `zero_client/index.html` to run 24 headless browser tests. Any modularization must preserve `index.html`'s inline script structure so zero test regressions occur in `zero_chat`.

---

## 2. Architecture & Scope of Changes

### 2.1 Reusable `ZeroChatWidget` in `zero_chat` (`zero_client/widget/`)
- Create `zero_client/widget/zero_chat_widget.js`:
  - Export `ZeroChatWidget` supporting `mode: 'standalone'` and `mode: 'embedded'`.
  - Supports `{ container, mode, onSendMessage, onBurnMessage, onBurnSession, agingMs = 70000, dissolveMs = 90000 }`.
  - Renders glassmorphic DOM with both `zero_chat` class names (`.message-bubble`, `.message-outgoing`, `.message-incoming`, `.peer-dot`, `.btn-message-burn`, `.peer-pill`, `.peer-lock-icon`, `.btn-send-emerald`) and backward-compatible IDs (`#proximity-chat-status`, `#proximity-chat-messages`, `#proximity-chat-form`, `#proximity-chat-input`, `#proximity-chat-send`).
  - Implements:
    - `appendMessage({ senderId, name, text, isSelf, messageUuid, timestamp })` with 70s/90s aging and dissolve timers.
    - `dissolveMessageById(messageUuid)`: triggers `.message-dissolve` and removes from DOM/state.
    - `burnPeerMessages(peerId)`: selectively purges messages by peer.
    - `burnAllMessages()`: clears all messages.
    - `setPeers(peerIds, peerCount)`: updates padlock pill and peer badge.
- Create `zero_client/widget/zero_chat_widget.css`:
  - Scoped glassmorphic styles under `.zero-chat-embed` ported from `zero_client/style.css`.
- Update `scripts/deploy_dry_run.sh` to package `zero_client/widget/` into `dist/widget/`.

### 2.2 Build Pipeline Sync & ProximityChat Mounting (`virtual_dog_walker`)
- In `scripts/build_wasm.sh` and `scripts/build_pages.sh`:
  - Automatically copy `../zero_chat/zero_client/widget/zero_chat_widget.js` to `src/ui/zero_chat_widget.js` and `zero_chat_widget.css` to `src/ui/zero_chat_widget.css`.
- In `index.html`:
  - Link `src/ui/zero_chat_widget.css`.
- In `src/ui/proximity_chat.js`:
  - Separate spatial proximity tracking from presentation: ProximityChat remains the spatial controller, mounting `ZeroChatWidget` inside `#proximity-chat-panel`.
  - Preserve all existing public methods and properties (`radius`, `joinRadius`, `activeCluster`, `messages`, `update()`, `sendMessage()`, `receiveMessage()`, `getMessages()`, `getActivePeers()`, `clearMessages()`, `destroy()`).
  - Wire distributed burns:
    - Per-message flame click -> `onBurnMessage(messageUuid)` -> `net.burnProximityMessage(messageUuid)`.
    - Header session burn -> `onBurnSession()` -> `net.burnProximitySession()`.

### 2.3 Distributed Proximity Message Burn (`src/net/zero_mesh_network.js`)
- Implement `burnProximityMessage(messageUuid, peers)`:
  - Broadcasts `{ type: 'PROXIMITY_BURN_MSG', messageUuid, senderId: this.peerId }` over `proximityMesh` / `gameMesh.send_direct_state`.
- Implement `burnProximitySession(peers)`:
  - Broadcasts `{ type: 'PROXIMITY_BURN_ALL', senderId: this.peerId }`.
- In `handlePacket` / `_onGameMeshState`:
  - Handle `PROXIMITY_BURN_MSG` and `PROXIMITY_BURN_ALL`, delegating to `CliftonDownGame` -> `proximityChat.burnMessage(messageUuid)` and `proximityChat.clearMessages()`.

### 2.4 Verification Suites
- Add `test_unified_zero_chat_widget_mounted_in_proximity_chat` in `virtual_dog_walker/test/test_zero_mesh_integration.py`.
- Run `bash zero_client/tests/browser/run_test.sh` in `zero_chat` (all 24 tests).
- Run `cargo test` across all 4 repositories.
- Run `python3 test/run_tests.py` in `virtual_dog_walker` (all 22 suites).

---

## 3. Acceptance Criteria

- **AC-009.1 (Single Source of Truth in zero_chat)**: `zero_chat/zero_client/widget/zero_chat_widget.js` and `zero_chat_widget.css` encapsulate `zero_chat` glassmorphic UI, .message-bubble rendering, .peer-dot colors, .btn-message-burn per-message flame burn, and 70s/90s aging/dissolve lifecycle, synced into `virtual_dog_walker` via build scripts.
- **AC-009.2 (Unified In-Game Chat Experience)**: Proximity chat in `virtual_dog_walker` mounts `ZeroChatWidget` inside `#proximity-chat-panel`, providing matching visual styling, padlock badge, per-message flame burn button, and session burn.
- **AC-009.3 (Zero Test Regressions)**: All 24 browser scenarios in `zero_chat` pass, all Rust workspace tests pass, and all Python test suites in `virtual_dog_walker` pass 100%.

