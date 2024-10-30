use amm::AMM;
use anchor_lang::prelude::*;
use tick::TickArray;

use crate::state::*;

#[derive(Accounts)]
#[instruction(start_tick_index: i32)]
pub struct InitializeAMMTickArray<'info> {
	pub amm: Account<'info, AMM>,

	#[account(mut)]
	pub funder: Signer<'info>,

	#[account(
		init,
		payer = funder,
		seeds = [
			b"tick_array",
			amm.key().as_ref(),
			start_tick_index.to_string().as_bytes(),
		],
		bump,
		space = TickArray::LEN
	)]
	pub tick_array: AccountLoader<'info, TickArray>,

	pub system_program: Program<'info, System>,
}

pub fn handle_initialize_amm_tick_array(
	ctx: Context<InitializeAMMTickArray>,
	start_tick_index: i32
) -> Result<()> {
	let mut tick_array = ctx.accounts.tick_array.load_init()?;
	tick_array.initialize(&ctx.accounts.amm, start_tick_index)
}
