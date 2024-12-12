use anchor_lang::prelude::*;

use crate::{
	error::ErrorCode,
	instructions::RepegCurve,
	state::{
		oracle::{ OraclePriceData, OracleSource },
		oracle_map::OracleMap,
		instructions::constraints::market_valid,
	},
};

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_oracle(
	ctx: Context<RepegCurve>,
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

	msg!("market.amm.oracle: {:?} -> {:?}", market.amm.oracle, oracle);

	msg!(
		"market.amm.oracle_source: {:?} -> {:?}",
		market.amm.oracle_source,
		oracle_source
	);

	market.amm.oracle = oracle;
	market.amm.oracle_source = oracle_source;

	Ok(())
}
