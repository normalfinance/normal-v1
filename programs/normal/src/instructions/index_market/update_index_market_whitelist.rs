use anchor_lang::prelude::*;

use crate::load_mut;

use super::update_index_market_visibility::UpdateIndexFund;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_index_market_whitelist(
	ctx: Context<UpdateIndexFund>,
	whitelist: Vec<Pubkey>
) -> Result<()> {
	let index_market = &mut load_mut!(ctx.accounts.index_market)?;

	msg!("updating index market {} whitelist", index_market.market_index);

	msg!(
		"index_market.whitelist: {:?} -> {:?}",
		index_market.whitelist,
		whitelist
	);

	ctx.accounts.index_market.whitelist = whitelist;
	Ok(())
}
