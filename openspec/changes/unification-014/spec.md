---
ticket_id: "unification-014"
title: "Phase 2: Mobile UI, Scaling & Contextual Input Polish"
state: "IN_PROGRESS"
created_at: "2026-11-06T22:50:00Z"
target_repos:
  - "../virtual_dog_walker"
  - "../zero_chat"
max_review_cycles: 3
---

# Change Proposal: unification-014 — Phase 2: Mobile UI, Scaling & Contextual Input Polish

## 1. Audit Findings & Problem Statement

Inspecting `src/main.js`, `src/world/field.js`, `src/ui/touch_controls.js`, `src/ui/proximity_chat.js`, and `src/style.css` highlights three primary friction points affecting mobile browser gameplay on smartphones and tablets:

### 1.1 DPI / Retina Canvas Scaling & Dynamic Viewport Zoom
- **Soft Rendering on High-DPI Displays**: `initCanvas()` in `src/main.js:220` currently sets `canvas.width = window.innerWidth` and `canvas.height = window.innerHeight` at 1x CSS pixel ratio. On modern smartphones (Retina displays with `devicePixelRatio` 2.0 or 3.0), the browser stretches this low-resolution backing canvas over high-density physical pixels, causing sprites, lines, and nametag typography to render noticeably soft and fuzzy.
- **Narrow Portrait Viewport Tunnel Vision**: In `src/world/field.js:3`, `Camera` renders 1:1 world pixels. A 380px wide mobile portrait screen displays only ~11% of the 3200px park width, whereas a 1920px desktop monitor sees ~60%. Players on mobile feel cramped with minimal advance visibility of oncoming dogs, bins, or poop piles. Adding `devicePixelRatio` buffer scaling plus responsive camera zoom (0.75x–0.85x on narrow viewports) instantly makes the park feel crisp and spacious.
- **Coordinate Projection Coupling**: `Camera.worldToScreen` and `Camera.screenToWorld` do not currently account for zoom. Introducing zoom requires adjusting coordinate projection math so that touch clicks, ball throws, and entity taps continue to map 1:1 with world coordinates.

### 1.2 Missing Touch Button for Waste Pickup / Bin Disposal
- **Sanitation Ergonomics Gap**: On desktop, pressing `[E]` contextually interacts with the world: picks up dog waste if near a poop pile, disposes of it if near a bin, or pets the dog if near Ziggy.
- **Mobile Pain Point**: On mobile (`touch_controls.js`), there is no `[E]` button on the action pad (`btn-touch-throw`, `btn-touch-whistle`, `btn-touch-pet`, `btn-touch-leash`, `btn-touch-chat`). Mobile players are forced to tap the tiny 40px poop pile sprite on the moving canvas.
- **Misleading In-World Badges**: In `src/main.js:1136`, `drawInteractionPrompt` draws text prompts explicitly saying `[E] Pick up waste` and `[E] Dispose in bin`, which is confusing on touch devices without a physical keyboard.
- **Solution**: The `❤️` Pet button on the touch action pad should dynamically morph into a glowing `💩` (Pick Up Waste) or `🗑️` (Dispose in Bin) contextual action button whenever the player enters proximity (< 40px) of waste or a bin, and the in-world interaction badges should adapt their text for touch devices.

### 1.3 Virtual Keyboard Viewport Handling in Proximity Chat
- **Keyboard Clipping**: When a mobile player taps into the chat input bar within `#proximity-chat-panel`, the on-screen virtual keyboard slides up. On iOS Safari and Android Chrome, `window.visualViewport.height` shrinks (often by 300–400px), but standard `100vh` remains unchanged.
- **Input Field Obscuration**: `#proximity-chat-panel` in `src/style.css` currently uses `max-height: calc(100vh - 100px)`. As a result, the bottom of the drawer—where the form and text input field live—gets pushed down below the top of the virtual keyboard, obscuring what the user is typing.
- **Solution**: Bind `window.visualViewport` resize and scroll listeners to dynamically anchor `#proximity-chat-panel` height and max-height to `visualViewport.height`, ensuring the input bar remains pinned right above the keyboard.

---

## 2. Technical Architecture & Detailed Scope

