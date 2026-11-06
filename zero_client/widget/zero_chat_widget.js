/**
 * ZeroChatWidget - Reusable Glassmorphic Chat Widget
 *
 * Provides a canonical, modular chat interface supporting both standalone
 * and embedded modes with ephemeral aging, dissolve animation, per-message
 * flame burn, peer-color indicators, and distributed session burn.
 */

export const PEER_COLOR_PALETTE = [
  '#10b981', // emerald
  '#3b82f6', // blue
  '#f59e0b', // amber
  '#ec4899', // pink
  '#8b5cf6', // violet
  '#06b6d4', // cyan
  '#14b8a6', // teal
  '#f97316'  // orange
];

function generateUuid() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return 'msg-' + Math.random().toString(36).substring(2, 10) + '-' + Date.now().toString(36);
}

function escapeHTML(str) {
  if (typeof str !== 'string') return String(str ?? '');
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

export class ZeroChatWidget {
  /**
   * @param {Object} [options={}]
   * @param {HTMLElement|string} [options.container] Parent DOM container or selector
   * @param {'embedded'|'standalone'} [options.mode='embedded'] Widget presentation mode
   * @param {Function} [options.onSendMessage] Callback invoked when message is submitted (text)
   * @param {Function} [options.onBurnMessage] Callback invoked when per-message flame button is clicked (messageUuid)
   * @param {Function} [options.onBurnSession] Callback invoked when header burn button is clicked ()
   * @param {number} [options.agingMs=70000] Duration before message starts aging (ms)
   * @param {number} [options.dissolveMs=90000] Duration before message dissolves and is removed (ms)
   */
  constructor(options = {}) {
    const {
      container = null,
      mode = 'embedded',
      onSendMessage = null,
      onBurnMessage = null,
      onBurnSession = null,
      onUnreadCountChange = null,
      agingMs = 70000,
      dissolveMs = 90000
    } = options;

    this.mode = mode;
    this.onSendMessage = onSendMessage;
    this.onBurnMessage = onBurnMessage;
    this.onBurnSession = onBurnSession;
    this.onUnreadCountChange = onUnreadCountChange;
    this.agingMs = typeof agingMs === 'number' ? agingMs : 70000;
    this.dissolveMs = typeof dissolveMs === 'number' ? dissolveMs : 90000;

    this.peerColorMap = new Map();
    this.peerMetaMap = new Map(); // peerId -> { name, color }
    this.messagesMap = new Map(); // messageUuid -> record
    this.currentPeers = [];
    this.currentPeerCount = 0;
    this.unreadCount = 0;

    this.container = typeof container === 'string' && typeof document !== 'undefined'
      ? document.querySelector(container)
      : container;

    this.element = null;
    this.header = null;
    this.statusEl = null;
    this.peerCountEl = null;
    this.peerStripEl = null;
    this.burnHeaderBtn = null;
    this.messagesContainer = null;
    this.form = null;
    this.input = null;
    this.sendBtn = null;

    this._initDOM();
  }

  _initDOM() {
    if (typeof document === 'undefined') return;

    const widgetEl = document.createElement('div');
    widgetEl.className = `zero-chat-widget zero-chat-${this.mode}`;

    widgetEl.innerHTML = `
      <div class="zero-chat-header">
        <div class="peer-pill" id="proximity-chat-status">
          <span class="peer-lock-icon">🔒</span>
          <span class="peer-count" id="peer-count">0 nearby</span>
          <span class="peer-strip" id="peer-strip"></span>
        </div>
        <button type="button" class="btn-header-burn" title="Burn all messages" aria-label="Burn all messages">🔥</button>
      </div>
      <div class="zero-chat-messages" id="proximity-chat-messages"></div>
      <form class="zero-chat-input-bar" id="proximity-chat-form">
        <input type="text" class="zero-chat-input" id="proximity-chat-input" placeholder="Say something nearby..." maxlength="200" autocomplete="off" />
        <button type="submit" class="btn-send-emerald" id="proximity-chat-send" aria-label="Send">↑</button>
      </form>
    `;

    this.element = widgetEl;
    this.header = widgetEl.querySelector('.zero-chat-header');
    this.statusEl = widgetEl.querySelector('#proximity-chat-status');
    this.peerCountEl = widgetEl.querySelector('#peer-count');
    this.peerStripEl = widgetEl.querySelector('#peer-strip');
    this.burnHeaderBtn = widgetEl.querySelector('.btn-header-burn');
    this.messagesContainer = widgetEl.querySelector('#proximity-chat-messages');
    this.form = widgetEl.querySelector('#proximity-chat-form');
    this.input = widgetEl.querySelector('#proximity-chat-input');
    this.sendBtn = widgetEl.querySelector('#proximity-chat-send');

    if (this.burnHeaderBtn) {
      this.burnHeaderBtn.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        this.burnAllMessages();
      });
    }

    if (this.form) {
      this.form.addEventListener('submit', (e) => {
        e.preventDefault();
        const text = this.input ? this.input.value.trim() : '';
        if (text) {
          if (typeof this.onSendMessage === 'function') {
            this.onSendMessage(text);
          }
          if (this.input) {
            this.input.value = '';
            this.input.focus();
          }
        }
      });
    }

    if (this.container) {
      this.container.appendChild(widgetEl);
    }
  }

  /**
   * Mounts the widget into a DOM container if not mounted during construction.
   * @param {HTMLElement|string} container
   */
  mount(container) {
    if (typeof document === 'undefined') return;
    this.container = typeof container === 'string' ? document.querySelector(container) : container;
    if (this.container && this.element && !this.element.parentNode) {
      this.container.appendChild(this.element);
    }
  }

  /**
   * Retrieves or assigns a deterministic color from the palette for a given peer ID.
   * @param {string} [peerId='peer-01']
   * @returns {string} Hex color
   */
  getPeerColor(peerId = 'peer-01') {
    if (!this.peerColorMap.has(peerId)) {
      const color = PEER_COLOR_PALETTE[this.peerColorMap.size % PEER_COLOR_PALETTE.length];
      this.peerColorMap.set(peerId, color);
    }
    return this.peerColorMap.get(peerId);
  }

  /**
   * Appends a message bubble to the messages feed.
   *
   * @param {Object} options
   * @param {string} [options.senderId] Peer or local sender identifier
   * @param {string} [options.name] Display name of sender
   * @param {string} [options.senderName] Fallback display name
   * @param {string} [options.text] Message text content
   * @param {boolean} [options.isSelf] Whether message was sent by local user
   * @param {boolean} [options.isLocal] Fallback for isSelf
   * @param {string} [options.messageUuid] Unique message ID (generated if omitted)
   * @param {number} [options.timestamp] Timestamp in ms
   * @returns {HTMLElement|Object} Created bubble DOM element
   */
  appendMessage(options = {}) {
    const isSelf = Boolean(options.isSelf !== undefined ? options.isSelf : options.isLocal);
    const senderId = options.senderId || (isSelf ? 'self' : 'peer');
    const name = options.name || options.senderName || (isSelf ? 'You' : 'Peer');
    const text = options.text !== undefined ? String(options.text) : '';
    const messageUuid = options.messageUuid || options.uuid || options.id || generateUuid();
    const timestamp = options.timestamp || Date.now();

    if (typeof document === 'undefined') {
      const record = { senderId, name, text, isSelf, messageUuid, timestamp, element: null };
      this.messagesMap.set(messageUuid, record);
      return record;
    }

    const bubble = document.createElement('div');
    bubble.className = `message-bubble proximity-chat-msg ${isSelf ? 'message-outgoing' : 'message-incoming'}`;
    bubble.setAttribute('data-msg-uuid', messageUuid);
    bubble.setAttribute('data-message-id', messageUuid);
    bubble.setAttribute('data-peer-id', senderId);
    bubble.setAttribute('data-sender-id', senderId);

    const header = document.createElement('div');
    header.className = 'message-header';

    if (!isSelf) {
      const peerColor = this.getPeerColor(senderId);
      bubble.style.setProperty('--peer-color', peerColor);

      const peerDot = document.createElement('span');
      peerDot.className = 'peer-dot';
      peerDot.style.backgroundColor = peerColor;
      header.appendChild(peerDot);
    } else {
      const selfDot = document.createElement('span');
      selfDot.className = 'peer-dot self-dot';
      header.appendChild(selfDot);
    }

    const senderSpan = document.createElement('span');
    senderSpan.className = 'message-sender';
    senderSpan.textContent = name;
    header.appendChild(senderSpan);

    const burnBtn = document.createElement('button');
    burnBtn.type = 'button';
    burnBtn.className = 'btn-message-burn';
    burnBtn.setAttribute('title', 'Burn message');
    burnBtn.setAttribute('aria-label', 'Burn message');
    burnBtn.textContent = '🔥';
    burnBtn.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (typeof this.onBurnMessage === 'function') {
        this.onBurnMessage(messageUuid);
      }
      this.dissolveMessageById(messageUuid);
    });
    header.appendChild(burnBtn);

    const body = document.createElement('div');
    body.className = 'message-text';
    body.textContent = text;

    bubble.appendChild(header);
    bubble.appendChild(body);

    if (this.messagesContainer) {
      this.messagesContainer.appendChild(bubble);
      this.messagesContainer.scrollTop = this.messagesContainer.scrollHeight;
    }

    const record = {
      senderId,
      name,
      text,
      isSelf,
      messageUuid,
      timestamp,
      element: bubble,
      agingTimer: null,
      dissolveTimer: null,
      isDissolving: false
    };

    if (this.agingMs > 0) {
      record.agingTimer = setTimeout(() => {
        if (bubble.parentNode) {
          bubble.classList.add('message-aging');
        }
      }, this.agingMs);
    }

    if (this.dissolveMs > 0) {
      record.dissolveTimer = setTimeout(() => {
        this.dissolveMessageById(messageUuid);
      }, this.dissolveMs);
    }

    this.messagesMap.set(messageUuid, record);
    return bubble;
  }

  /**
   * Applies the dissolve animation to a message bubble and purges it after 200ms.
   * @param {string} messageUuid
   */
  dissolveMessageById(messageUuid) {
    if (!messageUuid) return;
    const record = this.messagesMap.get(messageUuid);
    let bubble = null;

    if (record) {
      if (record.isDissolving) return;
      record.isDissolving = true;
      if (record.agingTimer) {
        clearTimeout(record.agingTimer);
        record.agingTimer = null;
      }
      if (record.dissolveTimer) {
        clearTimeout(record.dissolveTimer);
        record.dissolveTimer = null;
      }
      bubble = record.element;
      this.messagesMap.delete(messageUuid);
    }

    if (!bubble && this.messagesContainer) {
      bubble = this.messagesContainer.querySelector(`[data-msg-uuid="${messageUuid}"], [data-message-id="${messageUuid}"]`);
    }

    if (bubble) {
      if (bubble._isDissolving) return;
      bubble._isDissolving = true;
      if (bubble._agingTimer) {
        clearTimeout(bubble._agingTimer);
        bubble._agingTimer = null;
      }
      if (bubble._dissolveTimer) {
        clearTimeout(bubble._dissolveTimer);
        bubble._dissolveTimer = null;
      }
      bubble.classList.add('message-dissolve');
      setTimeout(() => {
        if (bubble.parentNode) {
          bubble.remove();
        }
      }, 200);
    }
  }

  /**
   * Purges all message bubbles associated with a specific peer.
   * @param {string} peerId
   */
  burnPeerMessages(peerId) {
    if (!peerId) return;
    const toDissolve = [];
    for (const [uuid, record] of this.messagesMap.entries()) {
      if (record.senderId === peerId) {
        toDissolve.push(uuid);
      }
    }
    for (const uuid of toDissolve) {
      this.dissolveMessageById(uuid);
    }
    if (this.messagesContainer) {
      const matching = this.messagesContainer.querySelectorAll(`[data-peer-id="${peerId}"], [data-sender-id="${peerId}"]`);
      matching.forEach(el => {
        const uuid = el.getAttribute('data-msg-uuid') || el.getAttribute('data-message-id');
        if (uuid) {
          this.dissolveMessageById(uuid);
        } else {
          el.classList.add('message-dissolve');
          setTimeout(() => {
            if (el.parentNode) el.remove();
          }, 200);
        }
      });
    }
  }

  /**
   * Purges all messages from DOM and state, and calls onBurnSession.
   */
  burnAllMessages() {
    const uuids = Array.from(this.messagesMap.keys());
    for (const uuid of uuids) {
      this.dissolveMessageById(uuid);
    }
    if (this.messagesContainer) {
      const allBubbles = this.messagesContainer.querySelectorAll('.message-bubble, .proximity-chat-msg');
      allBubbles.forEach(b => {
        b.classList.add('message-dissolve');
        setTimeout(() => {
          if (b.parentNode) b.remove();
        }, 200);
      });
    }
    if (typeof this.onBurnSession === 'function') {
      this.onBurnSession();
    }
  }

  /**
   * Alias for burnAllMessages().
   */
  clearMessages() {
    this.burnAllMessages();
  }

  /**
   * Sets the unread count and notifies listeners.
   * @param {number} count
   */
  setUnreadCount(count) {
    this.unreadCount = Math.max(0, count || 0);
    if (typeof this.onUnreadCountChange === 'function') {
      this.onUnreadCountChange(this.unreadCount);
    }
  }

  /**
   * Increments unread count by 1 and notifies listeners.
   */
  incrementUnread() {
    this.setUnreadCount(this.unreadCount + 1);
  }

  /**
   * Clears unread count to 0 and notifies listeners.
   */
  clearUnread() {
    this.setUnreadCount(0);
  }

  /**
   * Updates peer membership and updates the padlock badge, nearby peer counter,
   * and renders participant name pills/peer dots when 2+ peers are present.
   * @param {Array<string>|Set<string>} peerIds
   * @param {number} [peerCount]
   * @param {Map|Object} [peerMeta] Map or object mapping peerId -> { name, color }
   */
  setPeers(peerIds, peerCount, peerMeta) {
    let count = 0;
    if (typeof peerCount === 'number') {
      count = peerCount;
    } else if (Array.isArray(peerIds)) {
      count = peerIds.length;
    } else if (peerIds && typeof peerIds.size === 'number') {
      count = peerIds.size;
    }

    this.currentPeerCount = count;
    this.currentPeers = Array.isArray(peerIds) ? peerIds : (peerIds ? Array.from(peerIds) : []);

    if (peerMeta) {
      if (peerMeta instanceof Map) {
        for (const [id, meta] of peerMeta.entries()) {
          this.peerMetaMap.set(id, meta);
        }
      } else if (typeof peerMeta === 'object') {
        for (const [id, meta] of Object.entries(peerMeta)) {
          this.peerMetaMap.set(id, meta);
        }
      }
    }

    if (this.peerCountEl) {
      this.peerCountEl.textContent = `${count} nearby`;
    }
    if (this.statusEl) {
      this.statusEl.setAttribute('data-peer-count', String(count));
      if (count > 0) {
        this.statusEl.classList.add('active');
      } else {
        this.statusEl.classList.remove('active');
      }
    }

    this._renderPeerStrip();
  }

  /**
   * Renders colored dots and truncated name tags for nearby participants.
   * @private
   */
  _renderPeerStrip() {
    if (!this.peerStripEl) return;

    if (typeof document === 'undefined') return;

    this.peerStripEl.innerHTML = '';

    if (this.currentPeerCount <= 0 || !this.currentPeers || this.currentPeers.length === 0) {
      return;
    }

    const maxPills = 4;
    const peersToRender = this.currentPeers.slice(0, maxPills);
    const overflowCount = this.currentPeers.length - maxPills;

    for (const peerId of peersToRender) {
      const meta = this.peerMetaMap.get(peerId) || {};
      const name = meta.name || peerId;
      const color = meta.color || this.getPeerColor(peerId);

      const pill = document.createElement('span');
      pill.className = 'peer-chip';
      pill.setAttribute('data-peer-id', peerId);

      const dot = document.createElement('span');
      dot.className = 'peer-chip-dot';
      dot.style.backgroundColor = color;

      const label = document.createElement('span');
      label.className = 'peer-chip-label';
      label.textContent = name.length > 8 ? name.slice(0, 7) + '…' : name;

      pill.appendChild(dot);
      pill.appendChild(label);
      this.peerStripEl.appendChild(pill);
    }

    if (overflowCount > 0) {
      const overflowPill = document.createElement('span');
      overflowPill.className = 'peer-chip peer-chip-overflow';
      overflowPill.textContent = `+${overflowCount}`;
      this.peerStripEl.appendChild(overflowPill);
    }
  }

  /**
   * Programmatically submits a message via onSendMessage.
   * @param {string} text
   */
  submitMessage(text) {
    const trimmed = String(text || '').trim();
    if (trimmed && typeof this.onSendMessage === 'function') {
      this.onSendMessage(trimmed);
    }
  }

  /**
   * Returns list of currently active messages.
   * @returns {Array<Object>}
   */
  getMessages() {
    return Array.from(this.messagesMap.values());
  }

  /**
   * Cleans up all timers and DOM nodes.
   */
  destroy() {
    for (const [uuid, record] of this.messagesMap.entries()) {
      if (record.agingTimer) clearTimeout(record.agingTimer);
      if (record.dissolveTimer) clearTimeout(record.dissolveTimer);
    }
    this.messagesMap.clear();
    if (this.element && this.element.parentNode) {
      this.element.parentNode.removeChild(this.element);
    }
    this.element = null;
    this.header = null;
    this.messagesContainer = null;
    this.input = null;
    this.form = null;
    this.sendBtn = null;
    this.statusEl = null;
    this.peerCountEl = null;
    this.peerStripEl = null;
    this.burnHeaderBtn = null;
  }
}

export default ZeroChatWidget;
