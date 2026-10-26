//! Zero-footprint WASM P2P client crate.
pub use zero_protocol;

pub mod bootstrap;
pub mod crypto;
pub mod invite;
pub mod mesh;
pub mod ui;
pub mod webrtc;

#[allow(unused_imports)]
pub use bootstrap::*;
pub use crypto::WasmCryptoSession;
#[allow(unused_imports)]
pub use crypto::*;
#[allow(unused_imports)]
pub use invite::*;
#[allow(unused_imports)]
pub use mesh::*;
#[allow(unused_imports)]
pub use ui::*;
#[allow(unused_imports)]
pub use webrtc::*;
