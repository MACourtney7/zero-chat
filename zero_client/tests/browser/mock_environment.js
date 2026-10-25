// Mock DOM & WebRTC environment for testing zero_client index.html mesh logic
(function(global) {
    if (typeof global.print === "function") {
        global.console = {
            log: global.print,
            warn: global.print,
            error: global.print,
            debug: global.print
        };
    }

    let timeoutCounter = 0;
    const activeTimeouts = new Map();
    const nativeSetTimeout = global.setTimeout;
    const nativeClearTimeout = global.clearTimeout;
    let virtualTime = 0;

    global.setTimeout = function(fn, ms) {
        const id = ++timeoutCounter;
        const due = virtualTime + (ms || 0);
        activeTimeouts.set(id, { fn, dueTime: due, ms: ms || 0 });
        nativeSetTimeout(function() {
            if (activeTimeouts.has(id)) {
                activeTimeouts.delete(id);
                fn();
            }
        }, ms || 0);
        return id;
    };

    global.clearTimeout = function(id) {
        if (id) {
            activeTimeouts.delete(id);
            if (nativeClearTimeout) {
                nativeClearTimeout(id);
            }
        }
    };

    global.advanceTimersByTime = function(ms) {
        const targetTime = virtualTime + (ms || 0);
        while (true) {
            let earliestId = null;
            let earliestDue = Infinity;
            for (const [id, t] of activeTimeouts.entries()) {
                if (t.dueTime <= targetTime && t.dueTime < earliestDue) {
                    earliestDue = t.dueTime;
                    earliestId = id;
                }
            }
            if (earliestId === null) {
                break;
            }
            const t = activeTimeouts.get(earliestId);
            activeTimeouts.delete(earliestId);
            virtualTime = t.dueTime;
            try {
                t.fn();
            } catch (err) {
                console.error("Error in timer callback during advanceTimersByTime:", err);
            }
        }
        virtualTime = targetTime;
    };

    if (!global.setInterval) {
        let intervalCounter = 0;
        const activeIntervals = new Map();
        global.setInterval = function(fn, ms) {
            const id = ++intervalCounter;
            let active = true;
            function run() {
                if (!active) return;
                try { fn(); } catch (_) {}
                if (active) {
                    setTimeout(run, ms);
                }
            }
            setTimeout(run, ms);
            activeIntervals.set(id, () => { active = false; });
            return id;
        };
        global.clearInterval = function(id) {
            const stop = activeIntervals.get(id);
            if (stop) {
                stop();
                activeIntervals.delete(id);
            }
        };
    }
    // 1. Mock crypto
    if (!global.crypto) {
        global.crypto = {
            getRandomValues: function(arr) {
                for (let i = 0; i < arr.length; i++) {
                    arr[i] = Math.floor(Math.random() * 256);
                }
                return arr;
            },
            randomUUID: function() {
                return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
                    const r = Math.random() * 16 | 0;
                    const v = c === 'x' ? r : (r & 0x3 | 0x8);
                    return v.toString(16);
                });
            },
            subtle: {
                importKey: async function() { return {}; },
                sign: async function() {
                    return new Uint8Array(32).fill(7).buffer;
                }
            }
        };
    }

    // TextEncoder / TextDecoder
    if (!global.TextEncoder) {
        global.TextEncoder = function() {};
        global.TextEncoder.prototype.encode = function(str) {
            const arr = new Uint8Array(str.length);
            for (let i = 0; i < str.length; i++) arr[i] = str.charCodeAt(i);
            return arr;
        };
    }
    if (!global.TextDecoder) {
        global.TextDecoder = function() {};
        global.TextDecoder.prototype.decode = function(bytes) {
            let s = '';
            for (let i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]);
            return s;
        };
    }

    // Base64 helpers
    if (!global.btoa) {
        global.btoa = function(bin) {
            return Buffer ? Buffer.from(bin, 'binary').toString('base64') : '';
        };
    }
    if (!global.atob) {
        global.atob = function(b64) {
            return Buffer ? Buffer.from(b64, 'base64').toString('binary') : '';
        };
    }

    // 2. Mock DOM
    function createMockElement(id = "", className = "", tag = "div") {
        if (!tag || tag === "div") {
            if (className && !className.includes(" ") && ["div", "button", "span", "svg", "path", "p", "a", "input", "form"].includes(className.toLowerCase())) {
                tag = className;
                className = "";
            }
        }
        const attributes = new Map();
        let classListArray = (className || "").split(" ").filter(Boolean);
        const listeners = new Map();
        const styleProps = new Map();
        const element = {
            tagName: (tag || "div").toUpperCase(),
            id: id,
            get className() {
                return classListArray.join(" ");
            },
            set className(v) {
                classListArray = (v || "").split(" ").filter(Boolean);
            },
            classList: {
                add: function(c) {
                    if (!classListArray.includes(c)) classListArray.push(c);
                },
                remove: function(c) {
                    const idx = classListArray.indexOf(c);
                    if (idx >= 0) classListArray.splice(idx, 1);
                },
                contains: function(c) {
                    return classListArray.includes(c);
                },
                toggle: function(c, force) {
                    if (force === undefined) {
                        force = !classListArray.includes(c);
                    }
                    if (force) {
                        if (!classListArray.includes(c)) classListArray.push(c);
                    } else {
                        const idx = classListArray.indexOf(c);
                        if (idx >= 0) classListArray.splice(idx, 1);
                    }
                    return force;
                }
            },
            _textContent: "",
            get textContent() {
                if (this._textContent !== "") return this._textContent;
                if (this.children && this.children.length > 0) {
                    const ch = this.children.map(c => c.textContent || "").join("");
                    if (ch) return ch;
                }
                if (this._innerHTML) {
                    return this._innerHTML.replace(/<[^>]+>/g, "");
                }
                return "";
            },
            set textContent(val) {
                this._textContent = String(val);
                this._innerHTML = String(val);
                this.children = [];
            },
            _innerHTML: "",
            get innerHTML() {
                return this._innerHTML;
            },
            set innerHTML(val) {
                this._innerHTML = String(val);
                this._textContent = "";
                this.children = [];
                if (!val.includes("<")) {
                    this._textContent = String(val);
                    return;
                }
                if (val && val.includes("<")) {
                    const tagRegex = /<(\/)?([a-zA-Z0-9_-]+)([^>]*?)(\/)?>/g;
                    const stack = [this];
                    const voidTags = new Set(["input", "img", "br", "hr", "meta", "link"]);
                    let m;
                    let lastIdx = 0;
                    while ((m = tagRegex.exec(val)) !== null) {
                        const textChunk = val.substring(lastIdx, m.index);
                        if (textChunk && stack.length > 1) {
                            const top = stack[stack.length - 1];
                            if (top.children.length === 0) {
                                top._textContent = (top._textContent || "") + textChunk;
                            }
                            top._innerHTML = (top._innerHTML || "") + textChunk;
                        }
                        lastIdx = tagRegex.lastIndex;

                        const isClosing = m[1] === "/";
                        const tName = m[2];
                        const attrs = m[3] || "";
                        const isSelfClosing = m[4] === "/" || voidTags.has(tName.toLowerCase());

                        if (isClosing) {
                            for (let i = stack.length - 1; i >= 1; i--) {
                                if (stack[i].tagName === tName.toUpperCase()) {
                                    stack.splice(i, stack.length - i);
                                    break;
                                }
                            }
                        } else {
                            let chId = "";
                            let chClass = "";
                            const idMatch = attrs.match(/\bid=["']([^"']+)["']/i);
                            if (idMatch) chId = idMatch[1];
                            const classMatch = attrs.match(/\bclass=["']([^"']+)["']/i);
                            if (classMatch) chClass = classMatch[1];

                            const ch = createMockElement(chId, chClass, tName);
                            const attrRegex = /([a-zA-Z0-9_-]+)=["']([^"']*)["']/g;
                            let am;
                            while ((am = attrRegex.exec(attrs)) !== null) {
                                ch.setAttribute(am[1], am[2]);
                            }

                            if (chId && typeof elementRegistry !== "undefined") {
                                elementRegistry.set(chId, ch);
                            }

                            const parent = stack[stack.length - 1];
                            ch.parentNode = parent;
                            parent.children.push(ch);

                            if (!isSelfClosing) {
                                stack.push(ch);
                            }
                        }
                    }
                    const trailingText = val.substring(lastIdx);
                    if (trailingText && stack.length > 1) {
                        const top = stack[stack.length - 1];
                        top._textContent = (top._textContent || "") + trailingText;
                        top._innerHTML = (top._innerHTML || "") + trailingText;
                    }
                }
            },
            insertAdjacentHTML: function(position, html) {
                const temp = createMockElement();
                temp.innerHTML = html;
                if (position === "afterbegin") {
                    for (let i = temp.children.length - 1; i >= 0; i--) {
                        const ch = temp.children[i];
                        ch.parentNode = this;
                        this.children.unshift(ch);
                    }
                } else {
                    for (const ch of temp.children) {
                        ch.parentNode = this;
                        this.children.push(ch);
                    }
                }
                this._innerHTML = (position === "afterbegin") ? (html + this._innerHTML) : (this._innerHTML + html);
            },
            disabled: false,
            placeholder: "",
            style: {
                _props: styleProps,
                setProperty: function(k, v) { styleProps.set(k, String(v)); },
                getPropertyValue: function(k) { return styleProps.get(k) || ""; },
                get minWidth() { return styleProps.get("min-width") || ""; },
                set minWidth(v) { styleProps.set("min-width", String(v)); },
                get minHeight() { return styleProps.get("min-height") || ""; },
                set minHeight(v) { styleProps.set("min-height", String(v)); }
            },
            offsetWidth: 100,
            children: [],
            get firstElementChild() {
                return (this.children && this.children.length > 0) ? this.children[0] : null;
            },
            contains: function(child) {
                let curr = child;
                while (curr) {
                    if (curr === this) return true;
                    curr = curr.parentNode;
                }
                return false;
            },
            appendChild: function(child) {
                child.parentNode = this;
                this.children.push(child);
                return child;
            },
            removeChild: function(child) {
                if (child) child.remove();
                return child;
            },
            remove: function() {
                if (this.parentNode && this.parentNode.children) {
                    const idx = this.parentNode.children.indexOf(this);
                    if (idx >= 0) this.parentNode.children.splice(idx, 1);
                }
                this.parentNode = null;
            },
            querySelectorAll: function(selector) {
                const results = [];
                const subSelectors = selector.split(",").map(s => s.trim());
                function matchesSingle(ch, sel) {
                    if (sel.includes("[") && sel.includes("]")) {
                        const m = sel.match(/^(\.?)([a-zA-Z0-9_-]*)\[([a-zA-Z0-9_-]+)=["']?([^"']+)["']?\]$/);
                        if (m) {
                            const [_, isClass, tagOrClass, attrName, attrVal] = m;
                            if (isClass === "." && tagOrClass && !ch.classList.contains(tagOrClass)) {
                                return false;
                            }
                            if (isClass !== "." && tagOrClass && ch.tagName !== tagOrClass.toUpperCase() && ch.tagName !== tagOrClass.toLowerCase()) {
                                return false;
                            }
                            return ch.getAttribute(attrName) === attrVal;
                        }
                    } else if (sel.startsWith("#")) {
                        return ch.id === sel.slice(1);
                    } else if (sel.startsWith(".message-bubble:not(.message-outgoing)")) {
                        return ch.classList.contains("message-bubble") && !ch.classList.contains("message-outgoing");
                    } else if (sel.startsWith(".")) {
                        const classes = sel.slice(1).split(".").filter(Boolean);
                        return classes.length > 0 && classes.every(c => ch.classList.contains(c));
                    } else if (/^[a-zA-Z0-9_-]+$/.test(sel)) {
                        return ch.tagName === sel.toUpperCase() || ch.tagName === sel.toLowerCase();
                    }
                    return false;
                }
                function matchesChain(node, parts, root) {
                    if (!matchesSingle(node, parts[parts.length - 1])) {
                        return false;
                    }
                    if (parts.length === 1) return true;
                    let curr = node.parentNode;
                    let partIdx = parts.length - 2;
                    while (curr && curr !== root) {
                        if (matchesSingle(curr, parts[partIdx])) {
                            partIdx--;
                            if (partIdx < 0) return true;
                        }
                        curr = curr.parentNode;
                    }
                    return false;
                }
                const self = this;
                function traverse(el) {
                    for (const ch of el.children) {
                        if (subSelectors.some(sel => {
                            const parts = sel.split(/\s+/).filter(Boolean);
                            return matchesChain(ch, parts, self);
                        })) {
                            results.push(ch);
                        }
                        traverse(ch);
                    }
                }
                traverse(this);
                return results;
            },
            querySelector: function(selector) {
                const res = this.querySelectorAll(selector);
                return res.length > 0 ? res[0] : null;
            },
            setAttribute: function(k, v) { attributes.set(k, String(v)); },
            getAttribute: function(k) { return attributes.get(k) || null; },
            focus: function() {
                if (global.document) {
                    global.document.activeElement = this;
                }
            },
            value: "",
            addEventListener: function(event, cb) {
                if (!listeners.has(event)) listeners.set(event, []);
                listeners.get(event).push(cb);
            },
            removeEventListener: function(event, cb) {
                if (listeners.has(event)) {
                    const list = listeners.get(event);
                    const idx = list.indexOf(cb);
                    if (idx >= 0) list.splice(idx, 1);
                }
            },
            dispatchEvent: function(event) {
                event.target = this;
                if (listeners.has(event.type)) {
                    for (const cb of [...listeners.get(event.type)]) {
                        cb(event);
                    }
                }
                return !event.defaultPrevented;
            },
            click: function() {
                if (listeners.has("click")) {
                    let last;
                    const evt = (typeof global.Event === "function")
                        ? new global.Event("click", { cancelable: true })
                        : { target: this, defaultPrevented: false, preventDefault: () => {}, stopPropagation: () => {} };
                    evt.target = this;
                    for (const cb of [...listeners.get("click")]) {
                        last = cb(evt);
                    }
                    return last;
                }
            }
        };
        return element;
    }
    global.createMockElement = createMockElement;

    const elementRegistry = new Map();

    // Pre-populate header controls hierarchy per Ticket-042
    const mockHeaderControls = createMockElement("header-controls", "header-controls", "div");
    elementRegistry.set("header-controls", mockHeaderControls);

    const mockBurnBtn = createMockElement("destroy-session-button", "btn btn-burn btn-header-burn hidden", "button");
    mockBurnBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8.5 14.5A2.5 2.5 0 0 0 11 12c0-1.38-.5-2-1-3-1.072-2.143-.224-4.054 2-6 .5 2.5 2 4.9 4 6.5 2 1.6 3 3.5 3 5.5a7 7 0 1 1-14 0c0-1.153.433-2.294 1-3a2.5 2.5 0 0 0 2.5 2.5z"></path></svg>';
    elementRegistry.set("destroy-session-button", mockBurnBtn);
    mockHeaderControls.appendChild(mockBurnBtn);

    const mockHeaderAddBtn = createMockElement("header-add-btn", "btn btn-icon hidden", "button");
    elementRegistry.set("header-add-btn", mockHeaderAddBtn);
    mockHeaderControls.appendChild(mockHeaderAddBtn);

    const mockPeerCountEl = createMockElement("peer-count", "peer-count-badge hidden", "div");
    mockPeerCountEl.innerHTML = '<svg class="peer-slashed-user-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M16 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path><circle cx="10" cy="7" r="4"></circle><line x1="2" y1="2" x2="22" y2="22"></line></svg>';
    elementRegistry.set("peer-count", mockPeerCountEl);
    mockHeaderControls.appendChild(mockPeerCountEl);

    const mockIdleStatusCapsule = createMockElement("idle-status-capsule", "status-capsule-disconnected hidden", "div");
    elementRegistry.set("idle-status-capsule", mockIdleStatusCapsule);
    mockHeaderControls.appendChild(mockIdleStatusCapsule);

    const mockPeerListEl = createMockElement("peer-list", "peer-list hidden", "div");
    elementRegistry.set("peer-list", mockPeerListEl);
    mockHeaderControls.appendChild(mockPeerListEl);

    const mockHeaderLeaveBtn = createMockElement("header-leave-btn", "btn btn-leave btn-icon hidden", "button");
    elementRegistry.set("header-leave-btn", mockHeaderLeaveBtn);
    mockHeaderControls.appendChild(mockHeaderLeaveBtn);

    const mockBrandTextEl = createMockElement("brand-text", "brand-text", "div");
    elementRegistry.set("brand-text", mockBrandTextEl);

    if (!global.document) {
        global.document = {
            getElementById: function(id) {
                if (id === "burn-room-bar") {
                    return null;
                }
                if (!elementRegistry.has(id)) {
                    const el = createMockElement(id);
                    if (id === "peer-count") {
                        el.innerHTML = '<svg class="peer-slashed-user-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M16 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path><circle cx="10" cy="7" r="4"></circle><line x1="2" y1="2" x2="22" y2="22"></line></svg>';
                    }
                    if (id === "welcome-splash") {
                        el.innerHTML = `
                            <div class="welcome-header">
                                <p class="welcome-subtitle">Create -&gt; Share -&gt; Chat</p>
                            </div>
                            <div class="welcome-features">
                                <div class="feature-card">
                                    <div class="feature-icon">
                                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon>
                                        </svg>
                                    </div>
                                    <div class="feature-content">
                                        <span class="feature-title">Direct P2P Connection</span>
                                        <p class="feature-desc">Private channels with no intermediate relays or storage servers.</p>
                                    </div>
                                </div>
                                <div class="feature-card">
                                    <div class="feature-icon">
                                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect>
                                            <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
                                        </svg>
                                    </div>
                                    <div class="feature-content">
                                        <span class="feature-title">Secure Communications</span>
                                        <p class="feature-desc">All messages are end-to-end encrypted.</p>
                                    </div>
                                </div>
                                <div class="feature-card">
                                    <div class="feature-icon">
                                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"></path>
                                        </svg>
                                    </div>
                                    <div class="feature-content">
                                        <span class="feature-title">Ephemeral Sessions</span>
                                        <p class="feature-desc">Chats are permanently destroyed the moment they are closed.</p>
                                    </div>
                                </div>
                            </div>`;
                    }
                    elementRegistry.set(id, el);
                }
                return elementRegistry.get(id);
            },
            querySelector: function(selector) {
                if (selector === ".header-controls" || selector === "#header-controls") {
                    return elementRegistry.get("header-controls");
                }
                if (selector === ".brand-text" || selector === "#brand-text") {
                    return elementRegistry.get("brand-text");
                }
                if (selector === "#destroy-session-button" || selector === ".btn-header-burn") {
                    return elementRegistry.get("destroy-session-button");
                }
                if (selector === "#burn-room-bar" || selector === ".burn-room-bar") {
                    return null;
                }
                if (selector.startsWith("#")) {
                    const id = selector.slice(1);
                    if (id === "burn-room-bar") return null;
                    return elementRegistry.has(id) ? elementRegistry.get(id) : null;
                }
                return null;
            },
            querySelectorAll: function(selector) {
                const el = this.querySelector(selector);
                return el ? [el] : [];
            },
            createElement: function(tag) {
                return createMockElement("", tag);
            }
        };
    }

    class MockEvent {
        constructor(type, options = {}) {
            this.type = type;
            this.bubbles = !!options.bubbles;
            this.cancelable = !!options.cancelable;
            this.defaultPrevented = false;
            this.target = null;
            for (const key in options) {
                this[key] = options[key];
            }
        }
        preventDefault() {
            this.defaultPrevented = true;
        }
        stopPropagation() {}
    }
    if (!global.Event) {
        global.Event = MockEvent;
    }
    if (!global.PageTransitionEvent) {
        class MockPageTransitionEvent extends MockEvent {
            constructor(type, options = {}) {
                super(type, options);
                this.persisted = !!options.persisted;
            }
        }
        global.PageTransitionEvent = MockPageTransitionEvent;
    }

    if (!global.document.documentElement) {
        global.document.documentElement = createMockElement("html", "", "html");
    }
    if (!("visibilityState" in global.document)) {
        global.document.visibilityState = "visible";
    }
    if (!("activeElement" in global.document)) {
        global.document.activeElement = null;
    }
    const documentListeners = new Map();
    global.document.addEventListener = function(event, cb) {
        if (!documentListeners.has(event)) documentListeners.set(event, []);
        documentListeners.get(event).push(cb);
    };
    global.document.removeEventListener = function(event, cb) {
        if (documentListeners.has(event)) {
            const list = documentListeners.get(event);
            const idx = list.indexOf(cb);
            if (idx >= 0) list.splice(idx, 1);
        }
    };
    global.document.dispatchEvent = function(evt) {
        evt.target = global.document;
        if (documentListeners.has(evt.type)) {
            for (const cb of [...documentListeners.get(evt.type)]) {
                cb(evt);
            }
        }
        return !evt.defaultPrevented;
    };

    if (!global.navigator) {
        global.navigator = {
            clipboard: {
                writeText: async function(text) {
                    global._lastCopiedText = text;
                }
            }
        };
    }

    if (!global.window) {
        global.window = global;
    }
    global.window.location = global.window.location || {
        origin: "http://localhost:8080",
        pathname: "/",
        hash: ""
    };
    global.window.history = global.window.history || {
        replaceState: function() {}
    };
    global.window.innerHeight = 844;
    global.window.innerWidth = 390;
    global.window.scrollTo = function(x, y) {
        global.window.scrollX = x;
        global.window.scrollY = y;
    };
    const windowListeners = new Map();
    global.window.addEventListener = function(event, cb) {
        if (!windowListeners.has(event)) windowListeners.set(event, []);
        windowListeners.get(event).push(cb);
    };
    global.window.removeEventListener = function(event, cb) {
        if (windowListeners.has(event)) {
            const list = windowListeners.get(event);
            const idx = list.indexOf(cb);
            if (idx >= 0) list.splice(idx, 1);
        }
    };
    global.window.dispatchEvent = function(evt) {
        evt.target = global.window;
        if (windowListeners.has(evt.type)) {
            for (const cb of [...windowListeners.get(evt.type)]) {
                cb(evt);
            }
        }
        return !evt.defaultPrevented;
    };

    if (!global.getComputedStyle) {
        global.getComputedStyle = function(el) {
            if (!el) return {};
            const res = {};
            if (el.style && el.style._props) {
                for (const [k, v] of el.style._props.entries()) {
                    res[k] = v;
                }
            }
            if (el.classList && el.classList.contains("message-bubble")) {
                res.position = res.position || "relative";
                res.minWidth = res["min-width"] || "135px";
                res.minHeight = res["min-height"] || "44px";
            }
            if (el.classList && el.classList.contains("btn-message-burn")) {
                res.position = "relative";
                res.marginLeft = "auto";
                res["margin-left"] = "auto";
                res.width = "22px";
                res.height = "22px";
            }
            if (el.classList && el.classList.contains("message-header")) {
                res.display = "flex";
                res.alignItems = "center";
                res.justifyContent = "space-between";
                res.minHeight = "22px";
            }
            if (el.classList && (el.classList.contains("system-temporary") || el.classList.contains("transient-status") || (el.querySelector && el.querySelector(".ticking-dots")))) {
                res.gap = "0px";
            } else if (el.classList && el.classList.contains("system-message")) {
                res.gap = "8px";
            }
            if (el.classList && el.classList.contains("ticking-dots")) {
                res.marginLeft = "0px";
                res["margin-left"] = "0px";
            }
            return res;
        };
        global.window.getComputedStyle = global.getComputedStyle;
    }

    // Mock WebSocket and fetch for signaling
    const signalingRooms = new Map();

    class MockWebSocket {
        constructor(url) {
            this.url = url;
            this.readyState = 1; // OPEN
            this.onmessage = null;
            this.onclose = null;
            this.onerror = null;

            const m = url && url.match(/\/host\/([a-zA-Z0-9_-]+)/);
            if (m) {
                this.doId = m[1];
                signalingRooms.set(this.doId, this);
            }
        }
        send(data) {}
        close() {
            this.readyState = 3;
            if (this.doId && signalingRooms.get(this.doId) === this) {
                signalingRooms.delete(this.doId);
            }
            if (this.onclose) this.onclose();
        }
    }
    global.WebSocket = MockWebSocket;
    global._signalingRooms = signalingRooms;

    global.fetch = async function(url, options = {}) {
        const m = url && url.match(/\/drop\/([a-zA-Z0-9_-]+)\//);
        if (m) {
            const doId = m[1];
            const ws = signalingRooms.get(doId);
            if (ws && ws.onmessage) {
                const body = options.body;
                setTimeout(() => {
                    if (ws.onmessage) {
                        ws.onmessage({ data: body });
                    }
                }, 5);
            }
            return {
                ok: true,
                status: 200,
                json: async () => ({ success: true })
            };
        }
        return {
            ok: true,
            status: 200,
            json: async () => ({})
        };
    };

    // 3. Mock WebRTC signalling bus
    const connectionRegistry = new Map(); // id -> RTCPeerConnection

    class MockRTCDataChannel {
        constructor(label, remotePeerConnection) {
            this.label = label;
            this.readyState = "connecting";
            this.remotePeerConnection = remotePeerConnection;
            this.peerDataChannel = null;
            this.onopen = null;
            this.onclose = null;
            this.onerror = null;
            this.onmessage = null;
        }

        _snapOpen(partner) {
            this.peerDataChannel = partner;
            this.readyState = "open";
            if (this.onopen) this.onopen();
        }

        send(data) {
            if (this.readyState !== "open") {
                throw new Error("RTCDataChannel not open");
            }
            if (this.peerDataChannel && this.peerDataChannel.readyState === "open") {
                const partner = this.peerDataChannel;
                setTimeout(() => {
                    if (partner.onmessage) {
                        partner.onmessage({ data });
                    }
                }, 1);
            }
        }

        close() {
            if (this.readyState === "closed") return;
            this.readyState = "closed";
            if (this.onclose) this.onclose();
            if (this.peerDataChannel && this.peerDataChannel.readyState !== "closed") {
                const partner = this.peerDataChannel;
                this.peerDataChannel = null;
                partner.close();
            }
        }
    }

    class MockRTCSessionDescription {
        constructor(desc) {
            this.type = desc.type;
            this.sdp = desc.sdp;
        }
    }
    global.RTCSessionDescription = MockRTCSessionDescription;

    let pcIdCounter = 0;
    class MockRTCPeerConnection {
        constructor(config) {
            this.id = "pc-" + (++pcIdCounter);
            this.config = config;
            this.connectionState = "new";
            this.iceConnectionState = "new";
            this.iceGatheringState = "new";
            this.signalingState = "stable";
            this.localDescription = null;
            this.remoteDescription = null;
            this.dataChannels = [];
            this.remotePc = null;

            this.onconnectionstatechange = null;
            this.oniceconnectionstatechange = null;
            this.ondatachannel = null;
            this.addEventListener = function(event, cb) {
                if (event === "icegatheringstatechange") {
                    this._onIceGather = cb;
                }
            };
            this.removeEventListener = function() {};

            connectionRegistry.set(this.id, this);
        }

        createDataChannel(label, options) {
            const dc = new MockRTCDataChannel(label, this);
            this.dataChannels.push(dc);
            return dc;
        }

        async createOffer() {
            return new MockRTCSessionDescription({
                type: "offer",
                sdp: `v=0\r\no=- ${Date.now()} 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=mock-pc-id:${this.id}\r\n`
            });
        }

        async createAnswer() {
            return new MockRTCSessionDescription({
                type: "answer",
                sdp: `v=0\r\no=- ${Date.now()} 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=mock-pc-id:${this.id}\r\n`
            });
        }

        async setLocalDescription(desc) {
            if (desc.type === "offer") {
                this.signalingState = "have-local-offer";
            } else if (desc.type === "answer") {
                this.signalingState = "stable";
            }
            this.localDescription = desc;
            this.iceGatheringState = "complete";
            if (this._onIceGather) this._onIceGather();
        }

        async setRemoteDescription(desc) {
            if (desc.type === "offer") {
                if (this.signalingState !== "stable") {
                    throw new Error("InvalidStateError: Failed to execute 'setRemoteDescription' on 'RTCPeerConnection': Cannot set remote offer in state " + this.signalingState);
                }
                this.signalingState = "have-remote-offer";
            } else if (desc.type === "answer") {
                if (this.signalingState !== "have-local-offer") {
                    throw new Error("InvalidStateError: Failed to execute 'setRemoteDescription' on 'RTCPeerConnection': Cannot set remote answer in state " + this.signalingState);
                }
                this.signalingState = "stable";
            }
            this.remoteDescription = desc;
            // Parse remote PC ID from mock SDP
            const match = desc.sdp && desc.sdp.match(/a=mock-pc-id:([^\r\n]+)/);
            if (match) {
                const remoteId = match[1];
                const remotePc = connectionRegistry.get(remoteId);
                if (remotePc) {
                    this.remotePc = remotePc;
                    remotePc.remotePc = this;

                    // If this is an answer, link data channels and open them
                    if (desc.type === "answer") {
                        this.connectionState = "connected";
                        this.iceConnectionState = "connected";
                        remotePc.connectionState = "connected";
                        remotePc.iceConnectionState = "connected";

                        // Connect channels
                        for (const localDc of this.dataChannels) {
                            const remoteDc = new MockRTCDataChannel(localDc.label, remotePc);
                            remotePc.dataChannels.push(remoteDc);
                            if (remotePc.ondatachannel) {
                                remotePc.ondatachannel({ channel: remoteDc });
                            }
                            localDc._snapOpen(remoteDc);
                            remoteDc._snapOpen(localDc);
                        }
                    }
                }
            }
        }

        close() {
            this.connectionState = "closed";
            this.iceConnectionState = "closed";
            this.signalingState = "closed";
            connectionRegistry.delete(this.id);
            for (const dc of this.dataChannels) {
                dc.close();
            }
            if (this.onconnectionstatechange) this.onconnectionstatechange();
            if (this.oniceconnectionstatechange) this.oniceconnectionstatechange();
        }
    }
    global.RTCPeerConnection = MockRTCPeerConnection;

})(typeof globalThis !== 'undefined' ? globalThis : this);
