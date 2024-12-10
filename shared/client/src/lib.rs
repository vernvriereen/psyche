mod client;
mod fetch_data;
mod protocol;
mod serialized_distro;
mod state;
mod trainer;
mod tui;

pub use client::Client;
pub use protocol::{TrainingResult, TransmittableDistroResult, NC};
pub use serialized_distro::{
    distro_results_from_reader, distro_results_to_bytes, SerializedDistroResult,
};
pub use state::{CheckpointConfig, HubUploadInfo, RunInitConfig, RunInitConfigAndIO};
pub use tui::{ClientTUI, ClientTUIState};

#[derive(Clone)]
pub struct WandBInfo {
    pub project: String,
    pub run: String,
    pub group: Option<String>,
    pub entity: Option<String>,
    pub api_key: String,
}

pub fn u8_to_string(slice: &[u8; 64]) -> String {
    String::from_utf8(slice.to_vec()).unwrap()
}
