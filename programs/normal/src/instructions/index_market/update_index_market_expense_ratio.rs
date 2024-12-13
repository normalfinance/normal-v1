use anchor_lang::prelude::*;

use crate::{ constants::main::MAX_INDEX_MARKET_EXPENSE_RATIO, load_mut };

use super::update_index_market_visibility::UpdateIndexFund;

#[access_control(index_market_valid(&ctx.accounts.market))]
pub fn handle_update_index_market_expense_ratio(
	ctx: Context<UpdateIndexFund>,
	expense_ratio: u64
) -> Result<()> {
	let index_market = &mut load_mut!(ctx.accounts.index_market)?;

	msg!("updating index market {} expense_ratio", index_market.market_index);

	msg!(
		"index_market.expense_ratio: {:?} -> {:?}",
		index_market.expense_ratio,
		expense_ratio
	);

	if expense_ratio > MAX_INDEX_MARKET_EXPENSE_RATIO {
		return Err(ErrorCode::FeeRateMaxExceeded.into());
	}

	ctx.accounts.index_market.expense_ratio = expense_ratio;

	Ok(())
}
