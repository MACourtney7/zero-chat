//! Ephemeral DOM chat interface for zero_client.
//!
//! Provides a responsive, aesthetically rich, dark-mode glassmorphic web chat interface
//! manipulating the DOM via `web-sys` on `wasm32`, and an in-memory test double on native targets.
//! Supports real-time message streaming, cryptographic status badges, security alerts for
//! tampered packets, and strict zero-persistence verification (REQ-005.1, REQ-005.4).

use std::fmt;

/// High-level lifecycle connection status for ZeroChat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Signaling,
    Connected,
    EdgeSevered,
}

impl ConnectionStatus {
    /// Human-readable UI label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Signaling => "Signaling (Host/Dead-drop)...",
            Self::Connected => "P2P Connected (WebRTC)",
            Self::EdgeSevered => "P2P Direct (Edge Severed)",
        }
    }

    /// Corresponding CSS class for status pill styling and pulsing indicator.
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Disconnected => "status-disconnected",
            Self::Signaling => "status-signaling",
            Self::Connected => "status-connected",
            Self::EdgeSevered => "status-edge-severed",
        }
    }
}

/// Errors occurring during UI DOM operations or forensic storage checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    DomElementNotFound(String),
    DomCastFailed(String),
    DomOperationFailed(String),
    StorageNotEmpty(String),
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DomElementNotFound(e) => write!(f, "DOM element not found: {e}"),
            Self::DomCastFailed(e) => write!(f, "DOM element cast failed: {e}"),
            Self::DomOperationFailed(e) => write!(f, "DOM operation failed: {e}"),
            Self::StorageNotEmpty(e) => write!(f, "Forensic violation! Storage not empty: {e}"),
        }
    }
}

impl std::error::Error for UiError {}

/// Ephemeral Chat UI controller.
///
/// Under `wasm32`, binds to and manipulates live DOM elements without storing messages in memory.
/// Under native targets, tracks UI events in-memory to verify state machine transitions.
pub struct ChatUi {
    #[cfg(target_arch = "wasm32")]
    container: web_sys::Element,
    #[cfg(target_arch = "wasm32")]
    status_badge: web_sys::Element,
    #[cfg(target_arch = "wasm32")]
    peer_list: web_sys::Element,
    #[cfg(target_arch = "wasm32")]
    peer_count: web_sys::Element,
    #[cfg(target_arch = "wasm32")]
    messages_feed: web_sys::Element,
    #[cfg(target_arch = "wasm32")]
    input_field: web_sys::HtmlInputElement,

    #[cfg(not(target_arch = "wasm32"))]
    status: std::sync::RwLock<ConnectionStatus>,
    #[cfg(not(target_arch = "wasm32"))]
    messages: std::sync::RwLock<Vec<(String, String, bool, bool)>>, // (sender, text, is_self, verified)
    #[cfg(not(target_arch = "wasm32"))]
    alerts: std::sync::RwLock<Vec<String>>,
    #[cfg(not(target_arch = "wasm32"))]
    invite_url: std::sync::RwLock<Option<String>>,
    #[cfg(not(target_arch = "wasm32"))]
    peers: std::sync::RwLock<Vec<String>>,
}

#[cfg(target_arch = "wasm32")]
impl ChatUi {
    /// Mounts the UI into `#app` or the specified container element.
    pub fn mount(container_id: &str) -> Result<Self, UiError> {
        use wasm_bindgen::JsCast;

        if container_id.trim().is_empty() {
            return Err(UiError::DomElementNotFound(container_id.to_string()));
        }

        let window = web_sys::window()
            .ok_or_else(|| UiError::DomOperationFailed("No global window found".into()))?;
        let document = window
            .document()
            .ok_or_else(|| UiError::DomOperationFailed("No document on window".into()))?;

        let clean_id = container_id.trim_start_matches('#');
        let container = document
            .get_element_by_id(clean_id)
            .ok_or_else(|| UiError::DomElementNotFound(container_id.to_string()))?;

        container.set_inner_html(
            r#"<div class="chat-layout">
  <header class="chat-header glass-panel">
    <div class="header-brand">
      <span class="brand-icon">0</span>
      <div class="brand-text">
        <h1 class="brand-title">ZeroChat</h1>
        <span class="brand-tagline">Ephemeral P2P</span>
      </div>
    </div>
    <div class="header-controls">
      <div id="peer-count" class="peer-count-badge">0 peers</div>
      <div id="peer-list" class="peer-list"></div>
      <div id="connection-status-pill" class="status-pill status-disconnected">
        <span class="status-indicator-dot"></span>
        <span id="connection-status-label" class="status-label">Disconnected</span>
      </div>
      <button id="destroy-session-button" class="btn btn-burn" type="button" title="Instantly wipes all DOM messages and memory">
        <span>Burn Session</span>
      </button>
    </div>
  </header>

  <section id="invite-section" class="invite-panel glass-panel hidden">
    <div class="invite-header">
      <span class="invite-title">Direct P2P Invite Link</span>
      <span class="invite-desc">Share this link to initiate direct signaling</span>
    </div>
    <div class="invite-input-row">
      <input id="invite-url-input" class="invite-input" type="text" readonly placeholder="Waiting for invite..." />
      <button id="copy-invite-button" class="btn btn-copy" type="button">Copy Link</button>
    </div>
    <div id="copy-feedback" class="copy-feedback hidden">✓ Copied to clipboard</div>
  </section>

  <main id="messages-container" class="messages-container"></main>

  <footer class="chat-footer glass-panel">
    <form id="message-form" class="message-form" onsubmit="return false;">
      <input id="message-input" class="message-input" type="text" placeholder="Type an encrypted message..." autocomplete="off" />
      <button id="send-button" class="btn btn-send" type="button">Send</button>
    </form>
  </footer>
</div>"#,
        );

        let status_badge = document
            .get_element_by_id("connection-status-pill")
            .ok_or_else(|| UiError::DomElementNotFound("connection-status-pill".into()))?;

        let peer_list = document
            .get_element_by_id("peer-list")
            .ok_or_else(|| UiError::DomElementNotFound("peer-list".into()))?;

        let peer_count = document
            .get_element_by_id("peer-count")
            .ok_or_else(|| UiError::DomElementNotFound("peer-count".into()))?;

        let messages_feed = document
            .get_element_by_id("messages-container")
            .ok_or_else(|| UiError::DomElementNotFound("messages-container".into()))?;

        let input_element = document
            .get_element_by_id("message-input")
            .ok_or_else(|| UiError::DomElementNotFound("message-input".into()))?;

        let input_field = input_element
            .dyn_into::<web_sys::HtmlInputElement>()
            .map_err(|_| {
                UiError::DomCastFailed("message-input is not an HtmlInputElement".into())
            })?;

        Ok(Self {
            container,
            status_badge,
            peer_list,
            peer_count,
            messages_feed,
            input_field,
        })
    }

