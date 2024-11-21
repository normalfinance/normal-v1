use std::cell::RefMut;

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{ TokenAccount, TokenInterface };
use solana_program::instruction::Instruction;
use solana_program::sysvar::instructions::{
	load_current_index_checked,
	load_instruction_at_checked,
	ID as IX_ID,
};

use crate::controller::position::PositionDirection;
use crate::error::ErrorCode;
use crate::ids::swift_server;
use crate::instructions::constraints::*;
use crate::instructions::optional_accounts::{ load_maps, AccountMaps };
use crate::math::casting::Cast;
use crate::math::constants::QUOTE_SPOT_MARKET_INDEX;
use crate::math::orders::{
	estimate_price_from_side,
	find_bids_and_asks_from_users,
};
use crate::math::spot_withdraw::validate_spot_market_vault_amount;
use crate::optional_accounts::{ get_token_mint, update_prelaunch_oracle };
use crate::state::fill_mode::FillMode;
use crate::state::insurance_fund_stake::InsuranceFundStake;
use crate::state::oracle_map::OracleMap;
use crate::state::order_params::{
	OrderParams,
	PlaceOrderOptions,
	SwiftOrderParamsMessage,
	SwiftServerMessage,
};
use crate::state::paused_operations::PerpOperation;

use crate::state::state::State;
use crate::state::vault::Vault;
use crate::validation::sig_verification::verify_ed25519_ix;
use crate::validation::user::validate_user_is_idle;
use crate::{
	controller,
	load,
	math,
	print_error,
	OracleSource,
	GOV_SPOT_MARKET_INDEX,
};
use crate::{ load_mut, QUOTE_PRECISION_U64 };
use crate::{ validate, QUOTE_PRECISION_I128 };

pub fn handle_update_vault_idle<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, UpdateVaultIdle<'info>>
) -> Result<()> {
	let mut vault = load_mut!(ctx.accounts.vault)?;
	let clock = Clock::get()?;

	let vault_collateral_value = get_token_value(
		token_amount,
		spot_market.decimals,
		oracle_price
	)?;

	// user flipped to idle faster if collateral is less than 1000
	let accelerated = vault_collateral_value < QUOTE_PRECISION_I128 * 1000;

	validate_vault_is_idle(&vault, clock.slot, accelerated)?;

	vault.idle = true;

	Ok(())
}

#[access_control(liq_not_paused(&ctx.accounts.state))]
pub fn handle_liquidate_vault<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, LiquidateVault<'info>>,
	market_index: u16,
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

	let AccountMaps { perp_market_map, spot_market_map, mut oracle_map } =
		load_maps(
			&mut ctx.remaining_accounts.iter().peekable(),
			&get_writable_perp_market_set(market_index),
			&MarketSet::new(),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	controller::liquidation::liquidate_vault(
		market_index,
		liquidator_max_base_asset_amount,
		limit_price,
		user,
		&user_key,
		user_stats,
		liquidator,
		&liquidator_key,
		liquidator_stats,
		&perp_market_map,
		&spot_market_map,
		&mut oracle_map,
		slot,
		now,
		state
	)?;

	Ok(())
}

