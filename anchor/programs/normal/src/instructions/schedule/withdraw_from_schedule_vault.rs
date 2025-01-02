use anchor_lang::prelude::*;

use crate::state::{ user::User, user_stats::UserStats };

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct WithdrawFromScheduleVault<'info> {
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
        seeds = [b"user_vault".as_ref()],
        bump,
    )]
	pub user_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(constraint = state.signer.eq(&drift_signer.key()))]
	/// CHECK: forced drift_signer
	pub drift_signer: AccountInfo<'info>,
	#[account(
        mut,
        constraint = &user_vault.mint.eq(&user_token_account.mint)
    )]
	pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

#[access_control(withdraw_not_paused(&ctx.accounts.state))]
pub fn handle_withdraw_from_schedule_vault<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, WithdrawFromScheduleVault<'info>>,
	amount: u64
) -> anchor_lang::Result<()> {
	let user_key = ctx.accounts.user.key();
	let user = &mut load_mut!(ctx.accounts.user)?;
	let mut user_stats = load_mut!(ctx.accounts.user_stats)?;
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let slot = clock.slot;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { perp_market_map, spot_market_map, mut oracle_map } =
		load_maps(
			remaining_accounts_iter,
			&MarketSet::new(),
			&get_writable_spot_market_set(market_index),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(!user.is_bankrupt(), ErrorCode::UserBankrupt)?;

	let spot_market_is_reduce_only = {
		let spot_market = &mut spot_market_map.get_ref_mut(&market_index)?;
		let oracle_price_data = oracle_map.get_price_data(&spot_market.oracle)?;

		controller::spot_balance::update_spot_market_cumulative_interest(
			spot_market,
			Some(oracle_price_data),
			now
		)?;

		spot_market.is_reduce_only()
	};

	let amount = {
		let reduce_only = reduce_only || spot_market_is_reduce_only;

		let position_index = user.force_get_spot_position_index(market_index)?;

		let mut amount = if reduce_only {
			validate!(
				user.spot_positions[position_index].balance_type ==
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

			let spot_market = &spot_market_map.get_ref(&market_index)?;
			let existing_deposit_amount = user.spot_positions[position_index]
				.get_token_amount(spot_market)?
				.cast::<u64>()?;

			amount.min(max_withdrawable_amount).min(existing_deposit_amount)
		} else {
			amount
		};

		let spot_market = &mut spot_market_map.get_ref_mut(&market_index)?;
		let oracle_price_data = oracle_map.get_price_data(&spot_market.oracle)?;

		if user.qualifies_for_withdraw_fee(&user_stats, slot) {
			let fee = charge_withdraw_fee(
				spot_market,
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


	if user.is_being_liquidated() {
		user.exit_liquidation();
	}

	user.update_last_active_slot(slot);

	let mut spot_market = spot_market_map.get_ref_mut(&market_index)?;
	let oracle_price = oracle_map.get_price_data(&spot_market.oracle)?.price;

	let is_borrow = user
		.get_spot_position(market_index)
		.map_or(false, |pos| pos.is_borrow());
	let deposit_explanation = if is_borrow {
		DepositExplanation::Borrow
	} else {
		DepositExplanation::None
	};

	let deposit_record_id = get_then_update_id!(
		spot_market,
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
		market_deposit_balance: spot_market.deposit_balance,
		market_withdraw_balance: spot_market.borrow_balance,
		market_cumulative_deposit_interest: spot_market.cumulative_deposit_interest,
		market_cumulative_borrow_interest: spot_market.cumulative_borrow_interest,
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
		&ctx.accounts.drift_signer,
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
