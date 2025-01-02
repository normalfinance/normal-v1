use amm::AMM;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };
use anchor_spl::token_interface::TokenAccount as TokenAccountInterface;
use index_market_map::MarketSet;
use synth_market::MarketStatus;
use synth_market_map::get_writable_market_set;
use user::{ User, UserStats };
use vault::Vault;
use vault_map::get_writable_vault_set;

use crate::error::ErrorCode;
use crate::errors::ErrorCode;
use crate::instructions::optional_accounts::{ load_maps, AccountMaps };
use crate::manager::liquidity_manager::{
	calculate_liquidity_token_deltas,
	calculate_modify_liquidity,
	sync_modify_liquidity_values,
};
use crate::math::liquidation::is_user_being_liquidated;
use crate::math::safe_math::SafeMath;
use crate::math::{ self, convert_to_liquidity_delta };
use crate::{ controller, load_mut, state::*, validate };
use crate::util::{
	mint_synthetic_to_vault,
	to_timestamp_u64,
	transfer_from_owner_to_vault,
	verify_position_authority_interface,
};

#[derive(Accounts)]
#[instruction(vault_index: u16,)]
pub struct DepositCollateral<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        seeds = [b"spot_market_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub spot_market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(
        mut,
        constraint = &spot_market_vault.mint.eq(&user_token_account.mint),
        token::authority = authority
    )]
	pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

