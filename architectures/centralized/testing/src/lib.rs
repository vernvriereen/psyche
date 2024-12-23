pub mod client;
pub mod server;
pub mod test_utils;
pub const RUN_ID: &str = "test";

// Model Parameters
//
pub const WARMUP_TIME: u64 = 5;
pub const MAX_ROUND_TRAIN_TIME: u64 = 5;
pub const ROUND_WITNESS_TIME: u64 = 2;
pub const COOLDOWN_TIME: u64 = 3;
