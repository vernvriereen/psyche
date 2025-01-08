use solana_toolbox_endpoint::{
    toolbox_endpoint_program_test_builtin_program_anchor, ToolboxEndpoint,
    ToolboxEndpointLoggerPrint,
};

pub async fn create_memnet_endpoint() -> ToolboxEndpoint {
    let mut endpoint = ToolboxEndpoint::new_program_test_with_builtin_programs(&[
        toolbox_endpoint_program_test_builtin_program_anchor!(
            "solana_coordinator",
            solana_coordinator::ID,
            solana_coordinator::entry
        ),
    ])
    .await;
    endpoint.add_logger(Box::new(ToolboxEndpointLoggerPrint::new()));
    endpoint
}
