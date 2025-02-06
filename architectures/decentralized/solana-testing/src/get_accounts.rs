use anchor_lang::AccountDeserialize;
use psyche_solana_coordinator::coordinator_account_from_bytes;
use psyche_solana_coordinator::CoordinatorInstanceState;
use psyche_solana_treasurer::state::Run;
use solana_sdk::pubkey::Pubkey;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointError;

pub async fn get_coordinator_account_state(
    endpoint: &mut ToolboxEndpoint,
    coordinator_account: &Pubkey,
) -> Result<CoordinatorInstanceState, ToolboxEndpointError> {
    let coordinator_account_data = get_account_data_or_else(
        endpoint,
        coordinator_account,
        "coordinator_account",
    )
    .await?;
    Ok(coordinator_account_from_bytes(&coordinator_account_data)
        .map_err(|_| {
            ToolboxEndpointError::Custom(
                "Unable to decode coordinator_account data".to_string(),
            )
        })?
        .state)
}

pub async fn get_run(
    endpoint: &mut ToolboxEndpoint,
    run: &Pubkey,
) -> Result<Run, ToolboxEndpointError> {
    let run_data = get_account_data_or_else(endpoint, run, "run").await?;
    Run::try_deserialize(&mut run_data.as_slice()).map_err(|_| {
        ToolboxEndpointError::Custom("Unable to decode run data".to_string())
    })
}

pub async fn get_participant(
    endpoint: &mut ToolboxEndpoint,
    participant: &Pubkey,
) -> Result<Run, ToolboxEndpointError> {
    let participant_data =
        get_account_data_or_else(endpoint, participant, "participant").await?;
    Run::try_deserialize(&mut participant_data.as_slice()).map_err(|_| {
        ToolboxEndpointError::Custom(
            "Unable to decode participant data".to_string(),
        )
    })
}

async fn get_account_data_or_else(
    endpoint: &mut ToolboxEndpoint,
    address: &Pubkey,
    name: &str,
) -> Result<Vec<u8>, ToolboxEndpointError> {
    endpoint.get_account_data(address).await?.ok_or_else(|| {
        ToolboxEndpointError::Custom(format!(
            "Account does not exist: {}",
            name
        ))
    })
}