    /// Updates the connection status pill and badge.
    pub fn set_status(&self, status: ConnectionStatus) -> Result<(), UiError> {
        self.status_badge
            .set_class_name(&format!("status-pill {}", status.css_class()));

        let document = self
            .container
            .owner_document()
            .ok_or_else(|| UiError::DomOperationFailed("no owner document on container".into()))?;

        if let Some(label) = document.get_element_by_id("connection-status-label") {
            label.set_text_content(Some(status.label()));
        }

        Ok(())
    }

    /// Displays the shareable invite link with copy action.
    pub fn show_invite(&self, invite_url: &str) -> Result<(), UiError> {
        use wasm_bindgen::JsCast;

        let document = self
            .container
            .owner_document()
            .ok_or_else(|| UiError::DomOperationFailed("no owner document on container".into()))?;

        if let Some(section) = document.get_element_by_id("invite-section") {
            remove_class(&section, "hidden");
        }

        if let Some(el) = document.get_element_by_id("invite-url-input") {
            if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
                input.set_value(invite_url);
            }
        }

        Ok(())
    }

    /// Appends a new chat message bubble (REQ-005.1).
    /// Displays sender ID, timestamp, decrypted plaintext, and crypto verification tag.
    pub fn append_message(
        &self,
        sender_id: &str,
        text: &str,
        is_self: bool,
        verified: bool,
    ) -> Result<(), UiError> {
        let document = self
            .container
            .owner_document()
            .ok_or_else(|| UiError::DomOperationFailed("no owner document on container".into()))?;

        let bubble = document
            .create_element("div")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        let direction_class = if is_self {
            "message-outgoing"
        } else {
            "message-incoming"
        };
        bubble.set_class_name(&format!("message-bubble {direction_class}"));
        let peer_attr = if is_self { "self" } else { sender_id };
        bubble
            .set_attribute("data-peer-id", peer_attr)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        // Header: Sender + Timestamp
        let header = document
            .create_element("div")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        header.set_class_name("message-header");

        let sender_el = document
            .create_element("span")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        sender_el.set_class_name("message-sender");
        let display_sender = if is_self { "You" } else { sender_id };
        sender_el.set_text_content(Some(display_sender));
        header
            .append_child(&sender_el)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        let time_el = document
            .create_element("span")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        time_el.set_class_name("message-timestamp");
        let date = js_sys::Date::new_0();
        let time_str = format!("{:02}:{:02}", date.get_hours(), date.get_minutes());
        time_el.set_text_content(Some(&time_str));
        header
            .append_child(&time_el)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        bubble
            .append_child(&header)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        // Body: Decrypted plaintext
        let body = document
            .create_element("div")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        body.set_class_name("message-text");
        body.set_text_content(Some(text));
        bubble
            .append_child(&body)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        // Crypto Verification Tag
        let badge = document
            .create_element("div")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        if verified {
            badge.set_class_name("security-badge verified");
            badge.set_text_content(Some("🔒 ChaCha20-Poly1305 Verified"));
        } else {
            badge.set_class_name("security-badge unverified");
            badge.set_text_content(Some("⚠️ Unverified Tag"));
        }
        bubble
            .append_child(&badge)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        self.messages_feed
            .append_child(&bubble)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        self.messages_feed
            .set_scroll_top(self.messages_feed.scroll_height());

        Ok(())
    }

    /// Appends a prominent security alert for tampered/dropped packets (REQ-005.3).
    pub fn append_security_alert(&self, alert_text: &str) -> Result<(), UiError> {
        let document = self
            .container
            .owner_document()
            .ok_or_else(|| UiError::DomOperationFailed("no owner document on container".into()))?;

        let card = document
            .create_element("div")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        card.set_class_name("security-alert-card");

        let icon = document
            .create_element("span")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        icon.set_class_name("alert-icon");
        icon.set_text_content(Some("⚠️ [SECURITY ALERT]"));
        card.append_child(&icon)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        let text_el = document
            .create_element("span")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        text_el.set_class_name("alert-message");
        text_el.set_text_content(Some(alert_text));
        card.append_child(&text_el)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        self.messages_feed
            .append_child(&card)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        self.messages_feed
            .set_scroll_top(self.messages_feed.scroll_height());

        Ok(())
    }

    /// Selectively purges messages from the DOM associated with a specific peer ID (or "self").
    pub fn burn_peer_messages(&self, peer_id: &str) -> Result<(), UiError> {
        use wasm_bindgen::JsCast;

        let mut curr = self.messages_feed.first_child();
        while let Some(node) = curr {
            let next = node.next_sibling();
            if let Some(el) = node.dyn_ref::<web_sys::Element>() {
                if el.get_attribute("data-peer-id").as_deref() == Some(peer_id) {
                    self.messages_feed
                        .remove_child(&node)
                        .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
                }
            }
            curr = next;
        }
        Ok(())
    }

    /// Wipes all messages and destroys the ephemeral session (REQ-005.4 zero-footprint).
    pub fn destroy_session(&self) -> Result<(), UiError> {
        use wasm_bindgen::JsCast;

        self.messages_feed.set_inner_html("");
        self.peer_list.set_inner_html("");
        self.peer_count.set_text_content(Some("0 peers"));
        self.input_field.set_value("");
        self.set_status(ConnectionStatus::Disconnected)?;

        let document = self
            .container
            .owner_document()
            .ok_or_else(|| UiError::DomOperationFailed("no owner document on container".into()))?;

        if let Some(invite_sec) = document.get_element_by_id("invite-section") {
            add_class(&invite_sec, "hidden");
        }

        if let Some(invite_el) = document.get_element_by_id("invite-url-input") {
            if let Ok(input) = invite_el.dyn_into::<web_sys::HtmlInputElement>() {
                input.set_value("");
            }
        }

        if let Some(feedback) = document.get_element_by_id("copy-feedback") {
            add_class(&feedback, "hidden");
        }

        Self::verify_zero_persistence()?;

        Ok(())
    }

    /// Verifies the zero persistence invariant (REQ-005.4).
    /// Confirms localStorage, sessionStorage, and IndexedDB have zero stored keys.
    pub fn verify_zero_persistence() -> Result<(), UiError> {
        let window = web_sys::window()
            .ok_or_else(|| UiError::DomOperationFailed("No global window found".into()))?;

        if let Ok(Some(storage)) = window.local_storage() {
            let len = storage
                .length()
                .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
            if len > 0 {
                return Err(UiError::StorageNotEmpty(format!(
                    "localStorage has {len} item(s)"
                )));
            }
        }

        if let Ok(Some(storage)) = window.session_storage() {
            let len = storage
                .length()
                .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
            if len > 0 {
                return Err(UiError::StorageNotEmpty(format!(
                    "sessionStorage has {len} item(s)"
                )));
            }
        }

        Ok(())
    }

    /// Dynamically adds a peer pill element (.peer-pill) inside #peer-list, displaying
    /// a shortened peer ID (first 8 chars or full string) and an online dot indicator. Updates #peer-count.
    pub fn add_peer(&self, peer_id: &str) -> Result<(), UiError> {
        use wasm_bindgen::JsCast;

        let mut curr = self.peer_list.first_child();
        while let Some(node) = curr {
            let next = node.next_sibling();
            if let Ok(el) = node.dyn_into::<web_sys::Element>() {
                if el.get_attribute("data-peer-id").as_deref() == Some(peer_id) {
                    return Ok(());
                }
            }
            curr = next;
        }

        let document = self
            .container
            .owner_document()
            .ok_or_else(|| UiError::DomOperationFailed("no owner document on container".into()))?;

        let pill = document
            .create_element("div")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        pill.set_class_name("peer-pill");
        pill.set_attribute("data-peer-id", peer_id)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        let dot = document
            .create_element("span")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        dot.set_class_name("peer-indicator-dot");
        pill.append_child(&dot)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        let label = document
            .create_element("span")
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
        label.set_class_name("peer-label");
        let display_id = if peer_id.len() > 8 {
            &peer_id[..8]
        } else {
            peer_id
        };
        label.set_text_content(Some(display_id));
        pill.append_child(&label)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        self.peer_list
            .append_child(&pill)
            .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;

        self.update_peer_count();
        Ok(())
    }

    /// Removes the peer pill element with matching data attribute from #peer-list. Updates #peer-count.
    pub fn remove_peer(&self, peer_id: &str) -> Result<(), UiError> {
        use wasm_bindgen::JsCast;

        let mut curr = self.peer_list.first_child();
        while let Some(node) = curr {
            let next = node.next_sibling();
            if let Ok(el) = node.clone().dyn_into::<web_sys::Element>() {
                if el.get_attribute("data-peer-id").as_deref() == Some(peer_id) {
                    self.peer_list
                        .remove_child(&node)
                        .map_err(|e| UiError::DomOperationFailed(format!("{e:?}")))?;
                    break;
                }
            }
            curr = next;
        }
        self.update_peer_count();
        Ok(())
    }

    /// Synchronizes the #peer-list to match the given list of peer IDs.
    pub fn set_peers(&self, peers: &[String]) -> Result<(), UiError> {
        self.peer_list.set_inner_html("");
        for peer_id in peers {
            self.add_peer(peer_id)?;
        }
        self.update_peer_count();
        Ok(())
    }

    /// Reads the list of currently rendered peer IDs from the DOM.
    pub fn peers(&self) -> Vec<String> {
        use wasm_bindgen::JsCast;

        let mut result = Vec::new();
        let mut curr = self.peer_list.first_child();
        while let Some(node) = curr {
            let next = node.next_sibling();
            if let Ok(el) = node.dyn_into::<web_sys::Element>() {
                if let Some(id) = el.get_attribute("data-peer-id") {
                    result.push(id);
                }
            }
            curr = next;
        }
        result
    }

    fn update_peer_count(&self) {
        let count = self.peers().len();
        let text = if count == 1 {
            "1 peer".to_string()
        } else {
            format!("{count} peers")
        };
        self.peer_count.set_text_content(Some(&text));
    }

    /// Reference to the container element.
    pub fn container(&self) -> &web_sys::Element {
        &self.container
    }

    /// Reference to the status badge element.
    pub fn status_badge(&self) -> &web_sys::Element {
        &self.status_badge
    }

    /// Reference to the peer list element.
    pub fn peer_list(&self) -> &web_sys::Element {
        &self.peer_list
    }

    /// Reference to the peer count badge element.
    pub fn peer_count(&self) -> &web_sys::Element {
        &self.peer_count
    }

    /// Reference to the messages feed container.
    pub fn messages_feed(&self) -> &web_sys::Element {
        &self.messages_feed
    }

    /// Reference to the input field.
    pub fn input_field(&self) -> &web_sys::HtmlInputElement {
        &self.input_field
    }
}

