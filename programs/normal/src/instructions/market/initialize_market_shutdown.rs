use anchor_lang::prelude::*;

use crate::instructions::constraints::market_valid;
use market::{ Market, MarketStatus };
use paused_operations::VaultOperation;

use crate::state::*;

use super::update_market_liquidation_penalty::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_initialize_market_shutdown(
	ctx: Context<AdminUpdateMarket>,
	expiry_ts: i64
) -> Result<()> {
	let clock: Clock = Clock::get()?;
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("updating market {} expiry", market.market_index);

	// Pause vault Create, Deposit, Lend, and Delete
	market.paused_operations = EMERGENCY_SHUTDOWN_PAUSED_OPERATIONS;

	VaultOperation::log_all_operations_paused(market.paused_operations);

	// TODO: freeze collateral prices

	// vault owners can withraw any excess collateral if their debt obligations are met

	validate!(
		clock.unix_timestamp < expiry_ts,
		ErrorCode::DefaultError,
		"Market expiry ts must later than current clock timestamp"
	)?;

	msg!("market.status {:?} -> {:?}", market.status, MarketStatus::ReduceOnly);
	msg!("market.expiry_ts {} -> {}", market.expiry_ts, expiry_ts);

	// automatically enter reduce only
	market.status = MarketStatus::ReduceOnly;
	market.expiry_ts = expiry_ts;

	Ok(())
}