#[access_control(liq_not_paused(&ctx.accounts.state))]
pub fn handle_set_vault_status_to_being_liquidated<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, SetVaultStatusToBeingLiquidated<'info>>
) -> Result<()> {
	let state = &ctx.accounts.state;
	let clock = Clock::get()?;
	let vault = &mut load_mut!(ctx.accounts.vault)?;

	let AccountMaps { perp_market_map, spot_market_map, mut oracle_map } =
		load_maps(
			&mut ctx.remaining_accounts.iter().peekable(),
			&MarketSet::new(),
			&MarketSet::new(),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	controller::liquidation::set_vault_status_to_being_liquidated(
		vaultuser,
		&perp_market_map,
		&mut oracle_map,
		clock.slot,
		&state
	)?;

	Ok(())
}

#[access_control(withdraw_not_paused(&ctx.accounts.state))]
pub fn handle_resolve_perp_bankruptcy<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, ResolveBankruptcy<'info>>,
	quote_spot_market_index: u16,
	market_index: u16
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let user_key = ctx.accounts.user.key();
	let liquidator_key = ctx.accounts.liquidator.key();

	validate!(user_key != liquidator_key, ErrorCode::UserCantLiquidateThemself)?;

	validate!(
		quote_spot_market_index == QUOTE_SPOT_MARKET_INDEX,
		ErrorCode::InvalidSpotMarketAccount
	)?;

	let user = &mut load_mut!(ctx.accounts.user)?;
	let liquidator = &mut load_mut!(ctx.accounts.liquidator)?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { perp_market_map, spot_market_map, mut oracle_map } =
		load_maps(
			remaining_accounts_iter,
			&get_writable_perp_market_set(market_index),
			&get_writable_spot_market_set(quote_spot_market_index),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	let mint = get_token_mint(remaining_accounts_iter)?;

	{
		let spot_market = &mut spot_market_map.get_ref_mut(
			&quote_spot_market_index
		)?;
		controller::insurance::attempt_settle_revenue_to_insurance_fund(
			&ctx.accounts.spot_market_vault,
			&ctx.accounts.insurance_fund_vault,
			spot_market,
			now,
			&ctx.accounts.token_program,
			&ctx.accounts.drift_signer,
			state,
			&mint
		)?;

		// reload the spot market vault balance so it's up-to-date
		ctx.accounts.spot_market_vault.reload()?;
		ctx.accounts.insurance_fund_vault.reload()?;
		math::spot_withdraw::validate_spot_market_vault_amount(
			spot_market,
			ctx.accounts.spot_market_vault.amount
		)?;
	}

	let pay_from_insurance = controller::liquidation::resolve_perp_bankruptcy(
		market_index,
		user,
		&user_key,
		liquidator,
		&liquidator_key,
		&perp_market_map,
		&spot_market_map,
		&mut oracle_map,
		now,
		ctx.accounts.insurance_fund_vault.amount
	)?;

	if pay_from_insurance > 0 {
		validate!(
			pay_from_insurance < ctx.accounts.insurance_fund_vault.amount,
			ErrorCode::InsufficientCollateral,
			"Insurance Fund balance InsufficientCollateral for payment: !{} < {}",
			pay_from_insurance,
			ctx.accounts.insurance_fund_vault.amount
		)?;

		controller::token::send_from_program_vault(
			&ctx.accounts.token_program,
			&ctx.accounts.insurance_fund_vault,
			&ctx.accounts.spot_market_vault,
			&ctx.accounts.drift_signer,
			state.signer_nonce,
			pay_from_insurance,
			&mint
		)?;

		validate!(
			ctx.accounts.insurance_fund_vault.amount > 0,
			ErrorCode::InvalidIFDetected,
			"insurance_fund_vault.amount must remain > 0"
		)?;
	}

	{
		let spot_market = &mut spot_market_map.get_ref_mut(
			&quote_spot_market_index
		)?;
		// reload the spot market vault balance so it's up-to-date
		ctx.accounts.spot_market_vault.reload()?;
		math::spot_withdraw::validate_spot_market_vault_amount(
			spot_market,
			ctx.accounts.spot_market_vault.amount
		)?;
	}

	Ok(())
}

#[access_control(withdraw_not_paused(&ctx.accounts.state))]
pub fn handle_settle_revenue_to_insurance_fund<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, SettleRevenueToInsuranceFund<'info>>,
	spot_market_index: u16
) -> Result<()> {
	let state = &ctx.accounts.state;
	let spot_market = &mut load_mut!(ctx.accounts.spot_market)?;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(
		spot_market_index == spot_market.market_index,
		ErrorCode::InvalidSpotMarketAccount,
		"invalid spot_market passed"
	)?;

	validate!(
		spot_market.insurance_fund.revenue_settle_period > 0,
		ErrorCode::RevenueSettingsCannotSettleToIF,
		"invalid revenue_settle_period settings on spot market"
	)?;

	let spot_vault_amount = ctx.accounts.spot_market_vault.amount;
	let insurance_vault_amount = ctx.accounts.insurance_fund_vault.amount;

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let time_until_next_update = math::helpers::on_the_hour_update(
		now,
		spot_market.insurance_fund.last_revenue_settle_ts,
		spot_market.insurance_fund.revenue_settle_period
	)?;

	validate!(
		time_until_next_update == 0,
		ErrorCode::RevenueSettingsCannotSettleToIF,
		"Must wait {} seconds until next available settlement time",
		time_until_next_update
	)?;

	// uses proportion of revenue pool allocated to insurance fund
	let token_amount = controller::insurance::settle_revenue_to_insurance_fund(
		spot_vault_amount,
		insurance_vault_amount,
		spot_market,
		now,
		true
	)?;

	spot_market.insurance_fund.last_revenue_settle_ts = now;

	controller::token::send_from_program_vault(
		&ctx.accounts.token_program,
		&ctx.accounts.spot_market_vault,
		&ctx.accounts.insurance_fund_vault,
		&ctx.accounts.drift_signer,
		state.signer_nonce,
		token_amount,
		&mint
	)?;

	// reload the spot market vault balance so it's up-to-date
	ctx.accounts.spot_market_vault.reload()?;
	math::spot_withdraw::validate_spot_market_vault_amount(
		spot_market,
		ctx.accounts.spot_market_vault.amount
	)?;

	Ok(())
}

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

#[derive(Accounts)]
pub struct LiquidateVault<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_vault(&liquidator, &authority)?
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

#[derive(Accounts)]
pub struct SetVaultStatusToBeingLiquidated<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub vault: AccountLoader<'info, Vault>,
	pub authority: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(spot_market_index: u16,)]
pub struct ResolveBankruptcy<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_vault(&liquidator, &authority)?
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
	#[account(
        mut,
        seeds = [b"spot_market_vault".as_ref(), spot_market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub spot_market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref(), spot_market_index.to_le_bytes().as_ref()], // todo: market_index=0 hardcode for perps?
        bump,
    )]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(constraint = state.signer.eq(&drift_signer.key()))]
	/// CHECK: forced drift_signer
	pub drift_signer: AccountInfo<'info>,
	pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct SettleRevenueToInsuranceFund<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"spot_market", market_index.to_le_bytes().as_ref()],
        bump
    )]
	pub spot_market: AccountLoader<'info, SpotMarket>,
	#[account(
        mut,
        seeds = [b"spot_market_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub spot_market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(constraint = state.signer.eq(&drift_signer.key()))]
	/// CHECK: forced drift_signer
	pub drift_signer: AccountInfo<'info>,
	#[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}
