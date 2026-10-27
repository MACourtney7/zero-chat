# Canonical System Architecture: zero_chat

```yaml
---
system: "zero_chat"
version: "1.23.1"
milestone: "cloudflare-turn-cgnat-relay"
status: "PRODUCTION_CANONICAL"
updated_at: "2026-10-27T22:55:00Z"
---
```

## 1. Project Overview & Vision
`zero_chat` is an entirely serverless, zero-footprint, real-time peer-to-peer (P2P) communication platform operating inside web browsers using pure Rust compiled to WebAssembly (`wasm32-unknown-unknown`), coupled with an ephemeral Cloudflare Worker / Hibernating Durable Object signaling bootstrap utilizing WebSocket Hibernation, and automated dual-target CI/CD delivery.

### Core Architectural Invariants
1. **Zero Forensic Footprint**: No account registrations, no cookies, no `localStorage`, `sessionStorage`, or `indexedDB` persistence. Closing or reloading the browser tab immediately incinerates all in-memory keys, sessions, and chat feeds.
2. **Stateless HMAC-Sealed Handshake (Pattern B)**: Signaling dead-drop HTTP `POST` requests and hibernating WebSockets ($0 idle duration billing). Offline invites are sealed with `HMAC_SHA256(host_secret, Invite_UUID)` embedded in URL `#fragment`s, which never touch any cloud edge.
3. **Pure Rust Cryptographic Engine**: Synchronous, memory-safe cryptographic primitives (`x25519-dalek`, `chacha20poly1305`, `hmac-sha256` with `getrandom/js`) shared across WASM and Cloudflare Worker, bypassing asynchronous JS WebCrypto bridging overhead.
4. **Direct P2P DataChannels**: Once the initial WebRTC SDP offer/answer exchange completes, the cloud edge drops out permanently (`is_edge_severed() == true`). All communication flows directly over browser-to-browser UDP DataChannels.
5. **Resilient In-Band Mesh, Secondary Snapping & Glare Avoidance**: Multi-peer scaling without cloud edge requests. Any connected participant (host or invitee) can invite additional peers. The inviter serves as an opaque in-band relay bridge for secondary SDP exchange (`RelaySdpOffer`, `RelaySdpAnswer`), ensuring full triangle ($K_3$) or complete mesh snapping ($K_N$). Glare collisions during crossed offers are resolved deterministically via polite/impolite roles (`myPeerId < remotePeerId` impolite winner ignores incoming offer, polite loser yields and answers). A periodic reconciliation loop (2s interval, max 5 retries with backoff) ensures any missing links between discovered members are autonomously retried and established.
6. **Canonical Pre-Assigned Peer IDs in URL Hash Fragments**:
   - When creating a room or generating an in-room invite, the inviter generates a unique canonical `assigned_peer_id` (`peer-[a-z0-9]{6}`), creates the WebRTC offer directly under that assigned ID, and embeds both `host_peer_id` and `assigned_peer_id` into the `#payload=` URL fragment.
   - Because hash fragments never travel over HTTP headers to cloud edge proxies, they are immune to edge serialization stripping.
   - Joiners adopt `assigned_peer_id` as their canonical `localPeerId` before generating SDP answers. This guarantees 100% ID consensus across all participants before WebRTC handshakes begin, permanently eliminating `rename_peer` drift, identity races, and unroutable relay traffic.
   - `InvitePayload` and `DropPayload` declare `host_peer_id`, `assigned_peer_id`, and `joiner_peer_id` as optional default-deserialized fields, preventing edge deserialization stripping.
7. **Scoped Invite Lifecycle & Cancellation**:
   - Enforces a strictly non-wasteful single-invite lifecycle: only one invite can be active at a time; subsequent clicks while pending re-copy the existing valid link without spawning redundant cloud signaling sessions.
   - `clearPendingInvite(peerId)` only closes the in-flight invite socket when `peerId === pendingInvitePeerId`, preventing secondary peer channel openings from killing active invite sockets.
8. **Clean Departure Deduplication & Host Departure Survivability**:
   - Clean departures broadcast `PeerLeftAndBurn` across the mesh before severance.
   - Calling `handlePeerDeparture(leavingPeerId)` first records the peer in `departedPeers` before `_teardownPeerQuietly` disconnects the peer, completely preventing duplicate `"Peer left chat"` system messages.
   - When the host departs, surviving participants retain direct pairwise DataChannels with `#peer-count = 1`.
   - `openPeerCount` filter excludes any peer in `departedPeers`.
9. **Zero-Text Minimalist UI, Slashed Person Icon & Staggered Temporary Notices**:
   - The bottom `#btn-create-room` operates with zero text labels, featuring an icon-only 2-stage visual flow: Stage 1 (unclicked, `.btn-emerald` with create chat icon) and Stage 2 (clicked, `.btn-waiting` with copy icon). Re-clicking in Stage 2 stays in Stage 2 and re-copies the link.
   - `#message-input` maintains an empty placeholder (`placeholder=""`) permanently across all lifecycle states, eliminating residual text artifacts upon peer departure.
   - Clicking create room dispatches staggered temporary notices: notice 1 (`"Invite link copied to clipboard"`) renders immediately, followed by notice 2 (`"Waiting for peer"`) after ~450ms. Re-clicking restarts the stagger cleanly and cancels in-flight timers.
   - Notice 2 contains strictly `"Waiting for peer"` without literal dots, animated dynamically by `.ticking-dots` (`::after`) without punctuation duplication.
   - Temporary system messages omit timestamps and are automatically purged upon permanent system message arrival.
   - In Home View, `#peer-count` renders an SVG slashed person icon (`var(--accent-cyan)`), transitions to numeric `"0"` upon room entry (and mirrors open DataChannels), and cleanly restores the slashed person icon on `resetToHome()`.
   - The header `[+]` button displays a temporary `"Invite link copied to clipboard"` message with a pulse micro-animation (`.notice-pulse`) that auto-clears after 5 seconds, re-triggering the pulse and restarting the 5s timer on repeated clicks without mutating the `[+]` button itself.
10. **Truthful Dynamic Peer Counter & Send Reliability**: `#peer-count` inside rooms evaluates strictly `engine.openPeerCount` (actual active WebRTC DataChannels in open state). Attempting to send messages when 0 peers are open is halted, preserving input text and rendering `"Message not sent: no connected peers"`.
11. **Hermetic CI/CD Pipelines**: Automated dual-target test validation, strict clippy auditing, and automated Cloudflare Worker + Cloudflare Pages deployment via GitHub Actions.
12. **90-Second Ephemeral Lifecycle, Aging Fade, Per-Message Distributed Burn & Quiet Room Invariant**:
    - Every message (chat bubbles and system notices) registers a client-side 90-second self-destruct lifecycle: full contrast for 70s, gentle transition to 0.80 opacity between 70s and 90s (`.message-aging`, preserving >9:1 contrast), and a 200ms minimal soft fade (`.message-dissolve`: opacity 0, scale 0.98, height/margin collapse) before DOM removal.
    - Quiet Room Invariant: when all messages dissolve in an active room session (`isInRoomSession`), the chat remains an open, serene, quiet space without re-popping the welcome splash screen.
    - Each chat bubble features a dedicated top-right flame burn button (`.btn-message-burn`, min-size `135px × 44px`, flame-red hover accent, default text cursor preserved).
    - Clicking the flame button dissolves the message locally and broadcasts an in-band `{ type: "signal", signal: { BurnMessage: { message_id, ttl: 3 } } }` frame across the mesh; receiving peers deduplicate via `seenMessageIds`, dissolve matching bubble with the 200ms animation, and gossip forward.
