use anchor_lang::prelude::*;

use crate::{
	controller,
	error::ErrorCode,
	instructions::optional_accounts::{ load_maps, AccountMaps },
	load_mut,
	state::{
		synth_market::SyntheticTier,
		synth_market_map::get_writable_market_set,
	},
};
use crate::instructions::constraints::market_valid;
use super::AdminUpdateSynthMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_synth_market_synthetic_tier(
	ctx: Context<AdminUpdateSynthMarket>,
	synthetic_tier: SyntheticTier
) -> Result<()> {
	let synth_market = &mut load_mut!(ctx.accounts.synth_market)?;
	msg!("synth market {}", synth_market.market_index);

	msg!(
		"synth_market.contract_tier: {:?} -> {:?}",
		synth_market.contract_tier,
		contract_tier
	);

	synth_market.contract_tier = contract_tier;
	Ok(())
}
