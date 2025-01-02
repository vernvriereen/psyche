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
pub use state::{CheckpointConfig, HubUploadInfo, InitRunError, RunInitConfig, RunInitConfigAndIO};
pub use tui::{ClientTUI, ClientTUIState};

#[derive(Clone)]
pub struct WandBInfo {
    pub project: String,
    pub run: String,
    pub group: Option<String>,
    pub entity: Option<String>,
    pub api_key: String,
}
