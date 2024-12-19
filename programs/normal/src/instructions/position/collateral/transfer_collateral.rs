use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };
use anchor_spl::token_interface::TokenAccount as TokenAccountInterface;
use events::DepositRecord;
use user::{ User, UserStats };
use vault_map::get_writable_vault_set;

use crate::errors::ErrorCode;
use crate::instructions::optional_accounts::{ load_maps, AccountMaps };
use crate::{ controller, load_mut, state::*, State };

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct TransferCollateral<'info> {
	#[account(
        mut,
        has_one = authority,
    )]
	pub from_user: AccountLoader<'info, User>,
	#[account(
        mut,
        has_one = authority,
    )]
	pub to_user: AccountLoader<'info, User>,
	#[account(
        mut,
        has_one = authority
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	pub state: Box<Account<'info, State>>,
	#[account(
		seeds = [
			b"synth_market_vault".as_ref(),
			market_index.to_le_bytes().as_ref(),
		],
		bump
	)]
	pub synth_market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
}

#[access_control(
    deposit_not_paused(&ctx.accounts.state)
    withdraw_not_paused(&ctx.accounts.state)
)]
pub fn handle_transfer_collateral<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, TransferCollateral<'info>>,
	market_index: u16,
	amount: u64
) -> Result<()> {
	let authority_key = ctx.accounts.authority.key;
	let to_user_key = ctx.accounts.to_user.key();
	let from_user_key = ctx.accounts.from_user.key();

	let state = &ctx.accounts.state;
	let clock = Clock::get()?;
	let slot = clock.slot;

	let to_user = &mut load_mut!(ctx.accounts.to_user)?;
	let from_user = &mut load_mut!(ctx.accounts.from_user)?;
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	validate!(
		!to_user.is_bankrupt(),
		ErrorCode::UserBankrupt,
		"to_user bankrupt"
	)?;
	validate!(
		!from_user.is_bankrupt(),
		ErrorCode::UserBankrupt,
		"from_user bankrupt"
	)?;

	validate!(
		from_user_key != to_user_key,
		ErrorCode::CantTransferBetweenSameUserAccount,
		"cant transfer between the same user account"
	)?;

	let AccountMaps { synth_market_map, index_market_map, mut oracle_map } =
		load_maps(
			&mut ctx.remaining_accounts.iter().peekable(),
			&MarketSet::new(),
			&get_writable_vault_set(vault_index),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	{
		let synth_market = &mut synth_market_map.get_ref_mut(&market_index)?;
		let oracle_price_data = oracle_map.get_price_data(&synth_market.oracle)?;
		controller::synth_balance::update_synth_market_cumulative_interest(
			synth_market,
			Some(oracle_price_data),
			clock.unix_timestamp
		)?;
	}

	let oracle_price = {
		let synth_market = &market_map.get_ref(&market_index)?;
		oracle_map.get_price_data(&synth_market.oracle)?.price
	};

	{
		let market = &mut market_map.get_ref_mut(&market_index)?;

		from_user.increment_total_withdraws(
			amount,
			oracle_price,
			synth_market.get_precision().cast()?
		)?;

		// prevents withdraw when limits hit
		controller::spot_position::update_spot_balances_and_cumulative_deposits_with_limits(
			amount as u128,
			&SpotBalanceType::Borrow,
			synth_market,
			from_user
		)?;
	}

	from_user.meets_withdraw_margin_requirement(
		&market_map,
		&vault_map,
		&mut oracle_map,
		MarginRequirementType::Initial,
		market_index,
		amount as u128,
		user_stats,
		now
	)?;

	if from_user.is_being_liquidated() {
		from_user.exit_liquidation();
	}

	from_user.update_last_active_slot(slot);

	{
		let market = &mut market_map.get_ref_mut(&market_index)?;

		let deposit_record_id = get_then_update_id!(market, next_deposit_record_id);
		let deposit_record = DepositRecord {
			ts: clock.unix_timestamp,
			deposit_record_id,
			user_authority: *authority_key,
			user: from_user_key,
			direction: DepositDirection::Withdraw,
			amount,
			oracle_price,
			market_index,
			market_deposit_balance: market.deposit_balance,
			market_withdraw_balance: market.borrow_balance,
			market_cumulative_deposit_interest: market.cumulative_deposit_interest,
			market_cumulative_borrow_interest: market.cumulative_borrow_interest,
			total_deposits_after: from_user.total_deposits,
			total_withdraws_after: from_user.total_withdraws,
			explanation: DepositExplanation::Transfer,
			transfer_user: Some(to_user_key),
		};
		emit!(deposit_record);
	}

	{
		let spot_market = &mut spot_market_map.get_ref_mut(&market_index)?;

		to_user.increment_total_deposits(
			amount,
			oracle_price,
			spot_market.get_precision().cast()?
		)?;

		let total_deposits_after = to_user.total_deposits;
		let total_withdraws_after = to_user.total_withdraws;

		let to_spot_position = to_user.force_get_spot_position_mut(
			spot_market.market_index
		)?;

		controller::spot_position::update_spot_balances_and_cumulative_deposits(
			amount as u128,
			&SpotBalanceType::Deposit,
			spot_market,
			to_spot_position,
			false,
			None
		)?;

		let token_amount = to_spot_position.get_token_amount(spot_market)?;
		if token_amount == 0 {
			validate!(
				to_spot_position.scaled_balance == 0,
				ErrorCode::InvalidSpotPosition,
				"deposit left to_user with invalid position. scaled balance = {} token amount = {}",
				to_spot_position.scaled_balance,
				token_amount
			)?;
		}

		let deposit_record_id = get_then_update_id!(
			spot_market,
			next_deposit_record_id
		);
		let deposit_record = DepositRecord {
			ts: clock.unix_timestamp,
			deposit_record_id,
			user_authority: *authority_key,
			user: to_user_key,
			direction: DepositDirection::Deposit,
			amount,
			oracle_price,
			market_index,
			market_deposit_balance: spot_market.deposit_balance,
			market_withdraw_balance: spot_market.borrow_balance,
			market_cumulative_deposit_interest: spot_market.cumulative_deposit_interest,
			market_cumulative_borrow_interest: spot_market.cumulative_borrow_interest,
			total_deposits_after,
			total_withdraws_after,
			explanation: DepositExplanation::Transfer,
			transfer_user: Some(from_user_key),
		};
		emit!(deposit_record);
	}

	to_user.update_last_active_slot(slot);

	let synth_market = synth_market_map.get_ref(&market_index)?;
	math::synth_withdraw::validate_synth_market_vault_amount(
		&synth_market,
		ctx.accounts.synth_market_vault.amount
	)?;

	Ok(())
}
