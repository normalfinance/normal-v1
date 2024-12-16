use anchor_lang::prelude::*;

use crate::instructions::constraints::market_valid;
use synth_market::{ Market, MarketStatus };

use crate::{ state::*, State };

#[derive(Accounts)]
pub struct FreezeMarketOracle<'info> {
	#[account(mut)]
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
	#[account(
		init,
		seeds = [b"market", state.number_of_markets.to_le_bytes().as_ref()],
		space = Market::SIZE,
		bump,
		payer = admin
	)]
	pub market: AccountLoader<'info, Market>,
	/// CHECK: checked in `initialize_perp_market`
	pub oracle: AccountInfo<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_freeze_market_oracle(
	ctx: Context<FreezeMarketOracle>,
	oracle: Pubkey,
	oracle_source: OracleSource
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("market {}", market.market_index);

	let clock = Clock::get()?;

	OracleMap::validate_oracle_account_info(&ctx.accounts.oracle)?;

	validate!(
		ctx.accounts.oracle.key == &oracle,
		ErrorCode::DefaultError,
		"oracle account info ({:?}) and ix data ({:?}) must match",
		ctx.accounts.oracle.key,
		oracle
	)?;

	// Verify oracle is readable
	let OraclePriceData {
		price: _oracle_price,
		delay: _oracle_delay,
		..
	} = get_oracle_price(&oracle_source, &ctx.accounts.oracle, clock.slot)?;

	msg!("amm.oracle: {:?} -> {:?}", amm.oracle, oracle);

	msg!(
		"amm.oracle_source: {:?} -> {:?}",
		amm.oracle_source,
		oracle_source
	);

	// amm.oracle = oracle;
	// amm.oracle_source = oracle_source;

	Ok(())
}
