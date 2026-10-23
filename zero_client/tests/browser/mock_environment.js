// Mock DOM & WebRTC environment for testing zero_client index.html mesh logic
(function(global) {
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
        return {
            id: id,
            className: className,
            classList: {
                add: function(c) {},
                remove: function(c) {},
                contains: function(c) { return false; }
            },
            textContent: "",
            innerHTML: "",
            disabled: false,
            placeholder: "",
            style: { setProperty: function() {} },
            children: [],
            appendChild: function(child) {
                this.children.push(child);
                return child;
            },
            remove: function() {
                if (this.parentNode && this.parentNode.children) {
                    const idx = this.parentNode.children.indexOf(this);
                    if (idx >= 0) this.parentNode.children.splice(idx, 1);
                }
            },
            querySelectorAll: function() { return []; },
            querySelector: function() { return null; },
            setAttribute: function() {},
            getAttribute: function() { return null; },
            focus: function() {},
            value: "",
            addEventListener: function(event, cb) {},
            removeEventListener: function(event, cb) {}
        };
    }

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
            this.localDescription = desc;
            this.iceGatheringState = "complete";
            if (this._onIceGather) this._onIceGather();
        }

        async setRemoteDescription(desc) {
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