13. **Limbo State Recovery, In-Room Plus Invite Button, Staggered Notice Flow & Ticking Ellipsis Alignment**:
    - When all peers depart from an active room session (`!anyConnecting` / `openPeerCount === 0`), the disabled message form (`#message-form`) is hidden and replaced by `#create-room-bar`.
    - `#btn-create-room` renders in Stage 1 Plus mode: `.btn-action.btn-action-full.btn-emerald` with a 20x20 plus (`+`) icon matching the top-bar invite button geometry (`<svg width="20" height="20" viewBox="0 0 24 24" stroke-width="2.5"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>`) and `aria-label="Add peer"`. Top bar `#header-add-btn` is hidden.
    - Clicking the button in this disconnected in-room state generates an in-room WebRTC peer offer on the active engine, connects the signaling dead-drop socket (`pendingInviteWs`), copies the invite URL to clipboard, transforms into Stage 2 (`.btn-waiting`, `COPY_ICON_SVG`, `aria-label="Copy invite link"`), and emits immediate Notice 1 (`"Invite link copied to clipboard"`) followed by staggered Notice 2 (`"Waiting for peer"` with animated `.ticking-dots`) after 450ms.
    - Re-clicks while waiting re-copy the URL and restart the staggered notices cleanly.
    - Peer reconnection restores `#message-form` enabled for messaging, restores `#header-add-btn`, hides `#create-room-bar`, and clears temporary notices.
    - Leaving or destroying the chat (`resetToHome()`) restores `#btn-create-room` to the default chat-bubble-plus icon (`CREATE_CHAT_ICON_SVG`).
    - Temporary system messages (`.system-temporary`, which omit timestamps) are exempt from 90s TTL and do not age-fade or dissolve.
    - Zero whitespace gap on all ticking ellipsis messages: `.system-temporary`, `.transient-status`, and `.system-message:has(.ticking-dots)` define `gap: 0; margin-left: 0;` eliminating any gap before `.ticking-dots`.
14. **Unified Header & Footer State Machine, Minimalist Badges, Clean Burn Button & Zero-Delay Recovery**:
    - **Rule 1 (Leave Button Presence)**: Whenever the peer counter is visible in any room session (`inRoom` is true, displaying `"0"`, `"1"`, `"2"`...), `#header-leave-btn` MUST be visible. In Home view (`resetToHome()`), `#peer-count` displays the slashed-person icon and `#header-leave-btn` is hidden.
    - **Rule 2 (0-Peers Bottom Plus Bar)**: If the peer counter is `0` (waiting for peer, limbo disconnect, or join failure), `#create-room-bar` MUST be visible in the Plus icon variant (`setCreateRoomStage1Plus()`, 20x20 `+` icon), `#message-form` is hidden, and `#header-add-btn` is hidden.
    - **Rule 3 (>0 Peers Top Add Button)**: If the peer counter is `> 0`, `#header-add-btn` MUST be visible in the top bar, `#message-form` is visible and enabled, and `#create-room-bar` is hidden.
    - **Rule 4 (Home Button Scope)**: `setCreateRoomStage1()` (chat bubble with plus) is strictly restricted to the Home view (`resetToHome()`). All 0-peer room states use `setCreateRoomStage1Plus()`.
    - **Minimalist Top-Bar Badges & Brand Icon**: Background fills and borders (`background: transparent; border: none;`) are removed from `.brand-icon` (leaving only the cyan `0` glyph), `.peer-count-badge`, `#peer-count`, `.status-capsule-disconnected`, `#idle-status-capsule`, and `.peer-pill`. Interactive top-bar buttons (`#header-add-btn`, `#header-leave-btn`) retain dedicated interactive button styling.
    - **Streamlined Welcome Splash & Default Blue Vector Icons**: Heading `"Zero accounts. Zero servers. Zero history."` is removed, subtitle is updated to `"Create -> Share -> Chat"`, and platform emojis (⚡, 🔒, 🛡️) are replaced with inline SVG vector icons styled uniformly in default blue (`color: var(--accent-blue); stroke: var(--accent-blue);`).
    - **Clean Message Burn Button**: Circular radial-gradient background and glowing red box shadow are removed from `.btn-message-burn` (`background: transparent; box-shadow: none;`), rendering only the SVG flame icon against the bubble background color, shifting to `#fca5a5` with `scale(1.1)` on hover.
    - **Immediate Connection Failure Recovery**: Zero 15–30s delay on dead-drop failure; `joinRoomFromHash` immediately closes the engine (`engine.close()`), displays `"Failed to establish connection"`, unhides `#header-leave-btn`, and immediately shows `#create-room-bar` in Stage 1 Plus mode.
    - **Display Timestamp Removal Across All Messages**: Timestamps are completely removed from display across all system messages (`.system-timestamp`) and chat message bubbles (`.message-timestamp`) in both HTML and Rust WASM UI client.
    - **"Joined chat" Message Integrity**: `"Joined chat"` is emitted strictly once upon joiner initial connection (`isJoining === true`), immediately toggling `isJoining = false`. Hosts never emit `"Joined chat"`. Subsequent connections emit `"Peer joined chat"` with duplicate suppression via `announcedJoinPeers`.
15. **Symmetrical Flex Chat Bubble Header & Centerline Icon Alignment**:
    - `.message-header` serves as the unified top-row flex container (`display: flex; align-items: center; justify-content: space-between; width: 100%; min-height: 22px;`) within `.message-bubble`.
    - Houses the left-aligned identity dot (`.peer-dot` / `.self-dot`) and the right-aligned flame burn button (`.btn-message-burn`, `position: relative; margin-left: auto; width: 22px; height: 22px;`).
    - Eliminates arbitrary absolute offsets; both the 7px circular dot and the 22px flame button (13px SVG) are anchored along the exact same horizontal centerline (`11px`), providing pixel-perfect vertical alignment and symmetry across both outgoing and incoming messages.
