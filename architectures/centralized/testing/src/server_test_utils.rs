use psyche_centralized_server::app::DataServerInfo;
use psyche_data_provider::TokenSize;
use std::path::PathBuf;

pub fn data_server_info_default_for_testing() -> DataServerInfo {
    DataServerInfo {
        dir: PathBuf::from("./"),
        token_size: TokenSize::TwoBytes,
        seq_len: 2048,
        shuffle_seed: [1; 32],
    }
}
