mod client;
mod fetch_data;
mod protocol;
mod serialized_distro;
mod state;
mod trainer;
mod tui;

pub use client::Client;
pub use protocol::{BroadcastMessage, DistroResult, Payload, PeerAnnouncement, TrainingResult, NC};
pub use serialized_distro::{
    disto_results_to_bytes, distro_results_from_reader, SerializedDistroResult,
};
pub use state::{BatchShuffleType, CheckpointUploadInfo, StateOptions};
pub use tui::{ClientTUI, ClientTUIState};

#[derive(Clone)]
pub struct WandBInfo {
    pub project: String,
    pub run: String,
    pub entity: Option<String>,
    pub api_key: String,
}