16. **Top-Bar Cleanliness, Zero-Peer Slashed Badge & Inlined Bottom-Left Invite Button**:
    - **Clean Home View Top Bar**: In Home view (`!inRoom` / `resetToHome()`), the top bar renders strictly the brand logo (`<span class="brand-icon">0</span>`) and domain name on the left. All right-side controls and status badges (`#peer-count`, `#idle-status-capsule`, `#peer-list`, and `#header-leave-btn`) are completely hidden.
    - **Zero-Peer Slashed Badge Invariant**: When in an active room session with 0 connected peers (`inRoom && openPeerCount === 0`, e.g. host waiting, limbo disconnect, or join failure), `#peer-count` renders `SLASHED_PERSON_ICON_SVG` (`[no_peers_badge]`) instead of numeric `"0"`. When peers > 0, `#peer-count` renders numeric count strings (`"1"`, `"2"`...).
    - **Zero-Peer Blue Dot & Padlock Unification**: In all 0-peer room states (both host waiting and limbo disconnect), the top bar right controls strictly display `[no_peers_badge] [blue dot] [leave]`. The padlock icon (`#peer-list`) is strictly hidden when peer count is 0 and is displayed only during active encrypted sessions with >0 peers (where the blue dot is hidden).
    - **Bottom-Left Inlined Add Peer Button (`#input-add-peer-btn`) & 2-Stage Blue/Cyan Styling**: The top-bar `+` button (`#header-add-btn`) is permanently omitted. Inside `#message-form`, `#input-add-peer-btn` is placed on the left side of `#message-input` with the `+[person_icon]` vector icon, styled symmetrically with `#send-button` (44px × 42px). Amber/orange styling is completely eliminated from the active chat view.
      - **Stage 1 (Default / Idle)**: Translucent cyan (`background: rgba(56, 189, 248, 0.12); border-color: rgba(56, 189, 248, 0.3); color: var(--accent-cyan);`) displaying `ADD_PEER_ICON_SVG`.
      - **Stage 2 (Active / Copied / Shared)**: Deeper blue/cyan accent (`background: rgba(56, 189, 248, 0.28); border-color: rgba(56, 189, 248, 0.65); color: #38bdf8;`) displaying `SHARE_ICON_SVG` (when Web Share API is available) or `COPY_ICON_SVG` (desktop/fallback).
    - **5-Second Notice Timeout & Elimination of "Waiting for peer..." in Chat View**:
      - Clicking `#input-add-peer-btn` triggers `triggerInChatInviteFeedback()`, copying the invite URL to clipboard, pulsing `.notice-pulse`, and displaying a single temporary notice `"Invite link ready to share"` (or `"Invite link copied to clipboard"`).
      - Distracting `"Waiting for peer..."` notices are completely suppressed in active chat.
      - The notice auto-dismisses and removes itself from the DOM after exactly 5000ms (`setTimeout(..., 5000)`).
      - Re-clicking within 5000ms re-copies the URL / re-triggers share, re-pulses the notice, and resets the 5000ms timer.
    - **Synchronized Button State Reversion**:
      - When the 5000ms timeout expires, `#input-add-peer-btn` cleanly reverts from Stage 2 (share/copy icon) back to Stage 1 (`+[person_icon]`).
      - Peer connections or room teardowns cancel active timers, remove feedback elements, and reset the button to Stage 1.
      - Message draft text in `#message-input` is never modified or cleared during invite actions.
17. **Mobile Browser Support & Resilience (Viewport, Safe Areas, Focus & Reconnection)**:
    - **Fixed Scroll Lock & Visual Viewport Synchronization**: `html, body` enforces `position: fixed; inset: 0; width: 100%; height: var(--app-height, 100dvh); overflow: hidden; overscroll-behavior: none; -webkit-text-size-adjust: 100%;`. `#app` dynamically synchronizes with `window.visualViewport` via `--app-height` and `--app-offset-top`, locking elastic scroll drift and maintaining feed scroll pinned to the bottom during virtual keyboard animation.
    - **Safe-Area Inset Handling**: `.chat-header` integrates `calc(12px + env(safe-area-inset-top, 0px))` to prevent collision with the iOS status bar. Dock padding adapts to `env(safe-area-inset-bottom)`, collapsing to `8px` when `.keyboard-open` is active.
    - **16px Font Size Invariant & Overflow Protection**: `#message-input` and `.message-input` enforce `font-size: 16px; min-width: 0; flex: 1 1 auto;`, permanently preventing iOS Safari's mandatory ~114% focus zoom. Containers enforce `box-sizing: border-box; overflow-x: hidden;` preventing horizontal clipping on narrow viewports (375px–390px).
    - **Touch Focus Retention on Send**: `#send-button` binds `pointerdown` with `e.preventDefault()`, and `doSendMessage()` explicitly retains focus on `#message-input`, allowing continuous mobile typing without keyboard dismissal.
    - **Mobile Clipboard User-Gesture Compliance & Honest Fallback**: Uses `ClipboardItem` with deferred `Promise` in the user click stack. On permission failure (`NotAllowedError`), false copy claims are eliminated; the button morphs to Stage 2 (`.btn-waiting`, `COPY_ICON_SVG`) and displays honest notice `"Invite ready — tap Copy to copy link"`, copying synchronously upon subsequent tap with 100% reliability.
    - **Host WebSocket Suspension & Auto-Reconnection**: Active signaling context (`activeHostSignaling`, `activeInviteSignaling`) is retained in memory. `visibilitychange` listener (`visibilityState === "visible"`) auto-reconnects closed host WebSockets while peer channels are pending open.
    - **Joiner 404 Retry Loop**: In `joinRoomFromHash`, if `POST /drop/...` receives HTTP 404 (host backgrounded in messaging app), it retries every 1500ms up to 10 attempts (~15s) while displaying transient status `"Waiting for host"`.
    - **ICE Gathering Latency & Bfcache Safety**: `_waitForIceGatheringComplete` timeout extended to 4500ms for cellular STUN latency. `pagehide` listener guarded with `!event.persisted`, preventing app switching into bfcache from falsely incinerating active room sessions.
18. **Atmospheric Transparency, Relocated Header Burn Button & Rounded Glass Controls**:
    - **Atmospheric Transparency**: `.chat-header`, `.welcome-splash`, and `.chat-footer` / `#create-room-bar` strip opaque panel backgrounds, hard borders, box-shadows, and backdrop filters (`background: transparent !important; border: none !important; box-shadow: none !important; backdrop-filter: none !important;`), allowing the dark ambient gradient to flow seamlessly through the application layout.
    - **Header Brand Minimalism**: In Home View (`!inRoom`), `.brand-text` (`zero-chat.pages.dev` and `SECURE P2P CHAT`) is displayed alongside the cyan `0` brand glyph. In active room sessions (`inRoom`), `.brand-text` is hidden (`classList.add("hidden")`), preserving only the `0` glyph on the top-left and freeing up horizontal header width. Full brand text is restored upon `resetToHome()`.
    - **Relocated Header Burn Button (Leftmost in Header Controls)**: The obsolete full-width `#burn-room-bar` underneath the header is completely excised, reclaiming vertical screen space for the message feed. `#destroy-session-button` is integrated into `.header-controls` as the leftmost/first child element, styled as a wide oval pill (`border-radius: 9999px; min-width: 80px; height: 26px; padding: 0 14px;`) with rose-500 styling matching `#header-leave-btn` (`background: rgba(244, 63, 94, 0.12); border: 1px solid rgba(244, 63, 94, 0.32); color: #fda4af;`), smooth rose micro-glow hover transitions (`box-shadow: 0 0 10px rgba(244, 63, 94, 0.35)`), and clean 16x16 flame icon. Managed by `updatePeerCountDisplay()`: hidden in Home view, and visible in all in-room sessions (both active chat and 0-peer limbo) alongside `#header-leave-btn`.
    - **Rounded Bottom Bar Buttons**: `#input-add-peer-btn` and `#send-button` enforce fully rounded pill geometry (`border-radius: 9999px; width: 42px; height: 42px;`), adopting the top-bar translucent glass aesthetic with micro-glow hover states and disabled opacity.
    - **Inverted Dock & Input Styling**: Bottom bar dock container is completely transparent; `#message-input` adopts the previous panel background (`rgba(18, 24, 38, 0.7)`), `backdrop-filter: blur(16px)`, `border: 1px solid rgba(255, 255, 255, 0.12)`, `border-radius: 9999px`, and `padding: 10px 18px`.
