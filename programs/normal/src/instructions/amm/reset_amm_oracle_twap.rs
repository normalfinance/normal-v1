use anchor_lang::prelude::*;

use crate::instructions::constraints::{ market_valid, valid_oracle_for_amm };
use crate::{ state::{ self, market::Market }, State };

#[derive(Accounts)]
pub struct RepegCurve<'info> {
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub market: AccountLoader<'info, Market>,
	/// CHECK: checked in `repeg_curve` ix constraint
	pub oracle: AccountInfo<'info>,
	pub admin: Signer<'info>,
}

#[access_control(
    market_valid(&ctx.accounts.market)
    valid_oracle_for_amm(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_reset_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
	// admin failsafe to reset amm oracle_twap to the mark_twap

	let market = &mut load_mut!(ctx.accounts.market)?;

	msg!("resetting amm oracle twap for market {}", market.market_index);
	msg!(
		"amm.historical_oracle_data.last_oracle_price_twap: {:?} -> {:?}",
		amm.historical_oracle_data.last_oracle_price_twap,
		amm.last_mark_price_twap.cast::<i64>()?
	);

	msg!(
		"amm.historical_oracle_data.last_oracle_price_twap_ts: {:?} -> {:?}",
		amm.historical_oracle_data.last_oracle_price_twap_ts,
		amm.last_mark_price_twap_ts
	);

	amm.historical_oracle_data.last_oracle_price_twap =
		amm.last_mark_price_twap.cast::<i64>()?;
	amm.historical_oracle_data.last_oracle_price_twap_ts =
		amm.last_mark_price_twap_ts;

	Ok(())
}
