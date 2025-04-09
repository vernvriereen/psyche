pub mod create_memnet_endpoint;
pub mod get_accounts;
pub mod process_authorizer_instructions;
pub mod process_coordinator_instructions;
pub mod process_treasurer_instructions;

pub const SOLANA_TOOLING_VERSION_MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
pub const SOLANA_TOOLING_VERSION_MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
pub const SOLANA_TOOLING_VERSION_PATCH: &str = env!("CARGO_PKG_VERSION_PATCH");
