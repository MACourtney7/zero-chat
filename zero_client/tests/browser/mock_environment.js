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

    global.mockIceFetchCount = 0;
    global.mockIceResponse = null;

    global.fetch = async function(url, options = {}) {
        if (url && (url.endsWith("/ice") || url.includes("/ice?") || url.includes("/ice/"))) {
            global.mockIceFetchCount = (global.mockIceFetchCount || 0) + 1;
            if (global.mockIceResponse !== null && global.mockIceResponse !== undefined) {
                if (typeof global.mockIceResponse === "function") {
                    return global.mockIceResponse(url, options);
                }
                return global.mockIceResponse;
            }
            return {
                ok: true,
                status: 200,
                json: async () => ({
                    iceServers: [
                        { urls: "stun:stun.cloudflare.com:3478" },
                        { urls: "turn:turn.cloudflare.com:3478", username: "mock", credential: "pass" }
                    ]
                })
            };
        }

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
            this.onicecandidate = null;
            this.onicegatheringstatechange = null;
            this.ondatachannel = null;
            this._listeners = new Map();
            this.addEventListener = function(event, cb) {
                if (!this._listeners.has(event)) this._listeners.set(event, []);
                this._listeners.get(event).push(cb);
                if (event === "icegatheringstatechange") {
                    this._onIceGather = cb;
                }
            };
            this.removeEventListener = function(event, cb) {
                if (this._listeners && this._listeners.has(event)) {
                    this._listeners.set(event, this._listeners.get(event).filter(x => x !== cb));
                }
            };
            this.dispatchEvent = function(ev) {
                const list = (this._listeners && this._listeners.get(ev.type)) || [];
                for (const cb of list) cb(ev);
            };

            connectionRegistry.set(this.id, this);
        }

        static generateCandidate(type = "host", options = {}) {
            const ufrag = options.ufrag || Math.random().toString(36).slice(2, 8);
            const port = options.port || (type === "relay" ? 50001 : 50000);
            const ip = options.ip || (type === "host" ? "192.168.1.100" : "1.2.3.4");
            const raddr = options.raddr || "192.168.1.100";
            const rport = options.rport || "50000";
            if (type === "host") {
                return `candidate:${ufrag} 1 UDP 2122260223 ${ip} ${port} typ host`;
            } else if (type === "srflx") {
                return `candidate:${ufrag} 1 UDP 2122260222 ${ip} ${port} typ srflx raddr ${raddr} rport ${rport}`;
            } else if (type === "relay") {
                return `candidate:${ufrag} 1 UDP 2122260221 ${ip} ${port} typ relay raddr ${raddr} rport ${rport}`;
            }
            return `candidate:${ufrag} 1 UDP 2122260223 ${ip} ${port} typ ${type}`;
        }

        generateMockCandidate(type = "host", options = {}) {
            const candStr = MockRTCPeerConnection.generateCandidate(type, options);
            if (this.localDescription && this.localDescription.sdp) {
                if (!this.localDescription.sdp.includes(candStr)) {
                    this.localDescription.sdp += `a=${candStr}\r\n`;
                }
            }
            const ev = { type: "icecandidate", candidate: { candidate: candStr } };
            this.dispatchEvent(ev);
            if (typeof this.onicecandidate === "function") {
                this.onicecandidate(ev);
            }
            return candStr;
        }

        generateMockCandidates(types = ["host", "srflx", "relay"]) {
            return types.map(t => this.generateMockCandidate(t));
        }

        completeIceGathering() {
            this.iceGatheringState = "complete";
            const evState = { type: "icegatheringstatechange" };
            this.dispatchEvent(evState);
            if (typeof this.onicegatheringstatechange === "function") {
                this.onicegatheringstatechange(evState);
            }
            const evCand = { type: "icecandidate", candidate: null };
            this.dispatchEvent(evCand);
            if (typeof this.onicecandidate === "function") {
                this.onicecandidate(evCand);
            }
        }

        createDataChannel(label, options) {
            const dc = new MockRTCDataChannel(label, this);
            this.dataChannels.push(dc);
            return dc;
        }

        async createOffer() {
            let sdp = `v=0\r\no=- ${Date.now()} 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=mock-pc-id:${this.id}\r\n`;
            if (this.config && Array.isArray(this.config.iceServers)) {
                const hasTurn = this.config.iceServers.some(s => {
                    const urls = Array.isArray(s.urls) ? s.urls : [s.urls || ""];
                    return urls.some(u => typeof u === "string" && (u.startsWith("turn:") || u.startsWith("turns:")));
                });
                if (hasTurn) {
                    sdp += `a=candidate:1 1 UDP 2122260223 192.168.1.100 50000 typ host\r\n`;
                    sdp += `a=candidate:2 1 UDP 2122260222 1.2.3.4 50000 typ srflx raddr 192.168.1.100 rport 50000\r\n`;
                    sdp += `a=candidate:3 1 UDP 2122260221 1.2.3.4 50001 typ relay raddr 1.2.3.4 rport 50000\r\n`;
                }
            }
            return new MockRTCSessionDescription({
                type: "offer",
                sdp
            });
        }

        async createAnswer() {
            let sdp = `v=0\r\no=- ${Date.now()} 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=mock-pc-id:${this.id}\r\n`;
            if (this.config && Array.isArray(this.config.iceServers)) {
                const hasTurn = this.config.iceServers.some(s => {
                    const urls = Array.isArray(s.urls) ? s.urls : [s.urls || ""];
                    return urls.some(u => typeof u === "string" && (u.startsWith("turn:") || u.startsWith("turns:")));
                });
                if (hasTurn) {
                    sdp += `a=candidate:1 1 UDP 2122260223 192.168.1.100 50000 typ host\r\n`;
                    sdp += `a=candidate:2 1 UDP 2122260222 1.2.3.4 50000 typ srflx raddr 192.168.1.100 rport 50000\r\n`;
                    sdp += `a=candidate:3 1 UDP 2122260221 1.2.3.4 50001 typ relay raddr 1.2.3.4 rport 50000\r\n`;
                }
            }
            return new MockRTCSessionDescription({
                type: "answer",
                sdp
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

    // 4. Mock WASM Crypto Bridge (ticket-047)
    function sha256Bytes(data) {
        function rightRotate(value, amount) {
            return (value >>> amount) | (value << (32 - amount));
        }
        let i, j;
        const words = [];
        const asciiBitLength = data.length * 8;

        const hash = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
            0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19
        ];

        const k = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
        ];

        const w = new Array(64);
        for (i = 0; i < data.length; i++) {
            words[i >> 2] = (words[i >> 2] || 0) | ((data[i] & 0xff) << ((3 - (i % 4)) * 8));
        }
        words[asciiBitLength >> 5] = (words[asciiBitLength >> 5] || 0) | (0x80 << ((3 - (asciiBitLength % 4)) * 8));
        const totalWords = (((asciiBitLength + 64) >> 9) << 4) + 16;
        for (i = 0; i < totalWords; i++) {
            if (!words[i]) words[i] = 0;
        }
        words[totalWords - 1] = asciiBitLength;

        for (i = 0; i < totalWords; i += 16) {
            let a = hash[0], b = hash[1], c = hash[2], d = hash[3],
                e = hash[4], f = hash[5], g = hash[6], h = hash[7];

            for (j = 0; j < 64; j++) {
                if (j < 16) {
                    w[j] = words[i + j] | 0;
                } else {
                    const s0 = rightRotate(w[j - 15], 7) ^ rightRotate(w[j - 15], 18) ^ (w[j - 15] >>> 3);
                    const s1 = rightRotate(w[j - 2], 17) ^ rightRotate(w[j - 2], 19) ^ (w[j - 2] >>> 10);
                    w[j] = (w[j - 16] + s0 + w[j - 7] + s1) | 0;
                }

                const s1 = rightRotate(e, 6) ^ rightRotate(e, 11) ^ rightRotate(e, 25);
                const ch = (e & f) ^ ((~e) & g);
                const temp1 = (h + s1 + ch + k[j] + w[j]) | 0;
                const s0 = rightRotate(a, 2) ^ rightRotate(a, 13) ^ rightRotate(a, 22);
                const maj = (a & b) ^ (a & c) ^ (b & c);
                const temp2 = (s0 + maj) | 0;

                h = g;
                g = f;
                f = e;
                e = (d + temp1) | 0;
                d = c;
                c = b;
                b = a;
                a = (temp1 + temp2) | 0;
            }

            hash[0] = (hash[0] + a) | 0;
            hash[1] = (hash[1] + b) | 0;
            hash[2] = (hash[2] + c) | 0;
            hash[3] = (hash[3] + d) | 0;
            hash[4] = (hash[4] + e) | 0;
            hash[5] = (hash[5] + f) | 0;
            hash[6] = (hash[6] + g) | 0;
            hash[7] = (hash[7] + h) | 0;
        }

        const out = [];
        for (i = 0; i < 8; i++) {
            for (j = 3; j >= 0; j--) {
                out.push((hash[i] >>> (j * 8)) & 0xff);
            }
        }
        return new Uint8Array(out);
    }

    function hmacSha256(key, message) {
        let k = (typeof key === "string") ? new TextEncoder().encode(key) : new Uint8Array(key);
        if (k.length > 64) {
            k = sha256Bytes(k);
        }
        const paddedKey = new Uint8Array(64);
        paddedKey.set(k, 0);
        const oKeyPad = new Uint8Array(64);
        const iKeyPad = new Uint8Array(64);
        for (let i = 0; i < 64; i++) {
            oKeyPad[i] = paddedKey[i] ^ 0x5c;
            iKeyPad[i] = paddedKey[i] ^ 0x36;
        }
        const msg = (typeof message === "string") ? new TextEncoder().encode(message) : new Uint8Array(message);
        const inner = new Uint8Array(64 + msg.length);
        inner.set(iKeyPad, 0);
        inner.set(msg, 64);
        const innerHash = sha256Bytes(inner);

        const outer = new Uint8Array(64 + 32);
        outer.set(oKeyPad, 0);
        outer.set(innerHash, 64);
        const outerHash = sha256Bytes(outer);

        let hex = "";
        for (let i = 0; i < outerHash.length; i++) {
            let h = outerHash[i].toString(16);
            if (h.length < 2) h = "0" + h;
            hex += h;
        }
        return hex;
    }

    function deriveMockSharedSecret(pub1, pub2) {
        const p1 = new Uint8Array(pub1);
        const p2 = new Uint8Array(pub2);
        let cmp = 0;
        for (let i = 0; i < 32; i++) {
            const b1 = p1[i] || 0;
            const b2 = p2[i] || 0;
            if (b1 < b2) { cmp = -1; break; }
            if (b1 > b2) { cmp = 1; break; }
        }
        const first = cmp <= 0 ? p1 : p2;
        const second = cmp <= 0 ? p2 : p1;
        const secret = new Uint8Array(32);
        for (let i = 0; i < 32; i++) {
            const b1 = first[i] || 0;
            const b2 = second[(i * 7 + 13) % (second.length || 1)] || 0;
            secret[i] = (b1 ^ b2 ^ ((i * 37) & 0xff)) & 0xff;
        }
        return secret;
    }

    function computeMockMac(key, nonce, encBytes) {
        const tag = new Uint8Array(16);
        for (let i = 0; i < 16; i++) {
            tag[i] = (key[i] ^ key[i + 16] ^ nonce[i % 12]) & 0xff;
        }
        for (let i = 0; i < encBytes.length; i++) {
            const b = encBytes[i];
            const idx = i % 16;
            tag[idx] = (tag[idx] * 33 + b + (i & 0xff)) & 0xff;
        }
        for (let i = 0; i < 16; i++) {
            tag[i] = (tag[i] ^ key[(i * 3 + 5) % 32]) & 0xff;
        }
        return tag;
    }

    function computeMockSignature(pubKey, data) {
        const sig = new Uint8Array(64);
        const pub = new Uint8Array(pubKey || 32);
        const d = new Uint8Array(data);
        for (let i = 0; i < 32; i++) {
            sig[i] = (pub[i] ^ ((i * 41 + 19) & 0xff)) & 0xff;
            sig[i + 32] = (pub[31 - i] ^ ((i * 59 + 83) & 0xff)) & 0xff;
        }
        for (let i = 0; i < d.length; i++) {
            const b = d[i];
            const idx1 = i % 32;
            const idx2 = 32 + ((i * 7) % 32);
            sig[idx1] = (sig[idx1] * 31 + b + (i & 0xff)) & 0xff;
            sig[idx2] = (sig[idx2] * 37 + b + ((i >> 3) & 0xff)) & 0xff;
        }
        for (let i = 0; i < 64; i++) {
            sig[i] = (sig[i] ^ sig[(i + 17) % 64] ^ 0xa5) & 0xff;
        }
        return sig;
    }

    class MockWasmCryptoSession {
        constructor() {
            this._x25519Pub = crypto.getRandomValues(new Uint8Array(32));
            this._ed25519Pub = crypto.getRandomValues(new Uint8Array(32));
            this.peerSessions = new Map();
        }

        static new() {
            return new MockWasmCryptoSession();
        }

        x25519_public_key_bytes() {
            return new Uint8Array(this._x25519Pub);
        }

        ed25519_public_key_bytes() {
            return new Uint8Array(this._ed25519Pub);
        }

        register_peer(peerId, peerX25519Pub, peerEd25519Pub) {
            if (!peerX25519Pub || peerX25519Pub.length !== 32) {
                throw new Error("Invalid X25519 public key length: expected 32, got " + (peerX25519Pub ? peerX25519Pub.length : 0));
            }
            const x25519 = new Uint8Array(peerX25519Pub);
            let ed25519 = null;
            if (peerEd25519Pub && peerEd25519Pub.length > 0) {
                if (peerEd25519Pub.length !== 32) {
                    throw new Error("Invalid Ed25519 public key length: expected 32, got " + peerEd25519Pub.length);
                }
                ed25519 = new Uint8Array(peerEd25519Pub);
            }
            const sharedSecret = deriveMockSharedSecret(this._x25519Pub, x25519);
            this.peerSessions.set(peerId, {
                x25519,
                ed25519,
                sharedSecret
            });
        }

        has_peer_session(peerId) {
            return this.peerSessions.has(peerId);
        }

        remove_peer(peerId) {
            this.peerSessions.delete(peerId);
        }

        seal_for_peer(peerId, plaintextBytes) {
            const entry = this.peerSessions.get(peerId);
            if (!entry) {
                throw new Error("No peer session found for '" + peerId + "'");
            }
            const pt = (typeof plaintextBytes === "string")
                ? new TextEncoder().encode(plaintextBytes)
                : new Uint8Array(plaintextBytes);

            const nonce = crypto.getRandomValues(new Uint8Array(12));
            const enc = new Uint8Array(pt.length);
            for (let i = 0; i < pt.length; i++) {
                enc[i] = (pt[i] ^ entry.sharedSecret[i % 32] ^ nonce[i % 12] ^ ((i * 31 + 17) & 0xff)) & 0xff;
            }

            const tag = computeMockMac(entry.sharedSecret, nonce, enc);
            const ciphertext = new Uint8Array(enc.length + 16);
            ciphertext.set(enc, 0);
            ciphertext.set(tag, enc.length);

            const toSign = new Uint8Array(nonce.length + ciphertext.length);
            toSign.set(nonce, 0);
            toSign.set(ciphertext, nonce.length);
            const signature = computeMockSignature(this._ed25519Pub, toSign);

            return JSON.stringify({
                nonce: Array.from(nonce),
                ciphertext: Array.from(ciphertext),
                signature: Array.from(signature)
            });
        }

        open_from_peer(peerId, sealedFrameJson) {
            let frame = sealedFrameJson;
            if (typeof frame === "string") {
                try {
                    frame = JSON.parse(frame);
                } catch (e) {
                    throw new Error("Invalid SealedFrame JSON: " + e.message);
                }
            }
            const entry = this.peerSessions.get(peerId);
            if (!entry) {
                throw new Error("No peer session found for '" + peerId + "'");
            }

            if (frame.signature && entry.ed25519) {
                const sig = new Uint8Array(frame.signature);
                if (sig.length !== 64) {
                    throw new Error("Ed25519 signature verification failed: invalid signature length");
                }
                const nonce = new Uint8Array(frame.nonce);
                const ct = new Uint8Array(frame.ciphertext);
                const toSign = new Uint8Array(nonce.length + ct.length);
                toSign.set(nonce, 0);
                toSign.set(ct, nonce.length);
                const expectedSig = computeMockSignature(entry.ed25519, toSign);
                let sigMatch = true;
                for (let i = 0; i < 64; i++) {
                    if (sig[i] !== expectedSig[i]) {
                        sigMatch = false;
                        break;
                    }
                }
                if (!sigMatch) {
                    throw new Error("Ed25519 signature verification failed");
                }
            }

            const ct = new Uint8Array(frame.ciphertext);
            if (ct.length < 16) {
                throw new Error("DecryptionFailed: ciphertext too short");
            }
            const enc = ct.slice(0, ct.length - 16);
            const tag = ct.slice(ct.length - 16);
            const nonce = new Uint8Array(frame.nonce);
            const expectedTag = computeMockMac(entry.sharedSecret, nonce, enc);
            let macMatch = true;
            for (let i = 0; i < 16; i++) {
                if (tag[i] !== expectedTag[i]) {
                    macMatch = false;
                    break;
                }
            }
            if (!macMatch) {
                throw new Error("DecryptionFailed: MAC tag verification failed");
            }

            const pt = new Uint8Array(enc.length);
            for (let i = 0; i < enc.length; i++) {
                pt[i] = (enc[i] ^ entry.sharedSecret[i % 32] ^ nonce[i % 12] ^ ((i * 31 + 17) & 0xff)) & 0xff;
            }
            return pt;
        }

        seal_drop_sdp(hostPeerId, hostX25519Pub, hostEd25519Pub, sdpPlaintext) {
            this.register_peer(hostPeerId, hostX25519Pub, hostEd25519Pub);
            const sdpBytes = (typeof sdpPlaintext === "string")
                ? new TextEncoder().encode(sdpPlaintext)
                : new Uint8Array(sdpPlaintext);
            return this.seal_for_peer(hostPeerId, sdpBytes);
        }

        open_drop_sdp(joinerPeerId, joinerX25519Pub, joinerEd25519Pub, sealedFrameJson) {
            this.register_peer(joinerPeerId, joinerX25519Pub, joinerEd25519Pub);
            const plaintextBytes = this.open_from_peer(joinerPeerId, sealedFrameJson);
            return new TextDecoder().decode(plaintextBytes);
        }

        sign_bytes(data) {
            const d = (typeof data === "string") ? new TextEncoder().encode(data) : new Uint8Array(data);
            return computeMockSignature(this._ed25519Pub, d);
        }

        verify_peer_signature(peerId, data, sig) {
            const entry = this.peerSessions.get(peerId);
            if (!entry) {
                throw new Error("No peer session found for '" + peerId + "'");
            }
            if (!entry.ed25519) {
                return false;
            }
            const s = new Uint8Array(sig);
            if (s.length !== 64) {
                return false;
            }
            const d = (typeof data === "string") ? new TextEncoder().encode(data) : new Uint8Array(data);
            const expected = computeMockSignature(entry.ed25519, d);
            for (let i = 0; i < 64; i++) {
                if (s[i] !== expected[i]) {
                    return false;
                }
            }
            return true;
        }

        static compute_hmac_hex(secret, inviteUuid) {
            return hmacSha256(secret, inviteUuid);
        }

        compute_hmac_hex(secret, inviteUuid) {
            return hmacSha256(secret, inviteUuid);
        }

        burn_all() {
            this.peerSessions.clear();
            if (this._x25519Pub) this._x25519Pub.fill(0);
            if (this._ed25519Pub) this._ed25519Pub.fill(0);
        }
    }

    global.initWasm = async function() {
        return Promise.resolve();
    };
    global.WasmCryptoSession = MockWasmCryptoSession;

    if (typeof globalThis !== "undefined") {
        globalThis.initWasm = global.initWasm;
        globalThis.WasmCryptoSession = MockWasmCryptoSession;
    }

})(typeof globalThis !== 'undefined' ? globalThis : this);
