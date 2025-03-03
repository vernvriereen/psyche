use anchor_lang::prelude::*;
use psyche_coordinator::model::Model;
use psyche_coordinator::CoordinatorConfig;
use psyche_solana_coordinator::cpi::accounts::OwnerCoordinatorAccounts;
use psyche_solana_coordinator::cpi::set_future_epoch_rates;
use psyche_solana_coordinator::cpi::set_paused;
use psyche_solana_coordinator::cpi::update_coordinator_config_model;
use psyche_solana_coordinator::program::PsycheSolanaCoordinator;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_coordinator::CoordinatorInstance;

use crate::state::Run;

#[derive(Accounts)]
#[instruction(params: RunUpdateParams)]
pub struct RunUpdateAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    #[account(
        constraint = run.authority == authority.key(),
        constraint = run.coordinator_instance == coordinator_instance.key(),
        constraint = run.coordinator_account == coordinator_account.key(),
    )]
    pub run: Box<Account<'info, Run>>,

    #[account()]
    pub coordinator_instance: Account<'info, CoordinatorInstance>,

    #[account(mut)]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,

    #[account()]
    pub coordinator_program: Program<'info, PsycheSolanaCoordinator>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RunUpdateParams {
    pub config: Option<CoordinatorConfig<ClientId>>,
    pub model: Option<Model>,
    pub epoch_earning_rate: Option<u64>,
    pub epoch_slashing_rate: Option<u64>,
    pub paused: Option<bool>,
}

pub fn run_update_processor(
    context: Context<RunUpdateAccounts>,
    params: RunUpdateParams,
) -> Result<()> {
    let run = &context.accounts.run;
    let run_signer_seeds: &[&[&[u8]]] =
        &[&[Run::SEEDS_PREFIX, &run.identity.to_bytes(), &[run.bump]]];

    if params.config.is_some() || params.model.is_some() {
        update_coordinator_config_model(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    instance: context
                        .accounts
                        .coordinator_instance
                        .to_account_info(),
                    account: context
                        .accounts
                        .coordinator_account
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            params.config,
            params.model,
        )?;
    }

    if params.epoch_earning_rate.is_some()
        || params.epoch_slashing_rate.is_some()
    {
        set_future_epoch_rates(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    instance: context
                        .accounts
                        .coordinator_instance
                        .to_account_info(),
                    account: context
                        .accounts
                        .coordinator_account
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            params.epoch_earning_rate,
            params.epoch_slashing_rate,
        )?;
    }

    if let Some(paused) = params.paused {
        set_paused(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    instance: context
                        .accounts
                        .coordinator_instance
                        .to_account_info(),
                    account: context
                        .accounts
                        .coordinator_account
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            paused,
        )?;
    }

    Ok(())
}
