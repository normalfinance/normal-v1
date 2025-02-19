use anchor_lang::prelude::*;

use crate::{
	controller,
	error::ErrorCode,
	instructions::optional_accounts::{ load_maps, AccountMaps },
	load_mut,
	state::{
		market::Tier,
		market_map::get_writable_market_set,
	},
};
use crate::instructions::constraints::market_valid;
use super::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_tier(
	ctx: Context<AdminUpdateMarket>,
	tier: Tier
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("market {}", market.market_index);

	msg!(
		"market.contract_tier: {:?} -> {:?}",
		market.contract_tier,
		contract_tier
	);

	market.contract_tier = contract_tier;
	Ok(())
}
