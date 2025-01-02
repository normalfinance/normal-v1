use amm::AMM;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };
use anchor_spl::token_interface::TokenAccount as TokenAccountInterface;
use vault_map::get_writable_vault_set;

use crate::errors::ErrorCode;
use crate::manager::liquidity_manager::{
	calculate_liquidity_token_deltas,
	calculate_modify_liquidity,
	sync_modify_liquidity_values,
};
use crate::math::{ self, convert_to_liquidity_delta };
use crate::{ controller, state::* };
use crate::util::{
	mint_synthetic_to_vault,
	to_timestamp_u64,
	transfer_from_owner_to_vault,
	verify_position_authority_interface,
};

#[derive(Accounts)]
#[instruction(vault_index: u16,)]
pub struct WithdrawCollateral<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        has_one = authority,
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        has_one = authority
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        seeds = [b"spot_market_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub spot_market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        constraint = &spot_market_vault.mint.eq(&user_token_account.mint)
    )]
	pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

#[access_control(withdraw_not_paused(&ctx.accounts.state))]
pub fn handle_withdraw_collateral<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, WithdrawCollateral<'info>>,
	market_index: u16,
	amount: u64,
	reduce_only: bool
) -> anchor_lang::Result<()> {
	let user_key = ctx.accounts.user.key();
	let user = &mut load_mut!(ctx.accounts.user)?;
	let mut user_stats = load_mut!(ctx.accounts.user_stats)?;
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let slot = clock.slot;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { market_map, vault_map, mut oracle_map } =
		load_maps(
			remaining_accounts_iter,
			&MarketSet::new(),
			&get_writable_vault_set(vault_index),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(!user.is_bankrupt(), ErrorCode::UserBankrupt)?;

	let market_is_reduce_only = {
		let market = &mut market_map.get_ref_mut(&market_index)?;
		let oracle_price_data = oracle_map.get_price_data(&market.oracle)?;

		controller::spot_balance::update_synth_market_cumulative_interest(
			market,
			Some(oracle_price_data),
			now
		)?;

		market.is_reduce_only()
	};

	let amount = {
		let reduce_only = reduce_only || market_is_reduce_only;

		let position_index = user.force_get_vault_position_index(vault_index)?;

		let mut amount = if reduce_only {
			validate!(
				user.vault_positions[position_index].balance_type ==
					SpotBalanceType::Deposit,
				ErrorCode::ReduceOnlyWithdrawIncreasedRisk
			)?;

			let max_withdrawable_amount = calculate_max_withdrawable_amount(
				market_index,
				user,
				&perp_market_map,
				&spot_market_map,
				&mut oracle_map
			)?;

			let market = &market_map.get_ref(&market_index)?;
			let existing_deposit_amount = user.vault_positions[position_index]
				.get_token_amount(market)?
				.cast::<u64>()?;

			amount.min(max_withdrawable_amount).min(existing_deposit_amount)
		} else {
			amount
		};

		let market = &mut market_map.get_ref_mut(&market_index)?;
		let oracle_price_data = oracle_map.get_price_data(&market.oracle)?;

		if user.qualifies_for_withdraw_fee(&user_stats, slot) {
			let fee = charge_withdraw_fee(
				market,
				oracle_price_data.price,
				user,
				&mut user_stats
			)?;
			amount = amount.safe_sub(fee.cast()?)?;
		}

		user.increment_total_withdraws(
			amount,
			oracle_price_data.price,
			spot_market.get_precision().cast()?
		)?;

		// prevents withdraw when limits hit
		controller::spot_position::update_spot_balances_and_cumulative_deposits_with_limits(
			amount as u128,
			&SpotBalanceType::Borrow,
			spot_market,
			user
		)?;

		amount
	};

	user.meets_withdraw_margin_requirement(
		&market_map,
		&vault_map,
		&mut oracle_map,
		MarginRequirementType::Initial,
		market_index,
		amount as u128,
		&mut user_stats,
		now
	)?;

	if user.is_being_liquidated() {
		user.exit_liquidation();
	}

	user.update_last_active_slot(slot);

	let mut market = market_map.get_ref_mut(&market_index)?;
	let oracle_price = oracle_map.get_price_data(&market.oracle)?.price;

	// let is_borrow = user
	// 	.get_spot_position(market_index)
	// 	.map_or(false, |pos| pos.is_borrow());
	let deposit_explanation = DepositExplanation::None;
	// let deposit_explanation = if is_borrow {
	// 	DepositExplanation::Borrow
	// } else {
	// 	DepositExplanation::None
	// };

	let deposit_record_id = get_then_update_id!(
		market,
		next_deposit_record_id
	);
	let deposit_record = DepositRecord {
		ts: now,
		deposit_record_id,
		user_authority: user.authority,
		user: user_key,
		direction: DepositDirection::Withdraw,
		oracle_price,
		amount,
		market_index,
		market_deposit_balance: market.deposit_balance,
		market_withdraw_balance: market.borrow_balance,
		market_cumulative_deposit_interest: market.cumulative_deposit_interest,
		market_cumulative_borrow_interest: market.cumulative_borrow_interest,
		total_deposits_after: user.total_deposits,
		total_withdraws_after: user.total_withdraws,
		explanation: deposit_explanation,
		transfer_user: None,
	};
	emit!(deposit_record);

	controller::token::send_from_program_vault(
		&ctx.accounts.token_program,
		&ctx.accounts.spot_market_vault,
		&ctx.accounts.user_token_account,
		&ctx.accounts.normal_signer,
		state.signer_nonce,
		amount,
		&mint
	)?;

	// reload the spot market vault balance so it's up-to-date
	ctx.accounts.spot_market_vault.reload()?;
	math::spot_withdraw::validate_spot_market_vault_amount(
		&spot_market,
		ctx.accounts.spot_market_vault.amount
	)?;

	spot_market.validate_max_token_deposits_and_borrows(is_borrow)?;

	Ok(())
}