### 2.1 Track 1: High-DPI Canvas Scaling & Dynamic Camera Zoom (`src/main.js`, `src/world/field.js`)
1. **DPI Backing Buffer**:
   - In `initCanvas()` (`src/main.js`), determine DPR: `const dpr = Math.min(window.devicePixelRatio || 1, 3.0);`.
   - Set canvas backing buffer dimensions:
     ```javascript
     this.canvas.width = Math.round(width * dpr);
     this.canvas.height = Math.round(height * dpr);
     this.canvas.style.width = `${width}px`;
     this.canvas.style.height = `${height}px`;
     ```
   - Store `this.dpr = dpr`.
   - In `render()`, reset and apply DPR transform:
     ```javascript
     this.ctx.setTransform(1, 0, 0, 1, 0, 0);
     this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
     this.ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
     ```
     This allows all existing rendering operations (field, entities, environment, navigation arrow) to operate in logical CSS pixel space (`0..viewportWidth`, `0..viewportHeight`) while being rendered with native Retina sharpness.

2. **Responsive Camera Zoom**:
   - In `src/world/field.js:Camera`:
     - Add `zoom = 1.0` to constructor options/properties.
     - Add `resize(width, height, zoom = 1.0)` support.
     - Update `apply(ctx)`:
       ```javascript
       ctx.save();
       ctx.scale(this.zoom, this.zoom);
       ctx.translate(-Math.round(this.x), -Math.round(this.y));
       ```
     - Update `worldToScreen(worldX, worldY)`:
       ```javascript
       return {
         x: (worldX - this.x) * this.zoom,
         y: (worldY - this.y) * this.zoom
       };
       ```
     - Update `screenToWorld(screenX, screenY)`:
       ```javascript
       const rawX = screenX / this.zoom + this.x;
       const rawY = screenY / this.zoom + this.y;
       return {
         x: Math.max(0, Math.min(this.worldWidth, rawX)),
         y: Math.max(0, Math.min(this.worldHeight, rawY))
       };
       ```
     - Update `update(dt)`:
       - Calculate target centering taking zoom into account:
         ```javascript
         const viewWorldW = this.viewportWidth / this.zoom;
         const viewWorldH = this.viewportHeight / this.zoom;
         const targetCenterX = this.target.x - viewWorldW / 2;
         const targetCenterY = this.target.y - viewWorldH / 2;
         ```
       - Clamp `this.x` and `this.y` against `this.worldWidth - viewWorldW` and `this.worldHeight - viewWorldH`.
   - In `src/main.js`:
     - Compute responsive zoom in `resize()`:
       - If `width < 600px`: `zoom = Math.max(0.75, Math.min(0.85, 0.75 + (width / 600) * 0.10))` (~0.78x–0.85x).
       - If `600px <= width < 900px`: `zoom = 0.88x`.
       - If `width >= 900px`: `zoom = 1.0x`.
     - Pass `zoom` to `this.camera.resize(width, height, zoom)`.

### 2.2 Track 2: Contextual Mobile Sanitation Action Button & Prompts (`src/ui/touch_controls.js`, `src/main.js`, `src/style.css`)
1. **Touch Controls Morphing**:
   - In `src/ui/touch_controls.js`:
     - Add method `setContextualAction(action)` where `action` can be `null`, `'pet'`, `'pickup'`, or `'dispose'`.
     - Maintain state `this.contextualAction = action`.
     - Update DOM element `btn-touch-pet`:
       - If `'pickup'`: text/emoji `💩`, `aria-label: "Pick up dog waste"`, add CSS class `contextual-pickup`.
       - If `'dispose'`: text/emoji `🗑️`, `aria-label: "Dispose waste in bin"`, add CSS class `contextual-dispose`.
       - If `null` or `'pet'`: text/emoji `❤️`, `aria-label: "Pet Dog"`, remove contextual classes.
     - In action pad trigger:
       - When `btn-touch-pet` is tapped:
         - If `contextualAction === 'pickup'`: invoke `this.callbacks.onPickup?.()` (or fallback to `onContextualAction?.('pickup')`).
         - If `contextualAction === 'dispose'`: invoke `this.callbacks.onDispose?.()` (or fallback to `onContextualAction?.('dispose')`).
         - Otherwise: invoke `this.callbacks.onPet?.()`.
   - In `src/main.js`:
     - Register `onPickup: () => this.pickUpPoop()`, `onDispose: () => this.disposePoopBag()`.
     - During the game loop, when `activePoopPileNearPlayer` or `activeBinNearPlayer` are updated:
       ```javascript
       if (this.activePoopPileNearPlayer) {
         this.touchControls?.setContextualAction('pickup');
       } else if (this.activeBinNearPlayer && this.hasPoopBag) {
         this.touchControls?.setContextualAction('dispose');
       } else {
         this.touchControls?.setContextualAction(null);
       }
       ```