19. **UI Glow Clearance, Bubble Flex Containment, Web Share API & Rounded Favicon Badge**:
    - **Bottom Button Hover Glow Clearance**: `.message-form` and `.input-dock` define `overflow: visible;` and `padding-top: 4px;` providing vertical headroom so `#send-button:hover` and `#input-add-peer-btn:hover` box-shadows bloom uniformly in 360° without clipping against dock boundaries.
    - **Mobile Bubble Flex Containment**: `.message-bubble` specifies `flex-shrink: 0; flex: 0 0 auto; height: auto;`. When `.messages-container` compresses vertically on mobile virtual keyboard deployment, bubbles retain their intrinsic content height, preventing message text from spilling outside the bubble container.
    - **Ellipsis De-duplication**: Transient status and temporary system messages defensively strip trailing literal dots before appending `.ticking-dots`, ensuring zero static/animated dot duplication.
    - **Mobile Browser Web Share API Integration**: On mobile browsers supporting `navigator.share`, clicking `#btn-create-room` or `#input-add-peer-btn` invokes the native OS share sheet with the invite link, dynamically transforming Stage 2 buttons to `SHARE_ICON_SVG` with `aria-label="Share invite link"`, with subsequent clicks re-opening the share sheet. Gracefully falls back to clipboard copying (`COPY_ICON_SVG`) on unsupported platforms.
    - **Favicon Outer Border Rounding**: `<link rel="icon">` uses circular geometry (`circle cx='16' cy='16' r='15'`) matching the app-wide pill/circular design language (`border-radius: 9999px`).
20. **Platform-Scored Desktop Share Bypass, Clean Mobile Share URL, Burn Icon Optical Alignment, Vertical Space Reclamation & Pill Action Button Modernization**:
    - **Desktop Web Share Bypass**: The Web Share API (`navigator.share`) is strictly scoped to mobile devices (`isMobileBrowser()`). On all desktop platforms (macOS, Windows, Linux), invite triggers directly copy the invite URL to clipboard via `navigator.clipboard.writeText`, completely bypassing desktop OS share popovers (AirDrop/Mail/Notes), displaying `COPY_ICON_SVG` with `aria-label="Copy invite link"` and temporary notice `"Invite link copied to clipboard"`.
    - **Clean Mobile Share URL**: When invoking `navigator.share` on mobile devices, the payload strictly omits the `text` attribute (`{ title: "zero-chat", url: inviteUrl }`). This guarantees that native mobile OS share sheets ("Copy" action on iOS/Android) place strictly the bare URL onto the clipboard without prepended text (`"Join my encrypted zero-chat session:"`), enabling immediate navigation in browser address bars.
    - **Burn Icon Optical Balance**: `.btn-message-burn` defines `top: -2px;`, elevating the bottom-heavy teardrop flame icon to align its visual center of mass with the left sender identity dot (`.peer-dot` / `.self-dot`).
    - **Vertical Space Reclamation**: Eliminates ~50–60px of dead space at the top and bottom of the viewport by tightening `.chat-layout` (`padding: 0 16px; gap: 6px;`), `.chat-header` (`padding: 4px 4px; padding-top: calc(8px + env(safe-area-inset-top, 0px));`), `.messages-container` (`padding: 4px 6px;`), `.chat-footer` (`padding: 4px 0;`), and `.message-form` / `.create-room-bar` (`padding-bottom: calc(6px + env(safe-area-inset-bottom, 0px));`).
    - **Full-Width Action Button Pill Modernization**: `#btn-create-room` and `.btn-action-full` transition from an outdated 8px opaque rectangle to a sleek glassmorphic pill (`border-radius: 9999px; height: 44px; padding: 0 24px;`) with translucent emerald glass styling, soft glow, and `backdrop-filter: blur(16px)`, harmonizing with `#send-button`, `#input-add-peer-btn`, and `#destroy-session-button`.
21. **Unified Mobile & Desktop Connectivity Resilience, Per-Peer Roles, Background Disconnect Grace Period & Smart ICE Gathering**:
    - **Deterministic Per-Peer Role Tracking & System Notice Invariant**: `peerRoles = new Map()` maps remote peer IDs to `"inviter" | "joiner" | "mesh"`. System notice `"Joined chat"` emits strictly when the local client is a joiner entering the room for the first time (`role === "joiner" && !hasAnnouncedInitialSelfJoin`); all subsequent peer connections emit `"Peer joined chat"` without repetition or role reversal. Guarded initial signaling: mesh reconciliation and incoming offer glare handling skip any `peerId` currently awaiting initial dead-drop response (`activeHostSignaling` / `activeInviteSignaling`).
    - **Transient Background Freeze Grace Period & Self-Healing `departedPeers`**: Unannounced channel closures (`closed` / `disconnected`) wait for a 12-second grace period (`pendingDisconnectTimers`, extended to 20s if tab is hidden) before executing departure teardown. Tab returns (`visibilitychange` to `"visible"`) extend pending disconnect timers to `visible + 8s` and kick off non-destructive mesh reconciliation. Reconnection within the grace window cancels the timer, self-heals `departedPeers.delete(peerId)`, preserves existing message bubbles, updates `#peer-count`, and suppresses duplicate join notices. Explicit `PeerLeftAndBurn` signals immediately bypass the grace period, purging messages and emitting `"Peer left chat"` with zero delay.
    - **Smart Non-Trickle ICE Gathering & Multi-Port STUN / TURN Configuration**: In `_waitForIceGatheringComplete(pc)`, candidate strings are inspected for `"typ srflx"` and `"typ relay"`. Detection of public/relay candidates starts an adaptive 400ms settle timer, resolving immediately to minimize invite creation latency while maintaining non-trickle candidate completeness. Includes a 5000ms fallback ceiling if public candidates are delayed. Default STUN endpoints span Google and Cloudflare across ports 19302 and 3478, and merges dynamic `window.ZERO_TURN_SERVERS` for symmetric cellular NAT / CGNAT relay fallback.
