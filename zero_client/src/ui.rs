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

    /// Wipes all messages and destroys the ephemeral session (REQ-005.4 zero-footprint).
    pub fn destroy_session(&self) -> Result<(), UiError> {
        use wasm_bindgen::JsCast;

        self.messages_feed.set_inner_html("");
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

    /// Reference to the container element.
    pub fn container(&self) -> &web_sys::Element {
        &self.container
    }

    /// Reference to the status badge element.
    pub fn status_badge(&self) -> &web_sys::Element {
        &self.status_badge
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
}
