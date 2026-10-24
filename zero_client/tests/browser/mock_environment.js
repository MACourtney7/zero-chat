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
    function createMockElement(id = "", className = "") {
        const attributes = new Map();
        let classListArray = (className || "").split(" ").filter(Boolean);
        const listeners = new Map();
        const element = {
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
                }
            },
            _textContent: "",
            get textContent() {
                if (this._textContent !== "") return this._textContent;
                if (this.children && this.children.length > 0) {
                    return this.children.map(c => c.textContent || "").join("");
                }
                return "";
            },
            set textContent(val) {
                this._textContent = val;
            },
            _innerHTML: "",
            get innerHTML() {
                return this._innerHTML;
            },
            set innerHTML(val) {
                this._innerHTML = val;
                if (val === "") {
                    this.children = [];
                }
            },
            disabled: false,
            placeholder: "",
            style: { setProperty: function() {} },
            offsetWidth: 100,
            children: [],
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
            },
            querySelectorAll: function(selector) {
                const results = [];
                function traverse(el) {
                    for (const ch of el.children) {
                        let match = false;
                        if (selector.startsWith(".") && selector.includes("[")) {
                            const m = selector.match(/^\.([a-zA-Z0-9_-]+)\[([a-zA-Z0-9_-]+)="([^"]+)"\]$/);
                            if (m) {
                                const [_, cls, attrName, attrVal] = m;
                                if (ch.classList.contains(cls) && ch.getAttribute(attrName) === attrVal) {
                                    match = true;
                                }
                            }
                        } else if (selector.startsWith(".message-bubble:not(.message-outgoing)")) {
                            if (ch.classList.contains("message-bubble") && !ch.classList.contains("message-outgoing")) {
                                match = true;
                            }
                        } else if (selector.startsWith(".")) {
                            const classes = selector.slice(1).split(".").filter(Boolean);
                            if (classes.length > 0 && classes.every(c => ch.classList.contains(c))) match = true;
                        }
                        if (match) results.push(ch);
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
            focus: function() {},
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
            click: function() {
                if (listeners.has("click")) {
                    for (const cb of listeners.get("click")) {
                        cb({ target: this, preventDefault: () => {} });
                    }
                }
            }
        };
        return element;
    }
    global.createMockElement = createMockElement;

    const elementRegistry = new Map();
    if (!global.document) {
        global.document = {
            getElementById: function(id) {
                if (!elementRegistry.has(id)) {
                    elementRegistry.set(id, createMockElement(id));
                }
                return elementRegistry.get(id);
            },
            createElement: function(tag) {
                return createMockElement("", tag);
            }
        };
    }

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
        global.window.location = {
            origin: "http://localhost:8080",
            pathname: "/",
            hash: ""
        };
        global.window.history = {
            replaceState: function() {}
        };
        global.window.addEventListener = function() {};
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