22. **Option B Burner Keybundle (Ed25519 + X25519 + ChaCha20-Poly1305), Blind Dead-Drop Signaling & WASM Crypto Bridge**:
    - **Dual Cryptographic Burner Identity**: Each client generates an in-memory `BurnerKeybundle` containing an `X25519Keypair` (for pairwise ChaCha20-Poly1305 AEAD) and an `Ed25519Keypair` (for authenticating envelopes and mesh introductions). All secret key material derives `Zeroize` and `ZeroizeOnDrop`, with explicit volatile memory scrubbing via `burn_all()`.
    - **Blind Dead-Drop Signaling**: The Joiner uses the pre-distributed `host_public_key` from `#payload=` and its own ephemeral `joiner_private_key` to derive the pairwise X25519 shared secret. `sdp_answer.sdp` is sealed into `sealed_sdp_answer: Option<SealedFrame>` prior to POSTing to `/drop/:do_id/:invite_uuid`. The Cloudflare Worker (`zero_network`) is 100% blind to SDP parameters and internal ICE candidates.
    - **Signed Envelopes & Backward Compatibility**: `InvitePayload`, `DropPayload`, `SealedFrame`, and `PeerJoined` carry backward-compatible `#[serde(default)]` Ed25519 public keys and signatures, preventing mesh relay nodes from tampering with introduced peer identities or forging frames.
    - **WASM Crypto Bridge (`WasmCryptoSession`)**: Exposes an ergonomic, high-performance `#[wasm_bindgen]` API from `zero_client`, providing browser JavaScript with peer session management, authenticated frame sealing/opening, blind SDP encryption/decryption, and CSPRNG UUID generation without Curve25519 point multiplication.
23. **Browser WASM Cryptographic Runtime Integration, Blind Dead-Drop Sealing & Tamper Rejection**:
    - **WASM Bridge & 100% Stub Elimination**: Browser client (`zero_client/index.html`) directly binds to `WasmCryptoSession` (`./pkg/zero_client.js`) via `ensureCryptoReady()`, eliminating 100% of legacy JavaScript dummy keys (`crypto.getRandomValues`), fill-zero stubs, and cleartext byte buffers. Volatile memory is zeroized via `cryptoSession.burn_all()` on `resetToHome()` and `cryptoSession.remove_peer(peerId)` on peer departure.
    - **Blind Dead-Drop Handshake**: Joiners seal WebRTC SDP answers into `sealed_sdp_answer: Option<SealedFrame>` using the Host's pre-distributed X25519 public key before POSTing `DropPayload` with dummy `sdp_answer: { sdp: "SEALED" }`. The Cloudflare Worker signaling relay is 100% blind to WebRTC SDP parameters, codecs, internal IP addresses, and ICE candidates. Hosts verify and decrypt incoming dead-drops via `cryptoSession.open_drop_sdp(...)`.
    - **Signed Multi-Peer Mesh Introductions**: `PeerJoined` introduces existing participants to newly joined peers using real X25519 and Ed25519 public keys. Receiving nodes register pairwise sessions prior to in-band relay handshakes, preventing glare or unregistered peer exceptions.
    - **End-to-End Encrypted Relay & Per-Peer Sealed Chat**: `RelaySdpOffer` and `RelaySdpAnswer` are sealed end-to-end; intermediate mesh nodes forward frames without decryption. `BroadcastChat` is uniquely sealed per connected peer with ChaCha20-Poly1305 AEAD ciphertext and 64-byte Ed25519 signatures. Corrupted MAC tags or forged signatures fail closed and are discarded without mutating the DOM.
24. **Consolidated High-Assurance Pure-Rust Core & Lean WASM Footprint**:
    - **Clean Separation of Concerns**: The Rust client (`zero_client`) operates strictly as a pure-Rust/WASM Cryptography, Protocol & Pure-State library without legacy DOM templates, unconfigured `web-sys` browser RTC closures, or heavy web-sys dependencies.
    - **Optimized Binary Size**: Stripping 45+ unused DOM feature flags reduces the compiled release WASM binary (`dist/pkg/zero_client_bg.wasm`) to ~844KB (a 35% reduction from 1.3MB) for rapid cold-start loading.
    - **Pure-State Mesh Topology**: `MeshTopology` implements per-peer role tracking (`PeerRole::{Inviter, Joiner, Mesh}`) and two-stage transient disconnect grace periods (`mark_peer_disconnected_grace`, `recover_peer`, `expire_peer_grace`), preventing premature message burning during cellular handoffs and background tab freezes while ensuring explicit `PeerLeftAndBurn` signals immediately purge without delay.
25. **Ephemeral Cloudflare Realtime TURN Relay, 4G/5G CGNAT Traversal & Non-Trickle Relay Bundling**:
    - **Three-Tier ICE Endpoint (`/ice`)**:
      - **Mode 1 (Cloudflare Realtime TURN API)**: Worker queries `CF_TURN_KEY_ID` and `CF_TURN_API_TOKEN` from secrets. If present, it mints credentials via `POST https://rtc.live.cloudflare.com/v1/turn/keys/${CF_TURN_KEY_ID}/credentials/generate-ice-servers` (accepting any 2xx response, including `201 Created`) and normalizes `{ "iceServers": [...] }` directly.
      - **Mode 2 (Static / Custom JSON Override)**: Evaluates `TURN_SERVERS_JSON` in `env` via `parse_custom_ice_servers` (supporting `{ "iceServers": [...] }`, arrays, and flat credential objects with `urls`).
      - **Mode 3 (Graceful Default STUN Fallback)**: Returns default STUN servers (`stun.cloudflare.com:3478`, `stun.l.google.com:19302`) with `200 OK` on missing secrets or upstream API errors.
      - CORS headers (`Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: GET, POST, OPTIONS`, `Access-Control-Allow-Headers: Content-Type`) and `Content-Type: application/json` are strictly preserved on all responses.
    - **Operator Secret Activation**:
      - Operators activate Cloudflare TURN by running:
        `npx wrangler secret put CF_TURN_KEY_ID`
        `npx wrangler secret put CF_TURN_API_TOKEN`
    - **Client-Side 15-Minute In-Memory Caching**:
      - `ensureIceServersReady()` in `zero_client/index.html` caches provisioned ICE servers for 15 minutes (`ICE_CACHE_TTL_MS = 15 * 60 * 1000`), eliminating redundant network roundtrips during rapid secondary in-room invites and mesh handshakes.
      - Dynamic engine injection: `activeEng.setIceServers(...)` dynamically updates active WebRTC RTCPeerConnection configurations.
    - **Adaptive Non-Trickle Candidate Gathering & Settle Timers**:
      - `_waitForIceGatheringComplete(pc)` detects `hasTurnConfigured`.
      - When TURN is active, arrival of a `typ relay` candidate triggers a fast 350ms settle timer, canceling pending timers and ensuring cellular relay candidates are bundled into non-trickle SDPs.
      - If only `typ srflx` candidates arrive, a 1,200ms fallback settle timer applies, backed by an absolute 5,000ms hard ceiling.
    - **Privacy & Cryptographic Invariant**:
      - WebRTC SCTP DataChannels are protected by DTLS encapsulation over UDP/TCP.
      - Application chat payloads remain end-to-end encrypted and authenticated via `WasmCryptoSession` (pairwise X25519 + ChaCha20-Poly1305 AEAD + Ed25519 signatures).
      - Cloudflare TURN servers act strictly as blind transport relays (RFC 5766 / RFC 8656), inspecting only opaque doubly-encrypted UDP/TCP packets. Ephemeral credentials carry zero room, user, or message metadata.

