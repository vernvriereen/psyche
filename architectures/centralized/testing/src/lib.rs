pub mod server;
pub mod test_utils;
pub const RUN_ID: &str = "test";
pub const SERVER_PORT: u16 = 8080;

// Model Parameters
//
// IMPORTANT: If you modify these values, ensure they are also updated in
// the corresponding configuration file: config/testing/state.toml.
pub const WARMUP_TIME: u64 = 3;
pub const MAX_ROUND_TRAIN_TIME: u64 = 3;
