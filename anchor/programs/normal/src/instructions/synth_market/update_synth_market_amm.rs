use anchor_lang::prelude::*;

use crate::state::{ amm::AMM, synth_market::SynthMarket };

use super::AdminUpdateSynthMarket;

#[derive(Accounts)]
pub struct AdminUpdateSynthMarketAMM<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub synth_market: AccountLoader<'info, SynthMarket>,
	pub amm: AccountLoader<'info, AMM>,
}

#[access_control(synth_market_valid(&ctx.accounts.synth_market))]
pub fn handle_update_synth_market_amm(
	ctx: Context<AdminUpdateSynthMarketAMM>,
	amm: Pubkey
) -> Result<()> {
	let synth_market = &mut load_mut!(ctx.accounts.synth_market)?;
	msg!("synth market {}", synth_market.market_index);

	let clock = Clock::get()?;

	validate!(
		ctx.accounts.amm.key == &amm,
		ErrorCode::DefaultError,
		"amm account info ({:?}) and ix data ({:?}) must match",
		ctx.accounts.amm.key,
		amm
	)?;

	// Verify oracle is readable
	let OraclePriceData {
		price: _oracle_price,
		delay: _oracle_delay,
		..
	} = get_oracle_price(&oracle_source, &ctx.accounts.oracle, clock.slot)?;

	msg!("synth_market.amm: {:?} -> {:?}", synth_market.amm, amm);

	synth_market.amm = amm;

	Ok(())
}