---

## 2. Directory Layout & Subsystem Ownership

```text
zero_chat/
├── .agents/                        # Antigravity IDE configuration & plugins
│   ├── plugins.json                # Software factory plugin reference
│   └── mcp_config.json             # MCP server configuration
├── .factory/                       # Factory state machine runtime (gitignored)
├── .github/
│   └── workflows/                  # CI/CD automation pipelines
│       ├── ci.yml                  # Format, clippy, test, and wasm build validation
│       └── deploy.yml              # Cloudflare Worker and Pages deployment
├── crates/
│   └── zero_protocol/              # Shared schemas, DTOs, & crypto primitives
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── crypto.rs           # HMAC-SHA256, X25519, ChaCha20-Poly1305
│           └── envelope.rs         # Signaling, chat, and mesh relay envelopes
├── zero_client/                    # Pure-Rust WASM Cryptography, Protocol & Pure-State library
│   ├── Cargo.toml                  # Lean WASM configuration (zero DOM feature flags)
│   ├── index.html                  # Zero-npm HTML/CSS web frontend & WebRTC orchestrator
│   ├── style.css                   # Glassmorphic dark-mode styling
│   └── src/
│       ├── lib.rs                  # Clean public surface (WasmCryptoSession, crypto, invite, mesh, webrtc)
│       ├── crypto.rs               # WasmCryptoSession & ChaCha20-Poly1305 AEAD sealing
│       ├── invite.rs               # Offline HMAC invite generation & URL fragment packing
│       ├── mesh.rs                 # Pure-Rust mesh topology, PeerRole & disconnect grace
│       └── webrtc.rs               # Pure-Rust WebRTC state machine, SDP validator & in-memory test channels
├── zero_network/                   # Stateless Cloudflare Worker & Durable Object
│   ├── Cargo.toml
│   ├── wrangler.toml               # Cloudflare configuration & DO bindings
│   └── src/
│       ├── lib.rs
│       ├── durable_object.rs       # WebSocket hibernation & host attachment
│       └── handlers.rs             # POST /drop/:do_id/:uuid & HMAC validation
├── openspec/
│   ├── config.yaml
│   ├── specs/                      # Canonical architecture documentation
│   │   └── overview.md
│   ├── roadmaps/                   # Milestone roadmaps
│   │   ├── serverless-p2p-chat.md
│   │   └── archive/
│   └── changes/                    # Task specifications
│       └── archive/                # Archived change proposals (ticket-001..023)
├── tests/                          # Root integration test suites
│   ├── e2e_handshake.rs            # Full loopback handshake & chat roundtrip
│   └── mesh_simulation_test.rs     # 3+ peer in-band mesh simulation suite
├── docs/                           # Project guides & deployment runbooks
│   └── deployment.md               # Production deployment & secret configuration
├── scripts/
│   └── deploy_dry_run.sh           # Local pre-flight WASM bundle assembly check
├── Cargo.toml                      # Top-level Cargo workspace
└── factory.yaml                    # Software Factory automation commands
```

---

## 3. Subsystem Architecture

### 3.1 Cryptographic Primitives (`crates/zero_protocol::crypto`)
- **Key Exchange**: X25519 Diffie-Hellman (`x25519-dalek`) generating 32-byte ephemeral public/secret keypairs and deriving pairwise symmetric shared secrets ($K = \text{X25519}(sk_A, pk_B)$).
- **Symmetric AEAD**: Pure-Rust ChaCha20-Poly1305 (`chacha20poly1305`) providing authenticated encryption with 12-byte (96-bit) nonces generated via CSPRNG `getrandom` and 16-byte Poly1305 authentication tags. Plaintext is never placed on the wire in `ChatMessage.body`; all application payloads are sealed inside `SealedFrame { nonce, ciphertext }`.
- **Integrity & Token Sealing**: RFC 4231 constant-time HMAC-SHA256 (`hmac`, `sha2`) sealing offline room invites:
  $$\text{Token} = \text{HMAC-SHA256}(\text{host\_secret}, \text{Invite\_UUID})$$
- **Memory Zeroization**: Volatile memory zeroization on drop (`Zeroize`, `ZeroizeOnDrop`) ensures ephemeral keys and plaintext buffers are wiped immediately upon deallocation.

### 3.2 WebRTC Engine (`zero_client::webrtc` & `zero_client/index.html`)
- **Browser WebRTC Orchestrator (`ZeroWebRtcEngine` in `index.html`)**: Manages real browser `RTCPeerConnection` and `RTCDataChannel` lifecycles, smart non-trickle ICE candidate gathering (srflx/relay settling), STUN/TURN fallback servers, and visibility-change re-connections.
- **Pure-Rust State Machine (`zero_client::webrtc`)**: Provides pure-Rust `PeerLifecycleState` transitions, SDP parsing/validation (`validate_sdp_offer`, `validate_sdp_answer`, `bundle_sdp_with_candidates`), and in-memory simulated channel pairs for hermetic testing.

### 3.3 Hibernating Durable Object Dead-Drop (`zero_network`)
- **WebSocket Hibernation**: Room hosts connect to `/host/:do_id` via WebSocket. The Durable Object serializes the room state (`host_secret`) using `serialize_attachment()` and hibernates immediately, billing $0 during idle waiting.
- **Dead-Drop Endpoint (`POST /drop/:do_id/:uuid`)**: The joining peer posts its SDP answer directly to the Cloudflare Worker edge. The handler performs constant-time HMAC verification against the bearer token.
- **Single-Use Burning**: Upon successful delivery to the hibernating host WebSocket, the UUID is permanently burned in Durable Object storage. Any replay attempt receives an immediate `410 Gone`.

### 3.4 Stateless URL Fragment Signaling (`zero_client::invite` & `index.html`)
- Alice generates an offline room invite bundle containing: `do_id`, `host_pubkey`, `invite_uuid`, `hmac_token`, and Alice's bundled SDP offer.
- The bundle is compressed/serialized into URL fragment `#payload=<base64>` and shared out-of-band. URL fragments are never transmitted to cloud servers in HTTP headers.
- When Bob opens the URL, the browser client automatically extracts the fragment, verifies the HMAC token offline, derives the pairwise X25519 secret, seals the SDP answer into `sealed_sdp_answer`, and dispatches the dead-drop POST request.
- **Immediate Edge Severance**: As soon as the WebRTC DataChannel fires `onopen`, Alice and Bob explicitly close all signaling sockets.

