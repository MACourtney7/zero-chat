# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-012"
title: "UI Tweaks: Centered Session Burn, Green Create Room, Bottom Invite Bar, and Welcome Splash"
state: "PLANNING"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-011"]
created_at: "2026-10-17T16:52:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-012/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
In the current ZeroChat UI:
1. When a session is burned, the banner renders as `🔥 [SESSION BURNED]` and is left-aligned within the security alert card rather than cleanly centered and unadorned.
2. The "Create Private Room" button uses a blue/indigo gradient and includes a leading `✨` emoji (`✨ Create Private Room`), which conflicts with the desire for a clean, emerald-green action styling without emojis.
3. Upon clicking "Create Private Room", the generated invite link with the copy button is displayed in an upper panel while a disabled "Awaiting Peer..." button remains fixed in the bottom footer action slot. The room link with copy button bar should instead replace the bar at the bottom of the view that currently displays "Awaiting Peer...".
4. When a user lands on ZeroChat and is not in an active chat, the central chat container is completely empty and dark. A terse, glassmorphic welcome/info splash card should explain key features (serverless P2P, ChaCha20-Poly1305 E2E encryption, and zero forensic trace/ephemeral lifecycle) in a simple, intuitive manner.

The objective of this task is to implement these visual and layout refinements in `zero_client/index.html` and `zero_client/style.css`, ensuring seamless state transitions between idle, room hosting, active chat, and burned states.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Centered `SESSION BURNED` text in the burn notification banner (no fire emoji, no square brackets).
    - Update "Create Private Room" text to remove `✨` emoji across initial DOM and JS reset/disconnect branches.
    - Reposition / restructure the invite URL and copy button bar to directly replace the bottom footer action bar upon room creation, removing the disabled "Awaiting Peer..." button from the footer.
    - Insert a terse, elegant welcome/info splash component in `#messages-container` when disconnected / idle, disappearing upon active P2P connection or session burn.
  - `zero_client/style.css`:
    - Style `.security-alert-card` and burn alert elements to center text horizontally.
    - Add emerald green styling (`--accent-emerald` gradient/glow) for the "Create Private Room" action button.
    - Footer invite bar styling ensuring identical full-width responsive proportions to the action bars.
    - Styling for the welcome/info splash card (glassmorphic panel, subtle iconography/bullet badges, clean typography).
  - Workspace test suite:
    - Ensure all workspace tests pass (`cargo test --workspace`) and browser assets build cleanly.
- **Out-of-Scope**:
  - WebRTC signaling protocol or Rust cryptographic primitives in `crates/zero_protocol` or `zero_network`.
  - Multi-peer gossip mesh logic in `zero_client/src/mesh.rs`.

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-012.1 (Centered & Unadorned "SESSION BURNED")**: The session burned banner must display exactly `SESSION BURNED` (no square brackets, no fire emoji) and be horizontally centered within its container with clear, high-contrast, alert styling.
- [ ] **REQ-012.2 (Emerald-Green "Create Private Room" Button)**: The "Create Private Room" button must be green-colored (using an emerald gradient and subtle glow) and must not contain the `✨` stars emoji in either HTML or dynamic JavaScript text resets.
- [ ] **REQ-012.3 (Bottom Slot Swap for Room Invite & Copy Bar)**: Upon clicking "Create Private Room", the room invite URL and copy button bar must replace the bottom footer bar (removing the separate "Awaiting Peer..." bottom button). Waiting status is communicated clearly via the status indicator / feed while the copyable invite bar occupies the footer action slot.
- [ ] **REQ-012.4 (Terse Welcome Splash in Inactive View)**: When the user is not in an active chat, the main view must display a terse, beautifully formatted welcome/info splash card highlighting:
  - Direct P2P WebRTC data transmission (no intermediate servers storing chat data).
  - Authenticated End-to-End Encryption (ChaCha20-Poly1305).
  - Zero Trace (no logs, accounts, or persistent storage; closing or burning incinerates all session data).
  - Concise prompt to create a private room or use a shared link.
- [ ] **REQ-012.5 (Lifecycle Transitions & Test Integrity)**: Transitions between idle (welcome splash + green create room button), room hosting (splash/status + bottom copyable invite bar), active chat (message input + send button + burn room bar), and burned state (`SESSION BURNED` + reset button) must be smooth and robust. All workspace tests (`cargo test --workspace`) must pass.

## 4. Architectural Design & Impacted Modules
- `zero_client/index.html`:
  - Footer slot structure: `#create-room-bar`, `#invite-bar` (or unified action slot), and `#message-form`.
  - Welcome splash component inside `#messages-container` rendered when idle.
  - State machine updates in `initEngineListeners()`, `createRoomAsHost()`, `joinRoomFromHash()`, and `burnBtn` click handler.
- `zero_client/style.css`:
  - `.btn-create-room` / `.btn-emerald`: green gradient styles (`#10b981`, `#059669`).
  - `.burn-banner-centered`: `display: flex; justify-content: center; text-align: center;`.
  - `.welcome-splash`: glassmorphic card, feature grid/list, subtle badges.
  - Responsive alignment for bottom invite bar.
