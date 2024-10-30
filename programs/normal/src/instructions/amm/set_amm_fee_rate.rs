use anchor_lang::prelude::*;

use crate::state::amm::AMM;

#[derive(Accounts)]
pub struct SetAMMFeeRate<'info> {
	#[account(mut)]
	pub amm: Account<'info, AMM>,
}

pub fn handle_set_amm_fee_rate(
	ctx: Context<SetAMMFeeRate>,
	fee_rate: u16
) -> Result<()> {
	ctx.accounts.amm.update_fee_rate(fee_rate)
}