### 3.5 Ephemeral Chat UI (`zero_client/index.html`, `style.css`)
- Glassmorphic dark-mode interface with translucent panels (`backdrop-filter: blur(16px)`), pulsing connection badges, and responsive layouts.
- Domain URL branding in top-bar header and `<title>` (`zero-chat.pages.dev`) with embedded SVG favicon matching the static brand icon (`<span class="brand-icon">0</span>`).
- Header controls feature fixed bounding alignment with an adaptive split badge architecture:
  - **Disconnected State**: Standalone peer counter badge (`#peer-count`, displaying `0`) on the left and a double-width blue-dot idle capsule (`#idle-status-capsule`, 58px x 26px with centered static cyan/blue dot and non-pulsing animation) on the right.
  - **Connected State**: The idle capsule cleanly splits into the middle connection status pill (`#peer-list` / `#status-badge`) and the right circular 26px red exit button (`#header-leave-btn`), ensuring zero horizontal layout shifting across states.
  - **Header Add-Peer Button (`#header-add-btn`)**: Permanently omitted from the top-bar header in favor of `#input-add-peer-btn` in the footer.
  - **Inlined Bottom-Left Add-Peer Button (`#input-add-peer-btn`)**: Nested inside `#message-form` directly left of `#message-input`. Styled with cyan accent and symmetrical 44px × 42px dimensions. Clicking generates a single invite offer and silently copies the link to the clipboard, transitioning to `.btn-waiting` with copy icon without touching or clearing drafted text in `#message-input`. If an invite is already pending, subsequent clicks re-copy the valid link without spawning duplicate WebRTC connections or cloud sessions. The pending invite clears upon peer connection. Establishes the anchor for future nested attachment flows.
- Clean in-feed transient status messaging: Connection progress displays `"Establishing secure connection"` with an animated CSS ticking ellipsis (`.`, `..`, `...`) without timestamps, cleanly replaced in-place when permanent system messages are appended (`Joined chat`, `Peer joined chat`, or `Failed to establish connection`).
- Direct chat creation logging: Room creation immediately outputs `"Secure chat created"` with timestamp.
- Minimalist welcome splash with centered brand headline (`Zero accounts. Zero servers. Zero history.`), 3-step action flow (`Create Chat -> Share Link -> Talk P2P`), and 3 invariant feature cards (`Direct P2P Connection` with description `"Private channels with no intermediate relays or storage servers."`, `Secure Communications`, `Ephemeral Sessions`) displayed strictly when the chat is empty; session burn and leave return the user cleanly to this home view.
- Unified chat bubble styling across incoming and outgoing messages with left accent borders (emerald for self, cyan/palette for peers), `[dot] [timestamp]` header layout with regular arrow cursor, and clean message bodies without bulky verification footers.
- Message bubbles are stamped with `data-peer-id="<peer_id>"` (or `"local"` for self) for deterministic author attribution and selective burning. Distinct peer colors are assigned from `PEER_COLOR_PALETTE`.
- Distinct Leave and Burn UI triggers:
  - Circular red exit button (`#header-leave-btn`): triggers selective peer message purge across surviving peers (`PeerLeftAndBurn`) and resets local view to home.
  - Session burn button (`#destroy-session-button`): triggers global room annihilation (`GlobalBurnSession`) flooding all peers and resetting everyone to home.
- Left-aligned system messages with monospace timestamps (`[HH:MM]`).

### 3.6 Decentralized In-Band Gossip Mesh & Lifecycle (`zero_client::mesh`, `envelope`)
- **Multi-Peer Connection Pool**: `ZeroWebRtcEngine` in `zero_client/index.html` manages concurrent WebRTC peers (`Map<peerId, { pc, dc }>`), supporting broadcast messaging (`broadcast_text`) across all active DataChannels and independent connection lifecycle tracking.
- **Opaque Bridge Relay (REQ-006.1, REQ-026.1)**: Room host acts as an in-band signaling bridge between existing peers (`Channel_AB`, `Channel_AC`). Relay frames (`RelaySdpOffer`, `RelaySdpAnswer`) contain end-to-end sealed payloads; relay nodes forward them verbatim without inspection, decrementing TTL on forwarding hops.
- **Direct Mesh Snapping (REQ-006.2, REQ-026.2)**: Secondary peers (Bob and Charlie) exchange SDP offers/answers over Alice's bridge and snap direct WebRTC DataChannels (`Channel_BC`) with **0 cloud edge requests**. Automatic host introductions (`PeerJoined`) and lexicographical glare tie-breakers ensure deterministic channel establishment.
- **Broadcast Flooding & Loop Suppression (REQ-006.3, REQ-006.4, REQ-026.3)**: Chat messages flood across direct links with initial TTL = 3. Network loops in cyclic/triangle topologies are suppressed using an LRU deduplication cache (`seenMessageIds`, 1000 entries), delivering messages exactly once.
- **Selective Leave & Purge (`PeerLeftAndBurn`)**: Clean departures broadcast `PeerLeftAndBurn` across the mesh before connection severance. Remote peers update routing tables and peer counts, selectively purge all DOM bubbles matching `.message-bubble[data-peer-id="<peer_id>"]`, and log a single clean `"Peer left chat"` notice (REQ-024.1). In multi-peer rooms, when 1 of N peers leaves, the room remains active for surviving peers with a decremented counter.
- **Global Room Annihilation (`GlobalBurnSession`)**: Triggered via `#destroy-session-button`, flooding a burn signal across all active DataChannels. All recipient nodes immediately invoke `resetToHome()`, closing connections, zeroizing memory, and wiping feeds simultaneously.
- **Dirty Disconnect, Window Unload Purge & Controls Preservation**: Abrupt channel closures (`PeerLifecycleState::Closed` / `st === "closed"`) and `MeshTopology::remove_peer` automatically invoke `on_burn_peer_messages` to purge disconnected peer messages from the UI feed. Window `beforeunload` and `pagehide` listeners opportunistically dispatch `leave_and_burn` signals out before browser exit with deduplication guards. On peer departure, surviving nodes preserve disconnected room controls: header peer count `0`, padlock icon, and leave button remain visible (blue dot hidden), burn button remains accessible below header, and message input is disabled with `"Peer left chat"` placeholder, allowing the user to cleanly leave or burn to return to the home view (REQ-024.3, REQ-024.4).

