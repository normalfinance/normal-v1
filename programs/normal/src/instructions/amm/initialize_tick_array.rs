use anchor_lang::prelude::*;

use crate::{
	controller::tick,
	state::{ market::Market, tick::{ Tick, TickArray } },
};

#[derive(Accounts)]
#[instruction(start_tick_index: i32)]
pub struct InitializeTickArray<'info> {
	pub market: Account<'info, Market>,

	#[account(mut)]
	pub funder: Signer<'info>,

	#[account(
		init,
		payer = funder,
		seeds = [
			b"tick_array",
			market.key().as_ref(),
			start_tick_index.to_string().as_bytes(),
		],
		bump,
		space = TickArray::LEN
	)]
	pub tick_array: AccountLoader<'info, TickArray>,

	pub system_program: Program<'info, System>,
}

pub fn handle_initialize_tick_array(
	ctx: Context<InitializeTickArray>,
	start_tick_index: i32
) -> Result<()> {
	let mut tick_array = ctx.accounts.tick_array.load_init()?;

	if
		!Tick::check_is_valid_start_tick(
			start_tick_index,
			&ctx.accounts.market.amm.tick_spacing
		)
	{
		return Err(ErrorCode::InvalidStartTick.into());
	}

	tick_array.market = &ctx.accounts.markets.key();
	tick_array.start_tick_index = start_tick_index;

	Ok(())
}
