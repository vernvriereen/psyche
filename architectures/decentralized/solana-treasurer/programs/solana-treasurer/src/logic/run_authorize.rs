use anchor_lang::prelude::*;
use psyche_solana_authorizer::cpi::accounts::AuthorizationCloseAccounts;
use psyche_solana_authorizer::cpi::accounts::AuthorizationCreateAccounts;
use psyche_solana_authorizer::cpi::accounts::AuthorizationGrantorUpdateAccounts;
use psyche_solana_authorizer::cpi::authorization_close;
use psyche_solana_authorizer::cpi::authorization_create;
use psyche_solana_authorizer::cpi::authorization_grantor_update;
use psyche_solana_authorizer::logic::AuthorizationCloseParams;
use psyche_solana_authorizer::logic::AuthorizationCreateParams;
use psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams;
use psyche_solana_authorizer::program::PsycheSolanaAuthorizer;
use psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;

use crate::state::Run;

#[derive(Accounts)]
#[instruction(params: RunAuthorizeParams)]
pub struct RunAuthorizeAccounts<'info> {
    #[account()]
    pub payer: Signer<'info>,

    #[account()]
    pub authority: Signer<'info>,

    #[account(
        constraint = run.authority == authority.key(),
    )]
    pub run: Box<Account<'info, Run>>,

    #[account(mut)]
    pub authorization: UncheckedAccount<'info>,

    #[account()]
    pub authorizer_program: Program<'info, PsycheSolanaAuthorizer>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum RunAuthorizeAction {
    Create,
    Update { active: bool },
    Close,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RunAuthorizeParams {
    pub action: RunAuthorizeAction,
    pub user: Pubkey,
}

pub fn run_authorize_processor(
    context: Context<RunAuthorizeAccounts>,
    params: RunAuthorizeParams,
) -> Result<()> {
    let run = &context.accounts.run;
    let run_signer_seeds: &[&[&[u8]]] =
        &[&[Run::SEEDS_PREFIX, &run.identity.to_bytes(), &[run.bump]]];

    match params.action {
        RunAuthorizeAction::Create => authorization_create(
            CpiContext::new(
                context.accounts.authorizer_program.to_account_info(),
                AuthorizationCreateAccounts {
                    payer: context.accounts.payer.to_account_info(),
                    grantor: context.accounts.run.to_account_info(),
                    authorization: context
                        .accounts
                        .authorization
                        .to_account_info(),
                    system_program: context
                        .accounts
                        .system_program
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            AuthorizationCreateParams {
                grantee: params.user,
                scope: JOIN_RUN_AUTHORIZATION_SCOPE.to_vec(),
            },
        ),
        RunAuthorizeAction::Update { active } => authorization_grantor_update(
            CpiContext::new(
                context.accounts.authorizer_program.to_account_info(),
                AuthorizationGrantorUpdateAccounts {
                    grantor: context.accounts.run.to_account_info(),
                    authorization: context
                        .accounts
                        .authorization
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            AuthorizationGrantorUpdateParams { active },
        ),
        RunAuthorizeAction::Close => authorization_close(
            CpiContext::new(
                context.accounts.authorizer_program.to_account_info(),
                AuthorizationCloseAccounts {
                    grantor: context.accounts.run.to_account_info(),
                    spill: context.accounts.payer.to_account_info(),
                    authorization: context
                        .accounts
                        .authorization
                        .to_account_info(),
                    system_program: context
                        .accounts
                        .system_program
                        .to_account_info(),
                },
            )
            .with_signer(run_signer_seeds),
            AuthorizationCloseParams {},
        ),
    }
}
