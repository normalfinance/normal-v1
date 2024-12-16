use anchor_lang::prelude::*;
use vault::Vault;

use crate::{ controller, state::*, State };

#[derive(Accounts)]
pub struct SetVaultStatusToBeingLiquidated<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub vault: AccountLoader<'info, Vault>,
	pub authority: Signer<'info>,
}

#[access_control(liq_not_paused(&ctx.accounts.state))]
pub fn handle_set_vault_status_to_being_liquidated<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, SetVaultStatusToBeingLiquidated<'info>>
) -> Result<()> {
	let state = &ctx.accounts.state;
	let clock = Clock::get()?;
	let vault = &mut load_mut!(ctx.accounts.vault)?;

	let AccountMaps { market_map, vault_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	controller::liquidation::set_vault_status_to_being_liquidated(
		vault,
		&market_map,
		&vault_map,
		&mut oracle_map,
		clock.slot,
		&state
	)?;

	Ok(())
}