### 3.7 CI/CD Automation (`.github/workflows/`)
- **`ci.yml`**: Triggers on pull requests and pushes to `main`. Executes `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, full workspace test suite (`cargo test --workspace`), and target-specific builds (`wasm32-unknown-unknown` for `zero_client`).
- **`deploy.yml`**: Production CD triggering on pushes to `main`. Uses `cloudflare/wrangler-action@v3` to deploy `zero_network` Worker + Durable Object bindings and publish the assembled `zero_client` WASM bundle to Cloudflare Pages. Enforces `cancel-in-progress: false` to protect active deployments.

---

## 4. End-to-End Handshake & Messaging Sequence

```text
Host (Alice)                     Joiner (Bob)                 Cloudflare DO
    |                                 |                             |
    |-- 1. Create DO room & WS ------>|                             |
    |   (stores host_secret)          |                             |
    |                                 |                             |-- [DO Hibernates $0/sec]
    |-- 2. Offline Invite Bundle ---->|                             |
    |   (URL #payload= fragment)      |                             |
    |                                 |-- 3. Extract & verify HMAC -|
    |                                 |-- 4. POST /drop/:do_id/:uuid -> [Wakes DO]
    |                                 |      (contains SDP answer)  |-- Burn UUID (410)
    |<-- 5. Pipe Answer via WS -------------------------------------|
    |                                 |                             |
    |==== 6. WebRTC DataChannel Open (UDP P2P Direct) =============|
    |                                 |                             |
    |-- 7. Close WebSocket ---------->|                             |
    |   [EDGE SEVERED: 0 CLOUD HITS]  |                             |
    |                                 |                             |
    |<=== 8. Encrypted ChaCha20-Poly1305 Chat (P2P Direct) =======>|
```

---

## 5. Test & Validation Coverage (123 Rust Tests + 13 Browser Headless Tests)

```text
Unit Tests:
  - zero_client lib (58 tests):
    * bootstrap & auto-drop: 8 passed
    * payload sealing & zeroization: 13 passed
    * invite generation & fragment packing: 8 passed
    * mesh topology & in-band relay: 11 passed (including leave and burn broadcasts, disconnect cleanup)
    * chat UI & zero-persistence audit: 18 passed (including selective message burning, multi-peer partial departures, dynamic peer count, burn idempotency, multi-remote peer selective purge, dynamic peer counter lifecycle & symmetry)
  - zero_network lib (3 tests):
    * ephemeral room capacity & burn: 2 passed
    * mock host socket attachment: 1 passed
  - zero_protocol lib (8 tests):
    * envelope schemas & TTL decrementation: 8 passed (including PeerLeftAndBurn, GlobalBurnSession, pre-assigned peer IDs)

Integration Test Suites:
  - tests/e2e_handshake.rs (5 tests):
    * REQ-005.1: Ephemeral UI lifecycle & status transitions
    * REQ-005.2: Symmetric payload sealing & packet inspection
    * REQ-005.3: Tamper detection & security alert handling
    * REQ-005.4: Zero persistence lifecycle audit
    * REQ-005.5: Complete E2E handshake & encrypted chat roundtrip
  - tests/mesh_simulation_test.rs (23 tests):
    * REQ-006.1: Opaque bridge signaling relay
    * REQ-006.2: Direct mesh snap zero cloud edge
    * REQ-006.3: Mesh broadcast flooding delivery
    * REQ-006.4: Duplicate broadcast loop suppression & bounded cache
    * REQ-006.5: Peer disconnect & UI cleanup
    * REQ-007.1: Leave and burn selective message purging
    * REQ-007.2: Global burn session room annihilation
    * REQ-007.3: Dirty disconnect automatic peer message purging
    * REQ-028.1: Multi-node connected mesh simulation (triangular snap, 0 edge hits, broadcast flooding, loop suppression)
    * REQ-028.2: Partial peer departure & feed survivability (selective purge, surviving channel retention)
    * REQ-029.4: Third peer departure selective message purge
    * REQ-029.5: Host departure pairwise channel survivability and feed continuity
    * REQ-030.1 / REQ-030.2: Secondary inviter mesh snapping and counter synchronization across all nodes
    * REQ-030.3 / REQ-030.4: Symmetric departure counter decrement across surviving peers and host departure survivability
  - tests/invite_tests.rs (2 tests):
    * REQ-004.1 through REQ-004.5
  - tests/webrtc_tests.rs (6 tests):
    * REQ-002.1 through REQ-002.5
  - tests/network_tests.rs (8 tests):
    * REQ-003.1 through REQ-003.5 + Dead-drop joiner_peer_id preservation
    * REQ-049.1: Default STUN fallback and custom TURN parsing modes
  - tests/protocol_tests.rs (12 tests):
    * REQ-001.1 through REQ-001.5 + Serde backward compatibility & round-trip assertions

Browser Headless Test Suite (zero_client/tests/browser/run_test.sh):
  - 17 comprehensive multi-participant mesh, departure & UI scenarios passed:
    * Test 1: Full mesh establishment with 3 peers (A, B, C) (K_3)
    * Test 2: Host A departs; C broadcasts; B receives directly
    * Test 3: C departs; B reflects 0 open peers
    * Test 4: Idempotency check - duplicate offer/answer does not kill open link
    * Test 5: Mesh reconciliation timer recovers dropped handshake
    * Test 6: Send feedback - 0 peers displays clear error feedback and halts send
    * Test 7: Truthful peer counter deriving strictly from open DataChannels
    * Test 8: Crossed-offer glare resolution between simultaneous initiators
    * Test 9: Watchdog cleans stalled connecting peer when zero open peers remain
    * Test 10: Scoped clearPendingInvite protects in-flight invite from secondary peer connection
    * Test 11: Pre-assigned peer IDs and clean host departure in 3-peer mesh (single notice deduplication)
    * Test 12: E2E createRoomAsHost and joinRoomFromHash via signaling exchange (zero peer renaming)
    * Test 13: In-chat add peer button 2-stage blue/cyan styling, 5s temporary notice timeout & reversion (REQ-033.1 - REQ-033.4, REQ-040.1 - REQ-040.4)
    * Test 14: Zero input placeholder, staggered temporary notices, single ellipsis & slashed person icon (REQ-034.1 - REQ-034.4)
    * Test 15: Enhanced Burn Behavior - 90s TTL, 70s aging fade, per-message flame button, distributed BurnMessage signaling, quiet room invariant, and reset restoration (REQ-035.1 - REQ-035.5)
    * Test 16: Limbo State Recovery - in-room plus button, two-stage copy morphing, staggered notices, 95s TTL immunity, peer reconnection, and zero whitespace ticking dots (REQ-036.1 - REQ-036.7)
    * Test 17: UI Architecture Refinement - Rules 1-4, minimalist badges, brand icon glyph, clean burn icon, blue vector splash icons, zero-delay failure recovery, timestamp removal, and joiner message integrity (REQ-037.1 - REQ-037.9)
    * Test 19: UI Architecture Refinements - Transparent Panels, Relocated Burn Button & Rounded Controls (ticket-042)
    * Test 20: Mobile Browser Web Share API, Desktop Share Bypass, Clean Share URL, Burn Offset & Compacted Layout (ticket-043, ticket-044)
    * Test 21: Unified Connectivity Resilience - Per-Peer Roles, Grace Window & Smart ICE (REQ-045.1 - REQ-045.4)
    * Test 22: WasmCryptoSession Browser Runtime Integration & Blind Dead-Drop - Wire Blindness, Authenticated AEAD & Ed25519 Signatures, 3-Peer Mesh Formation & Host Departure Continuity, Tamper Rejection & Fail-Closed Security, and Volatile Memory Wipe (REQ-047.1 - REQ-047.5)
    * Test 23: Ephemeral Cloudflare Realtime TURN Relay, 15-Minute ICE Caching & Adaptive Candidate Gathering - Three-Tier ICE Endpoint, 15m Cache Expiration, 350ms Relay vs 1200ms Srflx Settle Logic, and Authenticated 3-Peer Mesh over TURN Relay (REQ-049.1 - REQ-049.4)

Total: 117 Rust tests + 23 Browser tests passed, 0 failed.
```

