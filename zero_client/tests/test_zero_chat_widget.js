// Automated Regression Test Suite for ZeroChatWidget
// Runs via JavaScriptCore (jsc) on macOS

(function() {
    print("===============================================================================");
    print("Running ZeroChatWidget Unit & Integration Suite");
    print("===============================================================================");

    // Minimal Mock DOM Implementation for JSC
    class MockClassList {
        constructor() {
            this.classes = new Set();
        }
        add(c) { this.classes.add(c); }
        remove(c) { this.classes.delete(c); }
        contains(c) { return this.classes.has(c); }
        toggle(c) {
            if (this.classes.has(c)) this.classes.delete(c);
            else this.classes.add(c);
        }
        toString() { return Array.from(this.classes).join(' '); }
    }

    class MockElement {
        constructor(tagName = 'div') {
            this.tagName = tagName.toUpperCase();
            this.id = '';
            this.classList = new MockClassList();
            this.attributes = new Map();
            const self = this;
            this.style = new Proxy({
                properties: new Map(),
                setProperty: (k, v) => self.style.properties.set(k, v),
                getPropertyValue: (k) => self.style.properties.get(k) || ''
            }, {
                get(target, prop) {
                    if (prop in target) return target[prop];
                    const kebab = String(prop).replace(/([A-Z])/g, '-$1').toLowerCase();
                    return target.properties.get(kebab) || '';
                },
                set(target, prop, value) {
                    if (prop in target) {
                        target[prop] = value;
                    } else {
                        const kebab = String(prop).replace(/([A-Z])/g, '-$1').toLowerCase();
                        target.properties.set(kebab, value);
                    }
                    return true;
                }
            });
            this.children = [];
            this.parentNode = null;
            this.textContent = '';
            this.value = '';
            this.listeners = new Map();
            this.scrollTop = 0;
            this.scrollHeight = 100;
        }

        get className() {
            return this.classList.toString();
        }

        set className(val) {
            this.classList.classes.clear();
            if (val) {
                val.split(/\s+/).forEach(c => c && this.classList.add(c));
            }
        }

        setAttribute(k, v) {
            this.attributes.set(k, String(v));
            if (k === 'id') this.id = String(v);
            if (k === 'class') this.className = String(v);
        }

        getAttribute(k) {
            if (k === 'id') return this.id;
            if (k === 'class') return this.className;
            return this.attributes.get(k) || null;
        }

        hasAttribute(k) {
            return this.attributes.has(k);
        }

        removeAttribute(k) {
            this.attributes.delete(k);
        }

        appendChild(child) {
            if (child.parentNode) {
                child.parentNode.removeChild(child);
            }
            child.parentNode = this;
            this.children.push(child);
            return child;
        }

        removeChild(child) {
            const idx = this.children.indexOf(child);
            if (idx !== -1) {
                this.children.splice(idx, 1);
                child.parentNode = null;
            }
            return child;
        }

        remove() {
            if (this.parentNode) {
                this.parentNode.removeChild(this);
            }
        }

        addEventListener(evt, fn) {
            if (!this.listeners.has(evt)) this.listeners.set(evt, []);
            this.listeners.get(evt).push(fn);
        }

        removeEventListener(evt, fn) {
            if (this.listeners.has(evt)) {
                const arr = this.listeners.get(evt);
                const idx = arr.indexOf(fn);
                if (idx !== -1) arr.splice(idx, 1);
            }
        }

        dispatchEvent(evt) {
            const arr = this.listeners.get(evt.type) || [];
            evt.target = this;
            for (const fn of arr) {
                fn(evt);
            }
        }

        focus() {
            this._focused = true;
        }

        set innerHTML(html) {
            this.children = [];
            // Parse simple known tags for widget
            this._rawHTML = html;
            this._parseSimpleHTML(html);
        }

        get innerHTML() {
            return this._rawHTML || '';
        }

        _parseSimpleHTML(html) {
            const selfClosingTags = new Set(['INPUT', 'IMG', 'BR', 'HR', 'META', 'LINK']);
            const stack = [this];
            let pos = 0;
            const len = html.length;

            while (pos < len) {
                const lt = html.indexOf('<', pos);
                if (lt === -1) {
                    const text = html.slice(pos).trim();
                    if (text && stack.length > 0) {
                        stack[stack.length - 1].textContent = text;
                    }
                    break;
                }

                if (lt > pos) {
                    const text = html.slice(pos, lt).trim();
                    if (text && stack.length > 0) {
                        stack[stack.length - 1].textContent = text;
                    }
                }

                const gt = html.indexOf('>', lt);
                if (gt === -1) break;

                const tagContent = html.slice(lt + 1, gt).trim();
                pos = gt + 1;

                if (tagContent.startsWith('/')) {
                    // Closing tag
                    if (stack.length > 1) {
                        stack.pop();
                    }
                } else {
                    // Opening or self-closing tag
                    const isSelfClosing = tagContent.endsWith('/') || selfClosingTags.has(tagContent.split(/\s+/)[0].toUpperCase());
                    const cleanTag = tagContent.endsWith('/') ? tagContent.slice(0, -1).trim() : tagContent;
                    const parts = cleanTag.match(/^[a-zA-Z0-9]+/);
                    if (!parts) continue;
                    const tagName = parts[0];
                    const el = new MockElement(tagName);

                    const attrRegex = /([a-zA-Z0-9_-]+)="([^"]*)"/g;
                    let attrMatch;
                    while ((attrMatch = attrRegex.exec(cleanTag)) !== null) {
                        el.setAttribute(attrMatch[1], attrMatch[2]);
                    }

                    if (stack.length > 0) {
                        stack[stack.length - 1].appendChild(el);
                    }

                    if (!isSelfClosing) {
                        stack.push(el);
                    }
                }
            }
        }

        querySelector(sel) {
            const all = this.querySelectorAll(sel);
            return all.length > 0 ? all[0] : null;
        }

        querySelectorAll(sel) {
            const results = [];
            const check = (el) => {
                if (sel.startsWith('#')) {
                    if (el.id === sel.slice(1)) results.push(el);
                } else if (sel.startsWith('.')) {
                    if (el.classList.contains(sel.slice(1))) results.push(el);
                } else if (sel.includes('[')) {
                    // basic attribute selector like [data-peer-id="x"]
                    const m = sel.match(/\[([a-zA-Z0-9_-]+)(?:="([^"]*)")?\]/);
                    if (m) {
                        const attr = m[1];
                        const val = m[2];
                        if (val !== undefined) {
                            if (el.getAttribute(attr) === val) results.push(el);
                        } else if (el.hasAttribute(attr)) {
                            results.push(el);
                        }
                    }
                } else if (el.tagName.toLowerCase() === sel.toLowerCase()) {
                    results.push(el);
                }
                for (const c of el.children) check(c);
            };
            for (const c of this.children) check(c);
            return results;
        }
    }

    // Setup global window and document
    globalThis.document = {
        createElement: (tag) => new MockElement(tag),
        querySelector: (sel) => null,
        querySelectorAll: (sel) => []
    };

    // Timer queue for simulated time
    let timerIdCounter = 1;
    const activeTimers = new Map();

    globalThis.setTimeout = function(fn, delay) {
        const id = timerIdCounter++;
        activeTimers.set(id, { fn, delay, due: delay });
        return id;
    };

    globalThis.clearTimeout = function(id) {
        activeTimers.delete(id);
    };

    function advanceTime(ms) {
        const dueList = [];
        for (const [id, t] of activeTimers.entries()) {
            t.due -= ms;
            if (t.due <= 0) {
                dueList.push({ id, fn: t.fn });
            }
        }
        for (const item of dueList) {
            activeTimers.delete(item.id);
            item.fn();
        }
    }

    // Load ZeroChatWidget code
    // We import or evaluate the widget code
    const widgetPath = "zero_client/widget/zero_chat_widget.js";
    const code = read(widgetPath);

    // Replace ES module export for JSC evaluation
    const cleanCode = code
        .replace(/export const PEER_COLOR_PALETTE/g, 'const PEER_COLOR_PALETTE')
        .replace(/export class ZeroChatWidget/g, 'class ZeroChatWidget')
        .replace(/export default ZeroChatWidget;/g, '')
        + '\nglobalThis.ZeroChatWidget = ZeroChatWidget;\nglobalThis.PEER_COLOR_PALETTE = PEER_COLOR_PALETTE;';

    eval(cleanCode);

    const ZeroChatWidget = globalThis.ZeroChatWidget;
    const PEER_COLOR_PALETTE = globalThis.PEER_COLOR_PALETTE;

    function assert(cond, msg) {
        if (!cond) {
            throw new Error("Assertion failed: " + msg);
        }
    }

    print("\n[Test 1] Palette & Constructor Validation...");
    assert(Array.isArray(PEER_COLOR_PALETTE) && PEER_COLOR_PALETTE.length === 8, "Palette must have 8 colors");
    assert(PEER_COLOR_PALETTE[0] === '#10b981', "Palette[0] should be emerald #10b981");

    const container = new MockElement('div');
    container.id = 'panel';

    let sentText = null;
    let burnedUuid = null;
    let sessionBurned = false;

    const widget = new ZeroChatWidget({
        container: container,
        mode: 'embedded',
        onSendMessage: (t) => { sentText = t; },
        onBurnMessage: (u) => { burnedUuid = u; },
        onBurnSession: () => { sessionBurned = true; },
        agingMs: 70000,
        dissolveMs: 90000
    });

    assert(widget.element !== null, "Widget element must be created");
    assert(widget.element.classList.contains('zero-chat-widget'), "Element must have class .zero-chat-widget");
    assert(widget.element.classList.contains('zero-chat-embedded'), "Element must have class .zero-chat-embedded");
    assert(container.children.includes(widget.element), "Widget must be mounted in container");

    print("  PASS: Constructor and container DOM initialized properly.");

    print("\n[Test 2] Header, Status Pill, Form & Inputs Presence...");
    assert(widget.statusEl !== null && widget.statusEl.id === 'proximity-chat-status', "#proximity-chat-status must exist");
    assert(widget.peerCountEl !== null && widget.peerCountEl.id === 'peer-count', "#peer-count must exist");
    assert(widget.burnHeaderBtn !== null && widget.burnHeaderBtn.classList.contains('btn-header-burn'), ".btn-header-burn must exist");
    assert(widget.messagesContainer !== null && widget.messagesContainer.id === 'proximity-chat-messages', "#proximity-chat-messages must exist");
    assert(widget.form !== null && widget.form.id === 'proximity-chat-form', "#proximity-chat-form must exist");
    assert(widget.input !== null && widget.input.id === 'proximity-chat-input', "#proximity-chat-input must exist");
    assert(widget.sendBtn !== null && widget.sendBtn.id === 'proximity-chat-send', "#proximity-chat-send must exist");

    print("  PASS: All required sub-elements and backward-compatible IDs present.");

    print("\n[Test 3] Peer Membership & Status Badge Updates...");
    widget.setPeers(['peer-1', 'peer-2'], 2);
    assert(widget.peerCountEl.textContent === '2 nearby', "Peer count text should update to '2 nearby'");
    assert(widget.statusEl.getAttribute('data-peer-count') === '2', "Status attribute should update");

    print("  PASS: setPeers updates status and nearby counter.");

    print("\n[Test 4] Form Submission & Message Sending...");
    widget.input.value = "Hello nearby peers!";
    widget.form.dispatchEvent({ type: 'submit', preventDefault: () => {} });
    assert(sentText === "Hello nearby peers!", "onSendMessage must receive submitted text");
    assert(widget.input.value === "", "Input must be cleared after submission");

    print("  PASS: Form submission dispatches onSendMessage and clears input.");

    print("\n[Test 5] Message Appending (Incoming & Outgoing)...");
    const msg1 = widget.appendMessage({
        senderId: 'peer-1',
        name: 'Alice',
        text: 'Hi everyone!',
        isSelf: false,
        messageUuid: 'uuid-msg-1'
    });

    assert(msg1.classList.contains('message-bubble'), "Bubble must have .message-bubble");
    assert(msg1.classList.contains('proximity-chat-msg'), "Bubble must have .proximity-chat-msg");
    assert(msg1.classList.contains('message-incoming'), "Incoming bubble must have .message-incoming");
    assert(msg1.getAttribute('data-msg-uuid') === 'uuid-msg-1', "Bubble must have data-msg-uuid");
    assert(msg1.getAttribute('data-peer-id') === 'peer-1', "Bubble must have data-peer-id");

    const peerDot = msg1.querySelector('.peer-dot');
    assert(peerDot !== null, "Incoming bubble must have peer-dot");
    const senderSpan = msg1.querySelector('.message-sender');
    assert(senderSpan !== null && senderSpan.textContent === 'Alice', "Sender span must display name");
    const flameBtn = msg1.querySelector('.btn-message-burn');
    assert(flameBtn !== null, "Bubble must have .btn-message-burn");
    assert(flameBtn.textContent === '🔥', "Burn button must have flame icon 🔥");

    const msg2 = widget.appendMessage({
        senderId: 'local',
        name: 'You',
        text: 'Replying locally',
        isSelf: true,
        messageUuid: 'uuid-msg-2'
    });
    assert(msg2.classList.contains('message-outgoing'), "Outgoing bubble must have .message-outgoing");
    assert(widget.messagesContainer.children.length === 2, "Messages container should have 2 bubbles");

    print("  PASS: Message bubbles rendered with headers, peer dots, flame buttons, and classes.");

    print("\n[Test 6] Per-Message Burn Button Click...");
    flameBtn.dispatchEvent({
        type: 'click',
        preventDefault: () => {},
        stopPropagation: () => {}
    });
    assert(burnedUuid === 'uuid-msg-1', "Clicking flame button must invoke onBurnMessage with messageUuid");
    assert(msg1.classList.contains('message-dissolve'), "Dissolve class must be added to bubble");
    advanceTime(250);
    assert(!widget.messagesContainer.children.includes(msg1), "Bubble must be removed from DOM after 200ms dissolve");

    print("  PASS: Per-message flame button invokes callback and dissolves message.");

    print("\n[Test 7] Ephemeral Aging & Dissolve Timers...");
    const msg3 = widget.appendMessage({
        senderId: 'peer-2',
        name: 'Bob',
        text: 'Ephemeral message',
        isSelf: false,
        messageUuid: 'uuid-msg-3'
    });

    assert(!msg3.classList.contains('message-aging'), "Message should not be aging initially");
    advanceTime(70000);
    assert(msg3.classList.contains('message-aging'), "Message must have .message-aging at 70s");
    assert(!msg3.classList.contains('message-dissolve'), "Message should not be dissolving at 70s");

    advanceTime(20000); // 90s total
    assert(msg3.classList.contains('message-dissolve'), "Message must have .message-dissolve at 90s");
    advanceTime(250);
    assert(!widget.messagesContainer.children.includes(msg3), "Message must be purged from DOM at 90s");

    print("  PASS: 70s aging transition and 90s dissolve lifecycle verified.");

    print("\n[Test 8] Peer Message Purging (burnPeerMessages)...");
    widget.appendMessage({ senderId: 'peer-3', text: 'Msg A', messageUuid: 'u-a' });
    widget.appendMessage({ senderId: 'peer-3', text: 'Msg B', messageUuid: 'u-b' });
    widget.appendMessage({ senderId: 'peer-4', text: 'Msg C', messageUuid: 'u-c' });

    assert(widget.messagesContainer.children.length === 3, "Should have 3 messages");
    widget.burnPeerMessages('peer-3');
    advanceTime(250);
    assert(widget.messagesContainer.children.length === 1, "Only peer-4 message should remain");

    print("  PASS: burnPeerMessages purges all matching peer messages.");

    print("\n[Test 9] Session Burn (Header Burn Button)...");
    widget.burnHeaderBtn.dispatchEvent({
        type: 'click',
        preventDefault: () => {},
        stopPropagation: () => {}
    });
    assert(sessionBurned === true, "Header burn click must trigger onBurnSession");
    advanceTime(250);
    assert(widget.messagesContainer.children.length === 0, "All messages must be removed on session burn");

    print("  PASS: Session burn button purges all messages and invokes callback.");

    print("\n[Test 10] Unread Count & Peer Strip Pills/Dots (ticket unification-013)...");
    let unreadNotified = -1;
    widget.onUnreadCountChange = (count) => {
        unreadNotified = count;
    };

    assert(widget.unreadCount === 0, "Initial unreadCount should be 0");
    widget.incrementUnread();
    assert(widget.unreadCount === 1, "incrementUnread should increase to 1");
    assert(unreadNotified === 1, "onUnreadCountChange should be notified with 1");

    widget.incrementUnread();
    assert(widget.unreadCount === 2, "incrementUnread should increase to 2");
    assert(unreadNotified === 2, "onUnreadCountChange should be notified with 2");

    widget.clearUnread();
    assert(widget.unreadCount === 0, "clearUnread should reset to 0");
    assert(unreadNotified === 0, "onUnreadCountChange should be notified with 0");

    // Test peer strip rendering with 2+ peers
    widget.setPeers(['peer-1', 'peer-2'], 2, {
        'peer-1': { name: 'Alice', color: '#10b981' },
        'peer-2': { name: 'Bob', color: '#3b82f6' }
    });
    assert(widget.currentPeerCount === 2, "Peer count must be 2");
    assert(widget.peerCountEl.textContent === '2 nearby', "Header text should be '2 nearby'");
    assert(widget.peerStripEl.children.length === 2, "Peer strip should render 2 chips");

    const chip1 = widget.peerStripEl.children[0];
    const dot1 = chip1.children[0];
    const label1 = chip1.children[1];
    assert(dot1.style.properties.get('background-color') === '#10b981', "Dot 1 should have Alice's color");
    assert(label1.textContent === 'Alice', "Label 1 should display 'Alice'");

    // Test overflow with >4 peers
    widget.setPeers(['p1', 'p2', 'p3', 'p4', 'p5'], 5, {
        'p1': { name: 'P1' }, 'p2': { name: 'P2' }, 'p3': { name: 'P3' }, 'p4': { name: 'P4' }, 'p5': { name: 'P5' }
    });
    assert(widget.peerStripEl.children.length === 5, "Peer strip should render 4 chips + 1 overflow chip");
    const overflowChip = widget.peerStripEl.children[4];
    assert(overflowChip.textContent === '+1', "Overflow chip should be '+1'");

    print("  PASS: Unread counting and peer strip pills/dots verified.");

    print("\n===============================================================================");
    print("ALL ZEROCATWIDGET UNIT TESTS PASSED SUCCESSFULLY!");
    print("===============================================================================");
})();