2. **Adaptive In-World Prompts**:
   - In `drawInteractionPrompt(ctx, worldX, worldY, text)`:
     - Check if touch controls or touch mode is active (`TouchControls.isTouchDevice()`).
     - If touch mode is active:
       - For poop: display `Tap 💩 or [E] to pick up`
       - For bin: display `Tap 🗑️ or [E] to dispose`
       - Desktop remains `[E] Pick up waste` and `[E] Dispose in bin`.
3. **Contextual Action Glow Styles**:
   - In `src/style.css`:
     - Define `.touch-action-btn.contextual-pickup` with warm amber glow and pulsing keyframe animation.
     - Define `.touch-action-btn.contextual-dispose` with vibrant emerald glow and pulsing keyframe animation.

### 2.3 Track 3: Virtual Keyboard Viewport Handling (`src/ui/proximity_chat.js`, `src/style.css`)
1. **VisualViewport Listener in ProximityChat**:
   - In `src/ui/proximity_chat.js`:
     - When `window.visualViewport` is supported, attach `resize` and `scroll` listeners:
       ```javascript
       const handleViewportResize = () => {
         const vv = window.visualViewport;
         if (!vv) return;
         const vh = vv.height;
         const vt = vv.offsetTop;
         document.documentElement.style.setProperty('--vdw-viewport-height', `${vh}px`);
         document.documentElement.style.setProperty('--vdw-viewport-top', `${vt}px`);
         if (this.panel && this.panel.classList.contains('active')) {
           this.adjustPanelForViewport(vh, vt);
         }
       };
       window.visualViewport.addEventListener('resize', handleViewportResize);
       window.visualViewport.addEventListener('scroll', handleViewportResize);
       handleViewportResize();
       ```
     - In `adjustPanelForViewport(vh, vt)`:
       - Ensure panel `maxHeight` is constrained: `this.panel.style.maxHeight = `${Math.min(vh - 20, 480)}px`` (or via CSS variable).
       - When `#proximity-chat-input` receives focus, scroll input into view.
2. **CSS Mobile Anchoring**:
   - In `src/style.css`:
     - In mobile media query `@media (max-width: 768px), (pointer: coarse)`:
       - Anchor `#proximity-chat-panel`:
         ```css
         #proximity-chat-panel {
           top: max(12px, calc(var(--vdw-viewport-top, 0px) + 12px));
           max-height: calc(var(--vdw-viewport-height, 100vh) - 24px);
           bottom: auto;
         }
         #proximity-chat-messages {
           max-height: calc(var(--vdw-viewport-height, 100vh) - 140px);
         }
         ```

### 2.4 Track 4: Testing & Verification
1. **Unit & Mathematical Tests**:
   - Extend `virtual_dog_walker/test/test_mobile_controls.py`:
     - Test `Camera` with `zoom` parameter (0.75x, 0.85x, 1.0x): verify `world_to_screen`, `screen_to_world`, bounds clamping, and round-trip consistency.
     - Test `TouchControls` contextual action transitions: verify button text, aria-label, CSS classes, and callback execution for `pet`, `pickup`, and `dispose`.
     - Test `visualViewport` CSS variable declarations and responsive styles in `style.css`.
2. **Integration Verification**:
   - Run `python3 test/run_tests.py` in `virtual_dog_walker` (all test suites must pass).
   - Run `cargo test` and `deploy_dry_run.sh` in `zero_chat`.

---

## 3. Acceptance Criteria

