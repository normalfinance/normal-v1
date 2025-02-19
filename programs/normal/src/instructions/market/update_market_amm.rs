use anchor_lang::prelude::*;

use crate::state::{ amm::AMM, market::Market };

use super::AdminUpdateMarket;

#[derive(Accounts)]
pub struct AdminUpdateMarketAMM<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub market: AccountLoader<'info, Market>,
	pub amm: AccountLoader<'info, AMM>,
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_amm(
	ctx: Context<AdminUpdateMarketAMM>,
	amm: Pubkey
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("synth market {}", market.market_index);

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

	msg!("market.amm: {:?} -> {:?}", market.amm, amm);

	market.amm = amm;

	Ok(())
}
