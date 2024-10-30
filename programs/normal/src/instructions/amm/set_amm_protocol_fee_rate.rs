use anchor_lang::prelude::*;

use crate::state::amm::AMM;

#[derive(Accounts)]
pub struct SetAMMProtocolFeeRate<'info> {
	#[account(mut)]
	pub amm: Account<'info, AMM>,
}

pub fn handle_set_amm_protocol_fee_rate(
	ctx: Context<SetAMMProtocolFeeRate>,
	protocol_fee_rate: u16
) -> Result<()> {
	ctx.accounts.amm.update_protocol_fee_rate(protocol_fee_rate)
}