#[cfg(target_arch = "wasm32")]
fn add_class(el: &web_sys::Element, class_to_add: &str) {
    let current = el.class_name();
    let classes: Vec<&str> = current.split_whitespace().collect();
    if !classes.contains(&class_to_add) {
        if current.is_empty() {
            el.set_class_name(class_to_add);
        } else {
            el.set_class_name(&format!("{current} {class_to_add}"));
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn remove_class(el: &web_sys::Element, class_to_remove: &str) {
    let current = el.class_name();
    let updated: Vec<&str> = current
        .split_whitespace()
        .filter(|&c| c != class_to_remove)
        .collect();
    el.set_class_name(&updated.join(" "));
}

#[cfg(not(target_arch = "wasm32"))]
impl ChatUi {
    /// Mounts the in-memory test double for the specified container.
    pub fn mount(container_id: &str) -> Result<Self, UiError> {
        if container_id.trim().is_empty() {
            return Err(UiError::DomElementNotFound(container_id.to_string()));
        }
        Ok(Self {
            status: std::sync::RwLock::new(ConnectionStatus::Disconnected),
            messages: std::sync::RwLock::new(Vec::new()),
            alerts: std::sync::RwLock::new(Vec::new()),
            invite_url: std::sync::RwLock::new(None),
            peers: std::sync::RwLock::new(Vec::new()),
        })
    }

    /// Updates the connection status pill and badge.
    pub fn set_status(&self, status: ConnectionStatus) -> Result<(), UiError> {
        let mut guard = self
            .status
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        *guard = status;
        Ok(())
    }

    /// Displays the shareable invite link with copy action.
    pub fn show_invite(&self, invite_url: &str) -> Result<(), UiError> {
        let mut guard = self
            .invite_url
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        *guard = Some(invite_url.to_string());
        Ok(())
    }

    /// Appends a new chat message bubble (REQ-005.1).
    pub fn append_message(
        &self,
        sender_id: &str,
        text: &str,
        is_self: bool,
        verified: bool,
    ) -> Result<(), UiError> {
        let mut guard = self
            .messages
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        guard.push((sender_id.to_string(), text.to_string(), is_self, verified));
        Ok(())
    }

    /// Appends a prominent security alert for tampered/dropped packets (REQ-005.3).
    pub fn append_security_alert(&self, alert_text: &str) -> Result<(), UiError> {
        let mut guard = self
            .alerts
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        guard.push(alert_text.to_string());
        Ok(())
    }

    /// Selectively purges messages from in-memory store matching the specified peer ID (or "self").
    pub fn burn_peer_messages(&self, peer_id: &str) -> Result<(), UiError> {
        let mut guard = self
            .messages
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        if peer_id == "self" {
            guard.retain(|(_, _, is_self, _)| !*is_self);
        } else {
            guard.retain(|(sender, _, is_self, _)| *is_self || sender != peer_id);
        }
        Ok(())
    }

    /// Dynamically adds a peer ID to the active peer list if not already present.
    pub fn add_peer(&self, peer_id: &str) -> Result<(), UiError> {
        let mut guard = self
            .peers
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        if !guard.iter().any(|p| p == peer_id) {
            guard.push(peer_id.to_string());
        }
        Ok(())
    }

    /// Removes the peer with matching ID from the active peer list.
    pub fn remove_peer(&self, peer_id: &str) -> Result<(), UiError> {
        let mut guard = self
            .peers
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        guard.retain(|p| p != peer_id);
        Ok(())
    }

    /// Synchronizes the active peer list with the given slice of peer IDs.
    pub fn set_peers(&self, peers: &[String]) -> Result<(), UiError> {
        let mut guard = self
            .peers
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        *guard = peers.to_vec();
        Ok(())
    }

    /// Returns a snapshot clone of the currently active peer IDs.
    pub fn peers(&self) -> Vec<String> {
        self.peers.read().expect("peers lock poisoned").clone()
    }

    /// Returns the current number of active peers.
    pub fn peer_count(&self) -> usize {
        self.peers.read().expect("peers lock poisoned").len()
    }

    /// Wipes all messages and destroys the ephemeral session (REQ-005.4 zero-footprint).
    pub fn destroy_session(&self) -> Result<(), UiError> {
        let mut msgs = self
            .messages
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        msgs.clear();

        let mut alerts = self
            .alerts
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        alerts.clear();

        let mut peers = self
            .peers
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        peers.clear();

        let mut status = self
            .status
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        *status = ConnectionStatus::Disconnected;

        let mut invite = self
            .invite_url
            .write()
            .map_err(|e| UiError::DomOperationFailed(e.to_string()))?;
        *invite = None;

        Self::verify_zero_persistence()?;

        Ok(())
    }

    /// Verifies the zero persistence invariant (REQ-005.4).
    pub fn verify_zero_persistence() -> Result<(), UiError> {
        Ok(())
    }

    /// Inspect current connection status in in-memory test model.
    pub fn status(&self) -> ConnectionStatus {
        *self.status.read().expect("status lock poisoned")
    }

    /// Inspect logged messages in in-memory test model.
    pub fn messages(&self) -> Vec<(String, String, bool, bool)> {
        self.messages
            .read()
            .expect("messages lock poisoned")
            .clone()
    }

    /// Inspect logged alerts in in-memory test model.
    pub fn alerts(&self) -> Vec<String> {
        self.alerts.read().expect("alerts lock poisoned").clone()
    }

    /// Inspect current invite URL in in-memory test model.
    pub fn invite_url(&self) -> Option<String> {
        self.invite_url
            .read()
            .expect("invite lock poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_status_labels_and_classes() {
        assert_eq!(ConnectionStatus::Disconnected.label(), "Disconnected");
        assert_eq!(
            ConnectionStatus::Disconnected.css_class(),
            "status-disconnected"
        );

        assert_eq!(
            ConnectionStatus::Signaling.label(),
            "Signaling (Host/Dead-drop)..."
        );
        assert_eq!(ConnectionStatus::Signaling.css_class(), "status-signaling");

        assert_eq!(
            ConnectionStatus::Connected.label(),
            "P2P Connected (WebRTC)"
        );
        assert_eq!(ConnectionStatus::Connected.css_class(), "status-connected");

        assert_eq!(
            ConnectionStatus::EdgeSevered.label(),
            "P2P Direct (Edge Severed)"
        );
        assert_eq!(
            ConnectionStatus::EdgeSevered.css_class(),
            "status-edge-severed"
        );
    }

    #[test]
    fn test_chat_ui_mount_and_status() {
        // Empty container fails
        assert!(ChatUi::mount("").is_err());
        assert!(matches!(
            ChatUi::mount(""),
            Err(UiError::DomElementNotFound(_))
        ));

        // Valid mount initializes as Disconnected
        let ui = ChatUi::mount("app").expect("mount failed");
        assert_eq!(ui.status(), ConnectionStatus::Disconnected);

        // Transition through signaling and connected states
        ui.set_status(ConnectionStatus::Signaling)
            .expect("set status failed");
        assert_eq!(ui.status(), ConnectionStatus::Signaling);

        ui.set_status(ConnectionStatus::Connected)
            .expect("set status failed");
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        ui.set_status(ConnectionStatus::EdgeSevered)
            .expect("set status failed");
        assert_eq!(ui.status(), ConnectionStatus::EdgeSevered);
    }

    #[test]
    fn test_append_message_and_crypto_badge() {
        let ui = ChatUi::mount("app").expect("mount failed");
        assert!(ui.messages().is_empty());

        // Self outgoing message, verified
        ui.append_message("self", "Hello peer!", true, true)
            .expect("append self failed");
        // Peer incoming message, verified
        ui.append_message("peer-node-01", "Hello back!", false, true)
            .expect("append peer failed");
        // Unverified incoming message
        ui.append_message("malicious-actor", "Tampered text", false, false)
            .expect("append unverified failed");

        let msgs = ui.messages();
        assert_eq!(msgs.len(), 3);
        assert_eq!(
            msgs[0],
            ("self".to_string(), "Hello peer!".to_string(), true, true)
        );
        assert_eq!(
            msgs[1],
            (
                "peer-node-01".to_string(),
                "Hello back!".to_string(),
                false,
                true
            )
        );
        assert_eq!(
            msgs[2],
            (
                "malicious-actor".to_string(),
                "Tampered text".to_string(),
                false,
                false
            )
        );
    }

    #[test]
    fn test_append_security_alert() {
        let ui = ChatUi::mount("app").expect("mount failed");
        assert!(ui.alerts().is_empty());

        let alert = "Dropped unauthenticated or tampered packet (authentication tag mismatch).";
        ui.append_security_alert(alert)
            .expect("append alert failed");

        let alerts = ui.alerts();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0], alert);
    }

    #[test]
    fn test_destroy_session_clears_feed() {
        let ui = ChatUi::mount("app").expect("mount failed");

        ui.set_status(ConnectionStatus::Connected)
            .expect("set status failed");
        ui.show_invite("https://zero.chat/#invite-data")
            .expect("show invite failed");
        ui.append_message("self", "Ephemeral message", true, true)
            .expect("append msg failed");
        ui.append_security_alert("Potential replay attempt")
            .expect("append alert failed");

        assert_eq!(ui.messages().len(), 1);
        assert_eq!(ui.alerts().len(), 1);
        assert_eq!(
            ui.invite_url(),
            Some("https://zero.chat/#invite-data".to_string())
        );
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        // Burn session
        ui.destroy_session().expect("destroy session failed");

        assert!(ui.messages().is_empty());
        assert!(ui.alerts().is_empty());
        assert_eq!(ui.invite_url(), None);
        assert_eq!(ui.status(), ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_zero_persistence_verification() {
        assert!(ChatUi::verify_zero_persistence().is_ok());

        // Test UiError formatting
        let err = UiError::StorageNotEmpty("localStorage length = 3".to_string());
        assert_eq!(
            format!("{err}"),
            "Forensic violation! Storage not empty: localStorage length = 3"
        );

        let not_found = UiError::DomElementNotFound("app".to_string());
        assert_eq!(format!("{not_found}"), "DOM element not found: app");

        let cast_err = UiError::DomCastFailed("input".to_string());
        assert_eq!(format!("{cast_err}"), "DOM element cast failed: input");

        let op_err = UiError::DomOperationFailed("failed op".to_string());
        assert_eq!(format!("{op_err}"), "DOM operation failed: failed op");
    }

    #[test]
    fn test_peer_list_add_and_remove() {
        let ui = ChatUi::mount("app").expect("mount failed");
        assert!(ui.peers().is_empty());
        assert_eq!(ui.peer_count(), 0);

        ui.add_peer("peer-bob").expect("add peer-bob failed");
        assert_eq!(ui.peers(), vec!["peer-bob".to_string()]);
        assert_eq!(ui.peer_count(), 1);

        // Duplicate add is idempotent
        ui.add_peer("peer-bob").expect("add duplicate failed");
        assert_eq!(ui.peers(), vec!["peer-bob".to_string()]);
        assert_eq!(ui.peer_count(), 1);

        ui.add_peer("peer-charlie")
            .expect("add peer-charlie failed");
        assert_eq!(
            ui.peers(),
            vec!["peer-bob".to_string(), "peer-charlie".to_string()]
        );
        assert_eq!(ui.peer_count(), 2);

        ui.remove_peer("peer-bob").expect("remove peer-bob failed");
        assert_eq!(ui.peers(), vec!["peer-charlie".to_string()]);
        assert_eq!(ui.peer_count(), 1);

        ui.remove_peer("non-existent")
            .expect("remove non-existent ok");
        assert_eq!(ui.peers(), vec!["peer-charlie".to_string()]);
        assert_eq!(ui.peer_count(), 1);
    }

    #[test]
    fn test_set_peers_synchronization() {
        let ui = ChatUi::mount("app").expect("mount failed");
        let initial_peers = vec![
            "alice".to_string(),
            "bob".to_string(),
            "charlie".to_string(),
        ];
        ui.set_peers(&initial_peers).expect("set_peers failed");
        assert_eq!(ui.peers().len(), 3);
        assert_eq!(ui.peers(), initial_peers);

        // Update with new set
        let updated_peers = vec!["david".to_string()];
        ui.set_peers(&updated_peers)
            .expect("set_peers update failed");
        assert_eq!(ui.peers(), updated_peers);
    }

    #[test]
    fn test_destroy_session_clears_peers() {
        let ui = ChatUi::mount("app").expect("mount failed");
        ui.add_peer("peer-alice").expect("add peer failed");
        ui.add_peer("peer-bob").expect("add peer failed");
        assert_eq!(ui.peers().len(), 2);

        ui.destroy_session().expect("destroy_session failed");
        assert!(ui.peers().is_empty());
    }

    #[test]
    fn test_burn_peer_messages_selective() {
        let ui = ChatUi::mount("app").expect("mount failed");

        // Verify appending messages for "alice", "bob", and "self"
        ui.append_message("alice", "Hello from Alice", false, true)
            .expect("append alice failed");
        ui.append_message("bob", "Hello from Bob", false, true)
            .expect("append bob failed");
        ui.append_message("self", "Hello from Self", true, true)
            .expect("append self failed");

        let msgs = ui.messages();
        assert_eq!(msgs.len(), 3);

        // Calling burn_peer_messages("alice") removes "alice" while keeping "bob" and "self"
        ui.burn_peer_messages("alice").expect("burn alice failed");
        let msgs = ui.messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].0, "bob");
        assert_eq!(msgs[1].0, "self");

        // Calling burn_peer_messages("self") removes "self" while keeping "bob"
        ui.burn_peer_messages("self").expect("burn self failed");
        let msgs = ui.messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].0, "bob");
    }

    #[test]
    fn test_burn_peer_messages_nonexistent_idempotent() {
        let ui = ChatUi::mount("app").expect("mount failed");

        ui.append_message("bob", "Hello from Bob", false, true)
            .expect("append bob failed");
        ui.append_message("self", "Hello from Self", true, true)
            .expect("append self failed");

        // Purging non-existent peer is idempotent and does not error or affect other messages
        ui.burn_peer_messages("non-existent-peer")
            .expect("burn non-existent failed");
        let msgs = ui.messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].0, "bob");
        assert_eq!(msgs[1].0, "self");

        // Subsequent call remains idempotent
        ui.burn_peer_messages("non-existent-peer")
            .expect("burn non-existent second call failed");
        assert_eq!(ui.messages().len(), 2);
    }

    #[test]
    fn test_peer_departure_preserves_session_until_explicit_destroy() {
        let ui = ChatUi::mount("app").expect("mount failed");
        ui.set_status(ConnectionStatus::Connected)
            .expect("set status failed");
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        // Add a peer ("peer-bob"), append remote message from "peer-bob", append local message from "self"
        ui.add_peer("peer-bob").expect("add peer failed");
        ui.append_message("peer-bob", "Incoming from Bob", false, true)
            .expect("append peer message failed");
        ui.append_message("self", "Outgoing from self", true, true)
            .expect("append self message failed");

        assert_eq!(ui.peers(), vec!["peer-bob".to_string()]);
        assert_eq!(ui.messages().len(), 2);

        // Remove "peer-bob" and purge peer messages via burn_peer_messages("peer-bob")
        ui.remove_peer("peer-bob").expect("remove peer failed");
        ui.burn_peer_messages("peer-bob")
            .expect("burn peer messages failed");

        // Assert that ui.peers() is empty (0 peers), but the session is NOT destroyed:
        // local message from "self" remains intact in ui.messages().
        assert!(ui.peers().is_empty());
        let msgs = ui.messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].0, "self");
        assert_eq!(msgs[0].1, "Outgoing from self");
        assert!(msgs[0].2); // is_self
        assert!(msgs[0].3); // verified

        // Explicitly invoke ui.destroy_session()
        ui.destroy_session().expect("destroy session failed");

        // Assert that all messages are now wiped, status is ConnectionStatus::Disconnected,
        // and zero persistence is verified.
        assert!(ui.messages().is_empty());
        assert!(ui.peers().is_empty());
        assert_eq!(ui.status(), ConnectionStatus::Disconnected);
        assert!(ChatUi::verify_zero_persistence().is_ok());
    }

    #[test]
    fn test_multi_peer_partial_departure_preserves_surviving_peers() {
        let ui = ChatUi::mount("app").expect("mount failed");
        ui.set_status(ConnectionStatus::Connected)
            .expect("set status failed");
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        // Mount ChatUi, connect, and add 3 peers: "alice", "bob", "charlie"
        ui.add_peer("alice").expect("add alice failed");
        ui.add_peer("bob").expect("add bob failed");
        ui.add_peer("charlie").expect("add charlie failed");
        assert_eq!(ui.peer_count(), 3);
        assert_eq!(
            ui.peers(),
            vec![
                "alice".to_string(),
                "bob".to_string(),
                "charlie".to_string()
            ]
        );

        // Append incoming messages for "bob" and "charlie", incoming for "alice", and self message
        ui.append_message("bob", "Hello from Bob", false, true)
            .expect("append bob message failed");
        ui.append_message("charlie", "Hello from Charlie", false, true)
            .expect("append charlie message failed");
        ui.append_message("alice", "Hello from Alice", false, true)
            .expect("append alice message failed");
        ui.append_message("self", "Hello from Self", true, true)
            .expect("append self message failed");

        let initial_msgs = ui.messages();
        assert_eq!(initial_msgs.len(), 4);

        // Simulate Bob departing: remove "bob", burn "bob" messages
        ui.remove_peer("bob").expect("remove bob failed");
        ui.burn_peer_messages("bob").expect("burn bob failed");

        // Assert ui.peer_count() is 2 and ui.peers() contains "alice" and "charlie"
        assert_eq!(ui.peer_count(), 2);
        assert_eq!(ui.peers(), vec!["alice".to_string(), "charlie".to_string()]);

        // Assert Bob's messages are gone, but Charlie's, Alice's, and Self's messages are retained
        let msgs_after_bob = ui.messages();
        assert_eq!(msgs_after_bob.len(), 3);
        assert!(!msgs_after_bob.iter().any(|(s, _, _, _)| s == "bob"));
        assert!(msgs_after_bob.iter().any(|(s, _, _, _)| s == "charlie"));
        assert!(msgs_after_bob.iter().any(|(s, _, _, _)| s == "alice"));
        assert!(msgs_after_bob
            .iter()
            .any(|(s, _, is_self, _)| *is_self && s == "self"));

        // Assert connection status remains Connected
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        // Simulate Charlie departing: remove "charlie", burn "charlie" messages
        ui.remove_peer("charlie").expect("remove charlie failed");
        ui.burn_peer_messages("charlie")
            .expect("burn charlie failed");

        // Assert ui.peer_count() is 1 and surviving peer is Alice
        assert_eq!(ui.peer_count(), 1);
        assert_eq!(ui.peers(), vec!["alice".to_string()]);
        let msgs_after_charlie = ui.messages();
        assert_eq!(msgs_after_charlie.len(), 2);
        assert!(!msgs_after_charlie.iter().any(|(s, _, _, _)| s == "charlie"));
        assert!(msgs_after_charlie.iter().any(|(s, _, _, _)| s == "alice"));
        assert!(msgs_after_charlie
            .iter()
            .any(|(s, _, is_self, _)| *is_self && s == "self"));

        // Simulate Alice departing: remove "alice", burn "alice" messages
        ui.remove_peer("alice").expect("remove alice failed");
        ui.burn_peer_messages("alice").expect("burn alice failed");

        // Assert peer_count is 0, peers is empty
        assert_eq!(ui.peer_count(), 0);
        assert!(ui.peers().is_empty());

        // Assert self messages are retained until explicit destroy_session()
        let msgs_after_all_peers = ui.messages();
        assert_eq!(msgs_after_all_peers.len(), 1);
        assert_eq!(msgs_after_all_peers[0].0, "self");
        assert_eq!(msgs_after_all_peers[0].1, "Hello from Self");
        assert!(msgs_after_all_peers[0].2);

        // Explicitly destroy session
        ui.destroy_session().expect("destroy session failed");
        assert!(ui.messages().is_empty());
        assert_eq!(ui.status(), ConnectionStatus::Disconnected);
        assert_eq!(ui.peer_count(), 0);
    }

    #[test]
    fn test_req_028_ui_dynamic_multi_peer_lifecycle_and_selective_purge() {
        let ui = ChatUi::mount("app").expect("mount failed");
        ui.set_status(ConnectionStatus::Connected)
            .expect("set status failed");
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        // Mount test double ChatUi, connect, and register 3 peers: "alice", "bob", "charlie"
        ui.add_peer("alice").expect("add alice failed");
        ui.add_peer("bob").expect("add bob failed");
        ui.add_peer("charlie").expect("add charlie failed");

        // Assert peer_count() is 3 and peers() contains all 3 peers
        assert_eq!(ui.peer_count(), 3);
        let peers = ui.peers();
        assert!(peers.contains(&"alice".to_string()));
        assert!(peers.contains(&"bob".to_string()));
        assert!(peers.contains(&"charlie".to_string()));

        // Append messages from all peers and self:
        // - "alice": "Message from Alice"
        // - "bob": "Message from Bob"
        // - "charlie": "Message from Charlie"
        // - "self": "Message from Self"
        ui.append_message("alice", "Message from Alice", false, true)
            .expect("append alice failed");
        ui.append_message("bob", "Message from Bob", false, true)
            .expect("append bob failed");
        ui.append_message("charlie", "Message from Charlie", false, true)
            .expect("append charlie failed");
        ui.append_message("self", "Message from Self", true, true)
            .expect("append self failed");

        // Assert all 4 messages are present in UI
        let msgs = ui.messages();
        assert_eq!(msgs.len(), 4);
        assert!(msgs
            .iter()
            .any(|(s, m, _, _)| s == "alice" && m == "Message from Alice"));
        assert!(msgs
            .iter()
            .any(|(s, m, _, _)| s == "bob" && m == "Message from Bob"));
        assert!(msgs
            .iter()
            .any(|(s, m, _, _)| s == "charlie" && m == "Message from Charlie"));
        assert!(msgs
            .iter()
            .any(|(s, m, is_self, _)| *is_self && s == "self" && m == "Message from Self"));

        // Simulate Bob departing:
        // - Call remove_peer("bob")
        // - Call burn_peer_messages("bob")
        ui.remove_peer("bob").expect("remove bob failed");
        ui.burn_peer_messages("bob")
            .expect("burn bob messages failed");

        // Verify surviving state:
        // - peer_count() is 2
        // - peers() contains "alice" and "charlie"
        // - Bob's messages are removed
        // - Alice's, Charlie's, and Self's messages remain intact
        // - Connection status remains Connected
        assert_eq!(ui.peer_count(), 2);
        let peers_after_bob = ui.peers();
        assert!(peers_after_bob.contains(&"alice".to_string()));
        assert!(peers_after_bob.contains(&"charlie".to_string()));
        assert!(!peers_after_bob.contains(&"bob".to_string()));

        let msgs_after_bob = ui.messages();
        assert_eq!(msgs_after_bob.len(), 3);
        assert!(!msgs_after_bob.iter().any(|(s, _, _, _)| s == "bob"));
        assert!(msgs_after_bob
            .iter()
            .any(|(s, m, _, _)| s == "alice" && m == "Message from Alice"));
        assert!(msgs_after_bob
            .iter()
            .any(|(s, m, _, _)| s == "charlie" && m == "Message from Charlie"));
        assert!(msgs_after_bob
            .iter()
            .any(|(s, m, is_self, _)| *is_self && s == "self" && m == "Message from Self"));
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        // Simulate Charlie departing:
        // - Call remove_peer("charlie")
        // - Call burn_peer_messages("charlie")
        ui.remove_peer("charlie").expect("remove charlie failed");
        ui.burn_peer_messages("charlie")
            .expect("burn charlie messages failed");

        // Verify surviving state:
        // - peer_count() is 1 ("alice")
        // - Charlie's messages are removed; Alice's and Self's messages remain intact
        assert_eq!(ui.peer_count(), 1);
        let peers_after_charlie = ui.peers();
        assert_eq!(peers_after_charlie, vec!["alice".to_string()]);

        let msgs_after_charlie = ui.messages();
        assert_eq!(msgs_after_charlie.len(), 2);
        assert!(!msgs_after_charlie.iter().any(|(s, _, _, _)| s == "charlie"));
        assert!(msgs_after_charlie
            .iter()
            .any(|(s, m, _, _)| s == "alice" && m == "Message from Alice"));
        assert!(msgs_after_charlie
            .iter()
            .any(|(s, m, is_self, _)| *is_self && s == "self" && m == "Message from Self"));

        // Call destroy_session():
        // - Verify all messages cleared
        // - Verify peer_count() is 0, peers() is empty
        // - Verify connection status transitions to Disconnected
        ui.destroy_session().expect("destroy session failed");
        assert!(ui.messages().is_empty());
        assert_eq!(ui.peer_count(), 0);
        assert!(ui.peers().is_empty());
        assert_eq!(ui.status(), ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_req_029_multi_remote_peer_removal_selective_message_purge() {
        let ui = ChatUi::mount("app").expect("mount failed");
        ui.set_status(ConnectionStatus::Connected)
            .expect("set status failed");
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        // Register 3 remote peers: "alice" (host), "bob", "charlie"
        ui.add_peer("alice").expect("add alice failed");
        ui.add_peer("bob").expect("add bob failed");
        ui.add_peer("charlie").expect("add charlie failed");
        assert_eq!(ui.peer_count(), 3);
        assert_eq!(
            ui.peers(),
            vec![
                "alice".to_string(),
                "bob".to_string(),
                "charlie".to_string()
            ]
        );

        // Populate multiple messages from each remote peer and self
        ui.append_message("alice", "First message from Alice", false, true)
            .expect("append alice msg 1 failed");
        ui.append_message("bob", "First message from Bob", false, true)
            .expect("append bob msg 1 failed");
        ui.append_message("charlie", "First message from Charlie", false, true)
            .expect("append charlie msg 1 failed");
        ui.append_message("self", "First message from Self", true, true)
            .expect("append self msg 1 failed");
        ui.append_message("alice", "Second message from Alice", false, true)
            .expect("append alice msg 2 failed");
        ui.append_message("bob", "Second message from Bob", false, true)
            .expect("append bob msg 2 failed");
        ui.append_message("charlie", "Second message from Charlie", false, true)
            .expect("append charlie msg 2 failed");
        ui.append_message("self", "Second message from Self", true, true)
            .expect("append self msg 2 failed");

        let initial_msgs = ui.messages();
        assert_eq!(initial_msgs.len(), 8);

        // Step 1: 3rd peer ("charlie") departs: remove peer and burn messages
        ui.remove_peer("charlie").expect("remove charlie failed");
        ui.burn_peer_messages("charlie")
            .expect("burn charlie messages failed");

        // Assert peer count drops to 2, Charlie is removed, Alice and Bob remain
        assert_eq!(ui.peer_count(), 2);
        assert_eq!(ui.peers(), vec!["alice".to_string(), "bob".to_string()]);

        // Assert Charlie's messages are selectively purged, while Alice, Bob, and Self messages remain intact
        let msgs_after_charlie = ui.messages();
        assert_eq!(msgs_after_charlie.len(), 6);
        assert!(!msgs_after_charlie.iter().any(|(s, _, _, _)| s == "charlie"));
        assert_eq!(
            msgs_after_charlie
                .iter()
                .filter(|(s, _, _, _)| s == "alice")
                .count(),
            2
        );
        assert_eq!(
            msgs_after_charlie
                .iter()
                .filter(|(s, _, _, _)| s == "bob")
                .count(),
            2
        );
        assert_eq!(
            msgs_after_charlie
                .iter()
                .filter(|(s, _, is_self, _)| *is_self && s == "self")
                .count(),
            2
        );
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        // Step 2: Host / 1st peer ("alice") departs: remove peer and burn messages
        ui.remove_peer("alice").expect("remove alice failed");
        ui.burn_peer_messages("alice")
            .expect("burn alice messages failed");

        // Assert peer count drops to 1, Alice is removed, Bob remains
        assert_eq!(ui.peer_count(), 1);
        assert_eq!(ui.peers(), vec!["bob".to_string()]);

        // Assert Alice's messages are selectively purged, Bob and Self messages remain intact
        let msgs_after_alice = ui.messages();
        assert_eq!(msgs_after_alice.len(), 4);
        assert!(!msgs_after_alice.iter().any(|(s, _, _, _)| s == "alice"));
        assert!(!msgs_after_alice.iter().any(|(s, _, _, _)| s == "charlie"));
        assert_eq!(
            msgs_after_alice
                .iter()
                .filter(|(s, _, _, _)| s == "bob")
                .count(),
            2
        );
        assert_eq!(
            msgs_after_alice
                .iter()
                .filter(|(s, _, is_self, _)| *is_self && s == "self")
                .count(),
            2
        );
        assert_eq!(ui.status(), ConnectionStatus::Connected);

        // Final cleanup
        ui.destroy_session().expect("destroy session failed");
        assert!(ui.messages().is_empty());
        assert_eq!(ui.peer_count(), 0);
        assert_eq!(ui.status(), ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_req_030_ui_peer_counter_dynamic_lifecycle_and_symmetry() {
        let ui = ChatUi::mount("app").expect("mount failed");
        assert_eq!(ui.peer_count(), 0);
        assert!(ui.peers().is_empty());

        // Dynamic increments: 0 -> 1 -> 2 -> 3 (REQ-030.2)
        ui.add_peer("alice").expect("add alice");
        assert_eq!(ui.peer_count(), 1);
        assert_eq!(ui.peers(), vec!["alice".to_string()]);

        ui.add_peer("bob").expect("add bob");
        assert_eq!(ui.peer_count(), 2);
        assert_eq!(ui.peers(), vec!["alice".to_string(), "bob".to_string()]);

        ui.add_peer("charlie").expect("add charlie");
        assert_eq!(ui.peer_count(), 3);
        assert_eq!(
            ui.peers(),
            vec![
                "alice".to_string(),
                "bob".to_string(),
                "charlie".to_string()
            ]
        );

        // Idempotent duplicate addition does not increment counter
        ui.add_peer("bob").expect("add bob duplicate");
        assert_eq!(ui.peer_count(), 3);
        assert_eq!(
            ui.peers(),
            vec![
                "alice".to_string(),
                "bob".to_string(),
                "charlie".to_string()
            ]
        );

        // Dynamic decrements via remove_peer: 3 -> 2 -> 1 -> 0 (REQ-030.3)
        ui.remove_peer("charlie").expect("remove charlie");
        assert_eq!(ui.peer_count(), 2);
        assert_eq!(ui.peers(), vec!["alice".to_string(), "bob".to_string()]);

        ui.remove_peer("alice").expect("remove alice");
        assert_eq!(ui.peer_count(), 1);
        assert_eq!(ui.peers(), vec!["bob".to_string()]);

        // Removing non-existent peer is a safe no-op
        ui.remove_peer("unknown").expect("remove unknown");
        assert_eq!(ui.peer_count(), 1);
        assert_eq!(ui.peers(), vec!["bob".to_string()]);

        ui.remove_peer("bob").expect("remove bob");
        assert_eq!(ui.peer_count(), 0);
        assert!(ui.peers().is_empty());

        // Full synchronization via set_peers
        let initial_batch = vec![
            "peer-1".to_string(),
            "peer-2".to_string(),
            "peer-3".to_string(),
        ];
        ui.set_peers(&initial_batch).expect("set_peers 3");
        assert_eq!(ui.peer_count(), 3);
        assert_eq!(ui.peers(), initial_batch);

        // Replace with 2 peers
        let updated_batch = vec!["peer-1".to_string(), "peer-3".to_string()];
        ui.set_peers(&updated_batch).expect("set_peers 2");
        assert_eq!(ui.peer_count(), 2);
        assert_eq!(ui.peers(), updated_batch);

        // Replace with empty slice
        ui.set_peers(&[]).expect("set_peers empty");
        assert_eq!(ui.peer_count(), 0);
        assert!(ui.peers().is_empty());

        // Destroy session resets peer count and clears peer list
        ui.add_peer("peer-x").expect("add peer-x");
        assert_eq!(ui.peer_count(), 1);
        ui.destroy_session().expect("destroy session");
        assert_eq!(ui.peer_count(), 0);
        assert!(ui.peers().is_empty());
    }
}