- **AC-014.1**: `canvas.width` and `canvas.height` backing buffer dimensions are multiplied by `devicePixelRatio` (clamped 1.0 to 3.0), and CSS styling (`style.width`, `style.height`) maintains viewport dimensions. Canvas rendering context applies DPR transform so entities and environment render without blurriness on Retina displays.
- **AC-014.2**: `Camera` in `src/world/field.js` supports a `zoom` property (`resize(w, h, zoom)`). On viewports narrower than 768px, a responsive zoom factor (0.75x–0.85x) is applied, expanding the visible park area on mobile by 20%–30%. Desktop viewports (>=900px) remain 1.0x.
- **AC-014.3**: `Camera.worldToScreen` and `Camera.screenToWorld` accurately transform coordinates with `zoom`. Tapping the canvas to pet the dog, throw the tennis ball, or interact with items maps accurately to world coordinates at any zoom level.
- **AC-014.4**: `btn-touch-pet` on the touch action pad dynamically morphs into `💩` with aria-label `"Pick up dog waste"` and glowing class `contextual-pickup` when within 40px of dog waste, and into `🗑️` with aria-label `"Dispose waste in bin"` and glowing class `contextual-dispose` when within 44px of a bin with a waste bag. It reverts to `❤️` Pet Dog otherwise.
- **AC-014.5**: Tapping the morphed `💩` button triggers `pickUpPoop()`; tapping `🗑️` triggers `disposePoopBag()`; haptic pulse (25ms) triggers on tap. In-world interaction prompts adapt text on touch devices to reference the touch buttons.
- **AC-014.6**: `src/style.css` includes distinct glowing animations for `.contextual-pickup` (warm amber glow) and `.contextual-dispose` (emerald glow).
- **AC-014.7**: `#proximity-chat-panel` listens to `window.visualViewport` resize/scroll events and dynamically constrains panel height using `--vdw-viewport-height`, keeping the chat input bar visible directly above the on-screen keyboard.
- **AC-014.8**: All existing tests in `virtual_dog_walker` (`python3 test/run_tests.py`) and `zero_chat` pass with 0 regressions, and new test cases covering camera zoom, touch button morphing, and keyboard viewport handling pass.

---

## 4. Proposed Brief Decomposition (Parallel, Disjoint File Sets)

| Brief | Cognitive Size | Target Repo | Primary Files Affected |
| :--- | :--- | :--- | :--- |
| `brief-canvas-dpr-camera-zoom` | MEDIUM [M] | `virtual_dog_walker` | `src/main.js`, `src/world/field.js` |
| `brief-contextual-touch-sanitation` | MEDIUM [M] | `virtual_dog_walker` | `src/ui/touch_controls.js`, `src/main.js` (touch callbacks & prompt text), `src/style.css` (glow styles) |
| `brief-virtual-keyboard-viewport` | LOW [L] | `virtual_dog_walker` | `src/ui/proximity_chat.js`, `src/style.css` (mobile media queries & CSS variables) |
| `brief-mobile-regression-tests` | LOW [L] | `virtual_dog_walker` | `test/test_mobile_controls.py` |

---

## 5. Blast Radius & Risk Analysis

1. **Canvas DPR Scaling & Offscreen Canvases**:
   - `src/world/environment.js` uses offscreen canvases for lighting cutouts (`_getOffscreenCanvas(w, h)`). When `w` and `h` are passed as logical CSS pixels (`this.camera.viewportWidth`, `this.camera.viewportHeight`), the offscreen canvas matches logical dimensions, and `ctx.drawImage` scales smoothly. Verified that passing logical dimensions prevents mismatch or double-scaling bugs.
2. **Touch Coordinate Projection Parity**:
   - Ball throw and entity tap logic in `main.js:573-605` calls `this.camera.screenToWorld(e.clientX, e.clientY)` and `this.camera.worldToScreen(...)`. Because `e.clientX` and `e.clientY` are CSS viewport coordinates, updating `screenToWorld` with `rawX = screenX / this.zoom + this.x` preserves exact tap alignment regardless of device pixel ratio or camera zoom.
3. **Cross-Repo Independence**:
   - Changes are strictly isolated to `virtual_dog_walker` runtime UI/rendering files and tests, while `zero_chat` maintains the OpenSpec proposal and tracking. No breaking protocol or Wasm changes are introduced.

