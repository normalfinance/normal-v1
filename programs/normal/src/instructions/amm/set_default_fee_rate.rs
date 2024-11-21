use anchor_lang::prelude::*;

use crate::state::{FeeTier, AMMsConfig};

#[derive(Accounts)]
pub struct SetDefaultFeeRate<'info> {
    pub amms_config: Account<'info, AMMsConfig>,

    #[account(mut, has_one = amms_config)]
    pub fee_tier: Account<'info, FeeTier>,

    #[account(address = amms_config.fee_authority)]
    pub fee_authority: Signer<'info>,
}

/*
   Updates the default fee rate on a FeeTier object.
*/
pub fn handler(ctx: Context<SetDefaultFeeRate>, default_fee_rate: u16) -> Result<()> {
    ctx.accounts
        .fee_tier
        .update_default_fee_rate(default_fee_rate)
}