#[access_control(
    deposit_not_paused(&ctx.accounts.state)
    synth_market_valid(&ctx.accounts.synth_market)
)]
pub fn handle_deposit_into_synth_market_vault<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, DepositCollateral<'info>>,
	market_index: u16,
	amount: u64,
	reduce_only: bool
) -> Result<()> {
	/**
	 * Overview

	 Deposit amount * margin_ratio = # synthetic tokens to mint

	 Validations:
	 - [] Debt ceiling / floor 
	 - [] Paused operations
	 - [] Market status
	 - [] User limitations

	 */
	let user_key = ctx.accounts.user.key();
	let user = &mut load_mut!(ctx.accounts.user)?;

	let state = &ctx.accounts.state;
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let slot = clock.slot;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { synth_market_map, index_market_map, mut oracle_map } =
		load_maps(
			remaining_accounts_iter,
			&MarketSet::new(),
			&get_writable_synth_market_set(market_index),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	let mint = get_token_mint(remaining_accounts_iter)?;

	if amount == 0 {
		return Err(ErrorCode::InsufficientDeposit.into());
	}

	validate!(!user.is_bankrupt(), ErrorCode::UserBankrupt)?;

	let mut synth_market = synth_market_map.get_ref_mut(&market_index)?;
	let oracle_price_data = &oracle_map
		.get_price_data(&synth_market.oracle)?
		.clone();

	validate!(
		!matches!(synth_market.status, MarketStatus::Initialized),
		ErrorCode::MarketBeingInitialized,
		"Market is being initialized"
	)?;

	//

	/**
	 *  Operations:

	 - Find synthetic mint #
	 - Mint tokens > LP
	 - Transfer diff > LP
	 - Update debt, metrics, margin variables
	 - Transfer remaining diff User > Vault
	 - Update market market variables
	 - Update user and user stats
	 */

	let collateral_oracle_price_data = &oracle_map
		.get_price_data(&market.oracle)?
		.clone();

	let deposit_value = user.get_deposit_value(
		amount,
		collateral_oracle_price_data.price,
		market.get_precision().cast()?
	)?;

	// Collateral Value * Margin Ratio / Synthetic Price
	let synthetic_oracle_price_data = &oracle_map
		.get_price_data(&market.oracle)?
		.clone();

	// market.margin_ratio_initial
	let synthetic_tokens_to_mint = deposit_value
		.cast::<u128>()?
		.safe_mul(market.margin_ratio_initial)?
		.safe_mul(price.cast::<u128>()?)?
		.safe_div(precision)?
		.cast::<u64>()?;

	// Validate against debt ceiling / floor
	let updated_outstanding_debt = market.outstanding_debt.safe_plus(
		synthetic_tokens_to_mint
	)?;
	validate!(
		updated_outstanding_debt >= market.debt_ceiling,
		ErrorCode::MarketBeingInitialized,
		"Market has reached its debt ceiling"
	)?;

	// Mint synthetic tokens to LP
	mint_synthetic_to_amm(
		authority,
		token_owner_account,
		token_vault,
		token_program,
		synthetic_tokens_to_mint
	);

	// Transfer user to LP
	let amount_to_vault = amount.safe_mul(market.margin_ratio_initial)?;
	let amount_to_lp = amount.safe_sub(amount_to_vault)?;

	transfer_from_owner_to_vault(
		position_authority,
		token_owner_account,
		token_vault,
		token_program,
		amount_to_lp
	);

	// Transfer user to Vault
	transfer_from_owner_to_vault(
		position_authority,
		token_owner_account,
		vault.token_vault_collateral,
		token_program,
		amount_to_vault
	);
	ctx.accounts.market_vault.reload()?;

	// Update variables
	market.outstanding_debt = 0;
	vault.synthetic_tokens_minted = vault.synthetic_tokens_minted.safe_add(
		synthetic_tokens_to_mint
	)?;

	user.increment_total_deposits(
		amount,
		oracle_price_data.price,
		market.get_precision().cast()?
	)?;

	vault.update_last_active_slot(slot);

	// ----------------

	let position_index = user.force_get_vault_position_index(vault_index)?;

	// let is_borrow_before = user.vault_positions[position_index].is_borrow();

	let force_reduce_only = market.is_reduce_only();

	// if reduce only, have to compare ix amount to current borrow amount
	let amount = if
		(force_reduce_only || reduce_only) &&
		user.spot_positions[position_index].balance_type == SpotBalanceType::Borrow
	{
		user.spot_positions[position_index]
			.get_token_amount(&spot_market)?
			.cast::<u64>()?
			.min(amount)
	} else {
		amount
	};

	user.increment_total_deposits(
		amount,
		oracle_price_data.price,
		market.get_precision().cast()?
	)?;

	let total_deposits_after = user.total_deposits;
	let total_withdraws_after = user.total_withdraws;

	let vault_position = &mut user.vault_positions[position_index];
	controller::spot_position::update_spot_balances_and_cumulative_deposits(
		amount as u128,
		&SpotBalanceType::Deposit,
		&mut spot_market,
		vault_position,
		false,
		None
	)?;

	let token_amount = vault_position.get_token_amount(&spot_market)?;
	if token_amount == 0 {
		validate!(
			vault_position.scaled_balance == 0,
			ErrorCode::InvalidSpotPosition,
			"deposit left user with invalid position. scaled balance = {} token amount = {}",
			vault_position.scaled_balance,
			token_amount
		)?;
	}

	if
		vault_position.balance_type == SpotBalanceType::Deposit &&
		vault_position.scaled_balance > 0
	{
		validate!(
			matches!(spot_market.status, MarketStatus::Active),
			ErrorCode::MarketActionPaused,
			"spot_market not active"
		)?;
	}

	drop(market);
	if user.is_being_liquidated() {
		// try to update liquidation status if user is was already being liq'd
		let is_being_liquidated = is_user_being_liquidated(
			user,
			&market_map,
			&vault_map,
			&mut oracle_map,
			state.liquidation_margin_buffer_ratio
		)?;

		if !is_being_liquidated {
			user.exit_liquidation();
		}
	}

	user.update_last_active_slot(slot);

	let market = &mut market_map.get_ref_mut(&vault.market_index)?;

	controller::token::receive(
		&ctx.accounts.token_program,
		&ctx.accounts.user_token_account,
		&ctx.accounts.market_vault,
		&ctx.accounts.authority,
		amount,
		&mint
	)?;
	ctx.accounts.market_vault.reload()?;

	let deposit_record_id = get_then_update_id!(market, next_deposit_record_id);
	let oracle_price = oracle_price_data.price;
	let explanation = DepositExplanation::None;
	// let explanation = if is_borrow_before {
	// 	DepositExplanation::RepayBorrow
	// } else {
	// 	DepositExplanation::None
	// };
	let deposit_record = DepositRecord {
		ts: now,
		deposit_record_id,
		user_authority: user.authority,
		user: user_key,
		direction: DepositDirection::Deposit,
		amount,
		oracle_price,
		market_deposit_balance: market.deposit_balance,
		market_withdraw_balance: market.borrow_balance,
		market_cumulative_deposit_interest: market.cumulative_deposit_interest,
		market_cumulative_borrow_interest: market.cumulative_borrow_interest,
		total_deposits_after,
		total_withdraws_after,
		market_index,
		explanation,
		transfer_user: None,
	};
	emit!(deposit_record);

	market.validate_max_token_deposits_and_borrows(false)?;

	Ok(())
}
