mod client;
mod state;
mod trainer;
mod tui;
mod protocol;

pub use client::Client;
pub use protocol::{NC, BroadcastMessage, Payload};
pub use tui::{ClientTUI, ClientTUIState};