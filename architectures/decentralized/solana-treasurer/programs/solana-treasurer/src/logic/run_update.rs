use crate::state::Run;
use anchor_lang::prelude::*;
use psyche_coordinator::model::Model;
use psyche_coordinator::CoordinatorConfig;
use psyche_solana_coordinator::cpi::accounts::OwnerCoordinatorAccounts;
use psyche_solana_coordinator::cpi::set_paused;
use psyche_solana_coordinator::cpi::set_whitelist;
use psyche_solana_coordinator::cpi::update_coordinator_config_model;
use psyche_solana_coordinator::program::PsycheSolanaCoordinator;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_coordinator::CoordinatorInstance;

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

    #[account(mut)]
    pub coordinator_instance: Account<'info, CoordinatorInstance>,

    #[account(mut)]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,

    #[account()]
    pub coordinator_program: Program<'info, PsycheSolanaCoordinator>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RunUpdateParams {
    pub clients: Option<Vec<Pubkey>>,
    pub paused: Option<bool>,
    pub config: Option<CoordinatorConfig<ClientId>>,
    pub model: Option<Model>,
}

pub fn run_update_processor(
    context: Context<RunUpdateAccounts>,
    params: RunUpdateParams,
) -> Result<()> {
    let run = &context.accounts.run;
    let run_signer_seeds: &[&[&[u8]]] =
        &[&[Run::SEEDS_PREFIX, &run.identity.to_bytes(), &[run.bump]]];

    if let Some(clients) = params.clients {
        set_whitelist(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    instance: context.accounts.coordinator_instance.to_account_info(),
                    account: context.accounts.coordinator_account.to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            clients,
        )?;
    }

    if let Some(paused) = params.paused {
        set_paused(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    instance: context.accounts.coordinator_instance.to_account_info(),
                    account: context.accounts.coordinator_account.to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            paused,
        )?;
    }

    if params.config.is_some() || params.model.is_some() {
        update_coordinator_config_model(
            CpiContext::new(
                context.accounts.coordinator_program.to_account_info(),
                OwnerCoordinatorAccounts {
                    authority: context.accounts.run.to_account_info(),
                    instance: context.accounts.coordinator_instance.to_account_info(),
                    account: context.accounts.coordinator_account.to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            params.config,
            params.model,
        )?;
    }

    Ok(())
}
