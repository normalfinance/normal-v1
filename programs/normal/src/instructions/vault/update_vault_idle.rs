use anchor_lang::prelude::*;
use market_map::MarketSet;
use user::User;
use vault::Vault;

use crate::{
	controller,
	instructions::optional_accounts::load_maps,
	math::margin::calculate_user_equity,
	state::*,
	validation::vault::validate_vault_is_idle,
	State,
};

#[derive(Accounts)]
pub struct UpdateVaultIdle<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_vault(&filler, &authority)?
    )]
	pub filler: AccountLoader<'info, User>,
	#[account(mut)]
	pub vault: AccountLoader<'info, Vault>,
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_update_vault_idle<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, UpdateVaultIdle<'info>>
) -> Result<()> {
	let mut vault = load_mut!(ctx.accounts.vault)?;
	let clock = Clock::get()?;

	let AccountMaps { market_map, vault_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		&MarketSet::new(),
		Clock::get()?.slot,
		None
	)?;

	let (equity, _) = calculate_user_equity(
		&user,
		&market_map,
		&vault_map,
		&mut oracle_map
	)?;

	// user flipped to idle faster if collateral is less than 1000
	let accelerated = equity < QUOTE_PRECISION_I128 * 1000;

	validate_vault_is_idle(&vault, clock.slot, accelerated)?;

	vault.idle = true;

	Ok(())
}
