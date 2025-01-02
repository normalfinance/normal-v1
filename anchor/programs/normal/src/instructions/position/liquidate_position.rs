use anchor_lang::prelude::*;
use synth_market_map::{ get_writable_market_set, MarketSet };
use vault::Vault;
use synth_market::VaultsConfig;
use vault_map::get_writable_vault_set;
use crate::instructions::constraints::*;

use crate::instructions::optional_accounts::load_maps;
use crate::{ controller, load_mut, state::*, validate };

#[derive(Accounts)]
pub struct LiquidateVault<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_user(&liquidator, &authority)?
    )]
	pub liquidator: AccountLoader<'info, User>,
	#[account(
	    mut,
	    constraint = is_stats_for_user(&liquidator, &liquidator_stats)?
	)]
	pub liquidator_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
}

#[access_control(liq_not_paused(&ctx.accounts.state))]
pub fn handle_liquidate_vault<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, LiquidateVault<'info>>,
	vault_index: u16,
	liquidator_max_base_asset_amount: u64,
	limit_price: Option<u64>
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let slot = clock.slot;
	let state = &ctx.accounts.state;

	let user_key = ctx.accounts.user.key();
	let liquidator_key = ctx.accounts.liquidator.key();

	validate!(user_key != liquidator_key, ErrorCode::UserCantLiquidateThemself)?;

	let user = &mut load_mut!(ctx.accounts.user)?;
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;
	let liquidator = &mut load_mut!(ctx.accounts.liquidator)?;
	let liquidator_stats = &mut load_mut!(ctx.accounts.liquidator_stats)?;

	let AccountMaps { market_map, vault_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&get_writable_vault_set(vault_index),
		&MarketSet::new(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	controller::liquidation::liquidate_vault(
		vault_index,
		liquidator_max_base_asset_amount,
		limit_price,
		user,
		&user_key,
		user_stats,
		liquidator,
		&liquidator_key,
		liquidator_stats,
		&market_map,
		&vault_map,
		&mut oracle_map,
		slot,
		now,
		state
	)?;

	Ok(())
}
