use anchor_lang::prelude::*;

use crate::state::{AMM, AMMsConfig};

#[derive(Accounts)]
pub struct SetFeeRate<'info> {
    pub amms_config: Account<'info, AMMsConfig>,

    #[account(mut, has_one = amms_config)]
    pub amm: Account<'info, AMM>,

    #[account(address = amms_config.fee_authority)]
    pub fee_authority: Signer<'info>,
}

pub fn handler(ctx: Context<SetFeeRate>, fee_rate: u16) -> Result<()> {
    ctx.accounts.amm.update_fee_rate(fee_rate)
}
