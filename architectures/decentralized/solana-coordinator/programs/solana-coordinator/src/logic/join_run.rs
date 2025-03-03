use anchor_lang::prelude::*;
use psyche_solana_authorizer::state::Authorization;

use crate::bytes_from_string;
use crate::program_error::ProgramError;
use crate::ClientId;
use crate::CoordinatorAccount;
use crate::CoordinatorInstance;

pub const JOIN_RUN_AUTHORIZATION_SCOPE: &[u8] = b"CoordinatorJoinRun";

#[derive(Accounts)]
#[instruction(params: JoinRunParams)]
pub struct JoinRunAccounts<'info> {
    #[account()]
    pub user: Signer<'info>,

    #[account(
        constraint = authorization.is_valid_for(
            &coordinator_instance.authority,
            &user.key(),
            JOIN_RUN_AUTHORIZATION_SCOPE,
        ),
    )]
    pub authorization: Account<'info, Authorization>,

    #[account(
        seeds = [
            CoordinatorInstance::SEEDS_PREFIX,
            bytes_from_string(&coordinator_instance.run_id)
        ],
        bump = coordinator_instance.bump
    )]
    pub coordinator_instance: Account<'info, CoordinatorInstance>,

    #[account(
        mut,
        constraint = coordinator_instance.account == coordinator_account.key()
    )]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct JoinRunParams {
    pub client_id: ClientId,
}

pub fn join_run_processor(
    context: Context<JoinRunAccounts>,
    params: JoinRunParams,
) -> Result<()> {
    if &params.client_id.signer != context.accounts.user.key {
        return err!(ProgramError::SignerMismatch);
    }
    context
        .accounts
        .coordinator_account
        .load_mut()?
        .state
        .join_run(params.client_id)
}
