use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn create_memnet_toolbox_endpoint() -> ToolboxEndpoint {
    ToolboxEndpoint::new_program_test_with_preloaded_programs(&[(
        solana_coordinator::ID,
        "../../target/deploy/solana_coordinator",
    )])
    .await
}
