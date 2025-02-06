use anchor_lang::AccountDeserialize;
use psyche_solana_coordinator::coordinator_account_from_bytes;
use psyche_solana_coordinator::CoordinatorInstanceState;
use psyche_solana_treasurer::state::Run;
use solana_sdk::pubkey::Pubkey;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointError;

pub async fn get_data_coordinator_instance_state(
    endpoint: &mut ToolboxEndpoint,
    coordinator_account: &Pubkey,
) -> Result<CoordinatorInstanceState, ToolboxEndpointError> {
    let data = endpoint.get_account_data(coordinator_account).await?.unwrap();
    Ok(coordinator_account_from_bytes(&data)
        .map_err(|_| {
            ToolboxEndpointError::Custom(
                "Unable to decode coordinator account data".to_string(),
            )
        })?
        .state)
}

pub async fn get_data_run(
    endpoint: &mut ToolboxEndpoint,
    run: &Pubkey,
) -> Result<Run, ToolboxEndpointError> {
    let run_data = endpoint.get_account_data(run).await?.unwrap();
    Ok(Run::try_deserialize(&mut run_data.as_slice()).unwrap())
}

pub async fn get_data_participant(
    endpoint: &mut ToolboxEndpoint,
    participant: &Pubkey,
) -> Result<Run, ToolboxEndpointError> {
    let participant_data =
        endpoint.get_account_data(participant).await?.unwrap();
    Ok(Run::try_deserialize(&mut participant_data.as_slice()).unwrap())
}
