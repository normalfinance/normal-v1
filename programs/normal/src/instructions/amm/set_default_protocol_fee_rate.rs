use anchor_lang::prelude::*;

use crate::state::AMMsConfig;

#[derive(Accounts)]
pub struct SetDefaultProtocolFeeRate<'info> {
    #[account(mut)]
    pub amms_config: Account<'info, AMMsConfig>,

    #[account(address = amms_config.fee_authority)]
    pub fee_authority: Signer<'info>,
}

pub fn handler(
    ctx: Context<SetDefaultProtocolFeeRate>,
    default_protocol_fee_rate: u16,
) -> Result<()> {
    ctx.accounts
        .amms_config
        .update_default_protocol_fee_rate(default_protocol_fee_rate)
}
