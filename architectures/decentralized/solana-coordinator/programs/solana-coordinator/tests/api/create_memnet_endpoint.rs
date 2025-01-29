use solana_toolbox_endpoint::{
    toolbox_endpoint_program_test_builtin_program_anchor, ToolboxEndpoint,
    ToolboxEndpointLoggerPrinter,
};

pub async fn create_memnet_endpoint() -> ToolboxEndpoint {
    let mut endpoint = ToolboxEndpoint::new_program_test_with_builtin_programs(&[
        toolbox_endpoint_program_test_builtin_program_anchor!(
            "psyche_solana_coordinator",
            psyche_solana_coordinator::ID,
            psyche_solana_coordinator::entry
        ),
    ])
    .await;
    endpoint.add_logger(Box::new(ToolboxEndpointLoggerPrinter::default()));
    endpoint
}
