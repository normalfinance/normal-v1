use anchor_lang::prelude::*;

use crate::{ constants::main::MAX_INDEX_MARKET_EXPENSE_RATIO, load_mut };

use super::update_index_market_visibility::UpdateIndexFund;

#[access_control(index_market_valid(&ctx.accounts.market))]
pub fn handle_update_index_market_revenue_share(
	ctx: Context<UpdateIndexFund>,
	revenue_share: u64
) -> Result<()> {
	let index_market = &mut load_mut!(ctx.accounts.index_market)?;

	msg!("updating index market {} revenue_share", index_market.market_index);

	msg!(
		"index_market.revenue_share: {:?} -> {:?}",
		index_market.revenue_share,
		revenue_share
	);

	if revenue_share > MAX_INDEX_MARKET_EXPENSE_RATIO {
		return Err(ErrorCode::FeeRateMaxExceeded.into());
	}

	ctx.accounts.index_market.revenue_share = revenue_share;

	Ok(())
}
