use anchor_lang::prelude::*;

use crate::{
	constants::main::MAX_INDEX_MARKET_EXPENSE_RATIO,
	load_mut,
	state::index_market::IndexVisibility,
};

use super::update_index_market_visibility::UpdateIndexFund;

#[access_control(index_market_valid(&ctx.accounts.market))]
pub fn handle_update_index_market_visibility(
	ctx: Context<UpdateIndexFund>,
	visibility: IndexVisibility
) -> Result<()> {
	let index_market = &mut load_mut!(ctx.accounts.index_market)?;

	msg!("updating index market {} visibility", index_market.market_index);

	msg!(
		"index_market.visibility: {:?} -> {:?}",
		index_market.visibility,
		visibility
	);

	// private ?> public
	// public > private ONLY if no additional token holders

	let token_holders = 0;

	if index_market.visibility == IndexVisibility::Public && token_holders > 1 {
		return Err(ErrorCode::FeeRateMaxExceeded.into());
	}

	ctx.accounts.index_market.visibility = visibility;

	Ok(())
}
