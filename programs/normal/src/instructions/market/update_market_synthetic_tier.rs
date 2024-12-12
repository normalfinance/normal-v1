use anchor_lang::prelude::*;

use crate::{
	controller,
	error::ErrorCode,
	instructions::optional_accounts::{ load_maps, AccountMaps },
	state::{ market::SyntheticTier, market_map::get_writable_market_set },
};
use crate::instructions::constraints::market_valid;
use super::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_synthetic_tier(
	ctx: Context<AdminUpdateMarket>,
	synthetic_tier: SyntheticTier
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("market {}", market.market_index);

	msg!(
		"market.synthetic_tier: {:?} -> {:?}",
		market.synthetic_tier,
		synthetic_tier
	);

	market.synthetic_tier = synthetic_tier;

	let AccountMaps { market_map, oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&get_writable_market_set(market_index),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	let prev_max_insurance_claim_pct = market.max_insurance_claim_pct;
	controller::insurance::update_market_max_insurance_claim(&market_map);
	let new_max_insurance_claim_pct = market.max_insurance_claim_pct;

	msg!(
		"market.max_insurance_claim_pct: {} -> {}",
		prev_max_insurance_claim_pct,
		new_max_insurance_claim_pct
	);

	Ok(())
}
