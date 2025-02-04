pub mod logic;
pub mod state;

use anchor_lang::prelude::*;
use logic::*;

declare_id!("CQy5JKR2Lrm16pqSY5nkMaMYSazRk2aYx99pJDNGupR7");

#[program]
pub mod psyche_solana_mining_pool {
    use super::*;

    pub fn pool_create(
        context: Context<PoolCreateAccounts>,
        params: PoolCreateParams,
    ) -> Result<()> {
        pool_create_processor(context, params)
    }

    pub fn pool_extract(
        context: Context<PoolExtractAccounts>,
        params: PoolExtractParams,
    ) -> Result<()> {
        pool_extract_processor(context, params)
    }

    pub fn pool_update(
        context: Context<PoolUpdateAccounts>,
        params: PoolUpdateParams,
    ) -> Result<()> {
        pool_update_processor(context, params)
    }

    pub fn pool_claimable(
        context: Context<PoolClaimableAccounts>,
        params: PoolClaimableParams,
    ) -> Result<()> {
        pool_claimable_processor(context, params)
    }

    pub fn lender_create(
        context: Context<LenderCreateAccounts>,
        params: LenderCreateParams,
    ) -> Result<()> {
        lender_create_processor(context, params)
    }

    pub fn lender_deposit(
        context: Context<LenderDepositAccounts>,
        params: LenderDepositParams,
    ) -> Result<()> {
        lender_deposit_processor(context, params)
    }

    pub fn lender_claim(
        context: Context<LenderClaimAccounts>,
        params: LenderClaimParams,
    ) -> Result<()> {
        lender_claim_processor(context, params)
    }
}

#[error_code]
pub enum ProgramError {
    #[msg("params.collateral_amount is too large")]
    ParamsCollateralAmountIsTooLarge,
    #[msg("params.redeemable_amount is too large")]
    ParamsRedeemableAmountIsTooLarge,
    #[msg("pool.claiming_enabled is true")]
    PoolClaimingEnabledIsTrue,
    #[msg("pool.claiming_enabled is false")]
    PoolClaimingEnabledIsFalse,
}
