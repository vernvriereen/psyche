use solana_toolbox_endpoint::{toolbox_endpoint_program_test_builtin_program, ToolboxEndpoint};

pub async fn create_memnet_endpoint() -> ToolboxEndpoint {
    ToolboxEndpoint::new_program_test_with_builtin_programs(&[
        toolbox_endpoint_program_test_builtin_program!(
            solana_coordinator::ID,
            |program_id, accounts, data| {
                let accounts = Box::leak(Box::new(accounts.to_vec()));
                solana_coordinator::entry(program_id, accounts, data)
            }
        ),
    ])
    .await
}
