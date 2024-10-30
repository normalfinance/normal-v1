use anchor_lang::prelude::*;
use anchor_spl::token_interface::{ Mint, TokenAccount, TokenInterface };
use solana_program::msg;

use crate::controller::balance::{ update_fee_pool_balances, update_balances };
use crate::controller::token::send_from_program_vault;
use crate::error::NormalResult;
use crate::error::ErrorCode;
use crate::math::amm::calculate_net_user_pnl;
use crate::math::casting::Cast;
use crate::constants::constants::{
	ONE_YEAR,
	PERCENTAGE_PRECISION,
	SHARE_OF_REVENUE_ALLOCATED_TO_INSURANCE_FUND_VAULT_DENOMINATOR,
	SHARE_OF_REVENUE_ALLOCATED_TO_INSURANCE_FUND_VAULT_NUMERATOR,
	INSURANCE_A_MAX,
	INSURANCE_B_MAX,
	INSURANCE_C_MAX,
	INSURANCE_SPECULATIVE_MAX,
};
use crate::math::helpers::get_proportion_u128;
use crate::math::helpers::on_the_hour_update;
use crate::math::insurance::{
	calculate_if_shares_lost,
	calculate_rebase_info,
	if_shares_to_vault_amount,
	vault_amount_to_if_shares,
};
use crate::math::safe_math::SafeMath;
use crate::math::balance::get_token_amount;
use crate::state::events::{
	InsuranceFundRecord,
	InsuranceFundStakeRecord,
	StakeAction,
};
use crate::state::insurance::{ InsuranceFund, InsuranceFundStake };
use crate::state::market::{ BalanceType, Market };
use crate::state::market_map::MarketMap;
use crate::state::state::State;
use crate::state::user::UserStats;
use crate::{ emit, validate };

// #[cfg(test)]
// mod tests;

pub fn update_market_max_insurance_claim(
	market_map: &MarketMap
) -> NormalResult {
	// Max insurance claim is a function of SyntheticTier and 7d rolling volume

	let max_insurance_for_tier = match market.synthetic_tier {
		SyntheticTier::A => INSURANCE_A_MAX,
		SyntheticTier::B => INSURANCE_B_MAX,
		SyntheticTier::C => INSURANCE_C_MAX,
		SyntheticTier::Speculative => INSURANCE_SPECULATIVE_MAX,
		SyntheticTier::HighlySpeculative => INSURANCE_SPECULATIVE_MAX,
		SyntheticTier::Isolated => INSURANCE_SPECULATIVE_MAX,
	};

	let total_adjusted_volume = 0;
	for (_key, market_account_loader) in market_map.0.iter_mut() {
		let market = &mut load_mut!(market_account_loader)?;

		adjusted_volume = market.amm.volume24h.safe_mul(market.tier);
		total_adjusted_volume = total_adjusted_volume.safe_add(adjusted_volume);

		new_max_insurance_claim_pct = market.amm.volume24h.safe_mul(
			max_insurance_for_tier(market.tier)
		);
		// TODO: validations
		market.max_insurance_claim_pct = new_max_insurance_claim_pct;
	}

	Ok(())
}

pub fn update_user_stats_if_stake_amount(
	if_stake_amount_delta: i64,
	insurance_vault_amount: u64,
	insurance_fund_stake: &mut InsuranceFundStake,
	user_stats: &mut UserStats,
	insurance_fund: &mut InsuranceFund,
	now: i64
) -> NormalResult {
	let if_stake_amount = if if_stake_amount_delta >= 0 {
		if_shares_to_vault_amount(
			insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?,
			insurance_fund.total_shares,
			insurance_vault_amount.safe_add(if_stake_amount_delta.unsigned_abs())?
		)?
	} else {
		if_shares_to_vault_amount(
			insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?,
			insurance_fund.total_shares,
			insurance_vault_amount.safe_sub(if_stake_amount_delta.unsigned_abs())?
		)?
	};

	user_stats.insurance_fund_staked_amount = if_stake_amount;

	Ok(())
}

pub fn add_insurance_fund_stake(
	amount: u64,
	insurance_vault_amount: u64,
	insurance_fund_stake: &mut InsuranceFundStake,
	user_stats: &mut UserStats,
	insurance_fund: &mut InsuranceFund,
	now: i64
) -> NormalResult {
	validate!(
		!(insurance_vault_amount == 0 && insurance_fund.total_shares != 0),
		ErrorCode::InvalidIFForNewStakes,
		"Insurance Fund balance should be non-zero for new stakers to enter"
	)?;

	apply_rebase_to_insurance_fund(insurance_vault_amount, insurance_fund)?;
	apply_rebase_to_insurance_fund_stake(insurance_fund_stake, insurance_fund)?;

	let if_shares_before =
		insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;
	let total_if_shares_before = insurance_fund.total_shares;
	let user_if_shares_before = insurance_fund.user_shares;

	let n_shares = vault_amount_to_if_shares(
		amount,
		insurance_fund.total_shares,
		insurance_vault_amount
	)?;

	// reset cost basis if no shares
	insurance_fund_stake.cost_basis = if if_shares_before == 0 {
		amount.cast()?
	} else {
		insurance_fund_stake.cost_basis.safe_add(amount.cast()?)?
	};

	insurance_fund_stake.increase_insurance_fund_shares(
		n_shares,
		insurance_fund
	)?;

	insurance_fund.total_shares = insurance_fund.total_shares.safe_add(n_shares)?;

	insurance_fund.user_shares = insurance_fund.user_shares.safe_add(n_shares)?;

	update_user_stats_if_stake_amount(
		amount.cast()?,
		insurance_vault_amount,
		insurance_fund_stake,
		user_stats,
		insurance_fund,
		now
	)?;

	let if_shares_after =
		insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;

	emit!(InsuranceFundStakeRecord {
		ts: now,
		user_authority: user_stats.authority,
		action: StakeAction::Stake,
		amount,
		insurance_vault_amount_before: insurance_vault_amount,
		if_shares_before,
		user_if_shares_before,
		total_if_shares_before,
		if_shares_after,
		total_if_shares_after: insurance_fund.total_shares,
		user_if_shares_after: insurance_fund.user_shares,
	});

	Ok(())
}

pub fn apply_rebase_to_insurance_fund(
	insurance_fund_vault_balance: u64,
	insurance_fund: &mut InsuranceFund
) -> NormalResult {
	if
		insurance_fund_vault_balance != 0 &&
		insurance_fund_vault_balance.cast::<u128>()? < insurance_fund.total_shares
	{
		let (expo_diff, rebase_divisor) = calculate_rebase_info(
			insurance_fund.total_shares,
			insurance_fund_vault_balance
		)?;

		insurance_fund.total_shares =
			insurance_fund.total_shares.safe_div(rebase_divisor)?;
		insurance_fund.user_shares =
			insurance_fund.user_shares.safe_div(rebase_divisor)?;
		insurance_fund.shares_base = insurance_fund.shares_base.safe_add(
			expo_diff.cast::<u128>()?
		)?;

		msg!("rebasing insurance fund: expo_diff={}", expo_diff);
	}

	if insurance_fund_vault_balance != 0 && insurance_fund.total_shares == 0 {
		insurance_fund.total_shares = insurance_fund_vault_balance.cast::<u128>()?;
	}

	Ok(())
}

pub fn apply_rebase_to_insurance_fund_stake(
	insurance_fund_stake: &mut InsuranceFundStake,
	insurance_fund: &mut InsuranceFund
) -> NormalResult {
	if insurance_fund.shares_base != insurance_fund_stake.if_base {
		validate!(
			insurance_fund.shares_base > insurance_fund_stake.if_base,
			ErrorCode::InvalidIFRebase,
			"Rebase expo out of bounds"
		)?;

		let expo_diff = (
			insurance_fund.shares_base - insurance_fund_stake.if_base
		).cast::<u32>()?;

		let rebase_divisor = (10_u128).pow(expo_diff);

		msg!(
			"rebasing insurance fund stake: base: {} -> {} ",
			insurance_fund_stake.if_base,
			insurance_fund.shares_base
		);

		insurance_fund_stake.if_base = insurance_fund.shares_base;

		let old_if_shares = insurance_fund_stake.unchecked_insurance_fund_shares();
		let new_if_shares = old_if_shares.safe_div(rebase_divisor)?;

		msg!("rebasing insurance fund stake: shares -> {} ", new_if_shares);

		insurance_fund_stake.update_insurance_fund_shares(
			new_if_shares,
			insurance_fund
		)?;

		insurance_fund_stake.last_withdraw_request_shares =
			insurance_fund_stake.last_withdraw_request_shares.safe_div(
				rebase_divisor
			)?;
	}

	Ok(())
}

pub fn request_remove_insurance_fund_stake(
	n_shares: u128,
	insurance_vault_amount: u64,
	insurance_fund_stake: &mut InsuranceFundStake,
	user_stats: &mut UserStats,
	insurance_fund: &mut InsuranceFund,
	now: i64
) -> NormalResult {
	msg!("n_shares {}", n_shares);
	insurance_fund_stake.last_withdraw_request_shares = n_shares;

	apply_rebase_to_insurance_fund(insurance_vault_amount, insurance_fund)?;
	apply_rebase_to_insurance_fund_stake(insurance_fund_stake, insurance_fund)?;

	let if_shares_before =
		insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;
	let total_if_shares_before = insurance_fund.total_shares;
	let user_if_shares_before = insurance_fund.user_shares;

	validate!(
		insurance_fund_stake.last_withdraw_request_shares <=
			insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?,
		ErrorCode::InvalidInsuranceUnstakeSize,
		"last_withdraw_request_shares exceeds if_shares {} > {}",
		insurance_fund_stake.last_withdraw_request_shares,
		insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?
	)?;

	validate!(
		insurance_fund_stake.if_base == insurance_fund.shares_base,
		ErrorCode::InvalidIFRebase,
		"if stake base != market base"
	)?;

	insurance_fund_stake.last_withdraw_request_value = if_shares_to_vault_amount(
		insurance_fund_stake.last_withdraw_request_shares,
		insurance_fund.total_shares,
		insurance_vault_amount
	)?.min(insurance_vault_amount.saturating_sub(1));

	validate!(
		insurance_fund_stake.last_withdraw_request_value == 0 ||
			insurance_fund_stake.last_withdraw_request_value < insurance_vault_amount,
		ErrorCode::InvalidIFUnstakeSize,
		"Requested withdraw value is not below Insurance Fund balance"
	)?;

	let if_shares_after =
		insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;

	update_user_stats_if_stake_amount(
		0,
		insurance_vault_amount,
		insurance_fund_stake,
		user_stats,
		market,
		now
	)?;

	emit!(InsuranceFundStakeRecord {
		ts: now,
		user_authority: user_stats.authority,
		action: StakeAction::UnstakeRequest,
		amount: insurance_fund_stake.last_withdraw_request_value,
		market_index: market.market_index,
		insurance_vault_amount_before: insurance_vault_amount,
		if_shares_before,
		user_if_shares_before,
		total_if_shares_before,
		if_shares_after,
		total_if_shares_after: insurance_fund.total_shares,
		user_if_shares_after: insurance_fund.user_shares,
	});

	insurance_fund_stake.last_withdraw_request_ts = now;

	Ok(())
}

pub fn cancel_request_remove_insurance_fund_stake(
	insurance_vault_amount: u64,
	insurance_fund_stake: &mut InsuranceFundStake,
	user_stats: &mut UserStats,
	insurance_fund: &mut InsuranceFund,
	now: i64
) -> NormalResult {
	apply_rebase_to_insurance_fund(insurance_vault_amount, insurance_fund)?;
	apply_rebase_to_insurance_fund_stake(insurance_fund_stake, insurance_fund)?;

	let if_shares_before =
		insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;
	let total_if_shares_before = insurance_fund.total_shares;
	let user_if_shares_before = insurance_fund.user_shares;

	validate!(
		insurance_fund_stake.insurance_fund_base == insurance_fund.shares_base,
		ErrorCode::InvalidIFRebase,
		"if stake base != insurance fund base"
	)?;

	validate!(
		insurance_fund_stake.last_withdraw_request_shares != 0,
		ErrorCode::InvalidIFUnstakeCancel,
		"No withdraw request in progress"
	)?;

	let if_shares_lost = calculate_if_shares_lost(
		insurance_fund_stake,
		insurance_fund,
		insurance_vault_amount
	)?;

	insurance_fund_stake.decrease_insurance_fund_shares(
		if_shares_lost,
		insurance_fund
	)?;

	insurance_fund.total_shares =
		insurance_fund.total_shares.safe_sub(if_shares_lost)?;

	insurance_fund.user_shares =
		insurance_fund.user_shares.safe_sub(if_shares_lost)?;

	let if_shares_after =
		insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;

	update_user_stats_if_stake_amount(
		0,
		insurance_vault_amount,
		insurance_fund_stake,
		user_stats,
		insurance_fund,
		now
	)?;

	emit!(InsuranceFundStakeRecord {
		ts: now,
		user_authority: user_stats.authority,
		action: StakeAction::UnstakeCancelRequest,
		amount: 0,
		insurance_vault_amount_before: insurance_vault_amount,
		if_shares_before,
		user_if_shares_before,
		total_if_shares_before,
		if_shares_after,
		total_if_shares_after: insurance_fund.total_shares,
		user_if_shares_after: insurance_fund.user_shares,
	});

	insurance_fund_stake.last_withdraw_request_shares = 0;
	insurance_fund_stake.last_withdraw_request_value = 0;
	insurance_fund_stake.last_withdraw_request_ts = now;

	Ok(())
}

pub fn remove_insurance_fund_stake(
	insurance_vault_amount: u64,
	insurance_fund_stake: &mut InsuranceFundStake,
	user_stats: &mut UserStats,
	insurance_fund: &mut InsuranceFund,
	now: i64
) -> NormalResult<u64> {
	let time_since_withdraw_request = now.safe_sub(
		insurance_fund_stake.last_withdraw_request_ts
	)?;

	validate!(
		time_since_withdraw_request >= insurance_fund.unstaking_period,
		ErrorCode::TryingToRemoveLiquidityTooFast
	)?;

	apply_rebase_to_insurance_fund(insurance_vault_amount, insurance_fund)?;
	apply_rebase_to_insurance_fund_stake(insurance_fund_stake, insurance_fund)?;

	let if_shares_before =
		insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;
	let total_if_shares_before = insurance_fund.total_shares;
	let user_if_shares_before = insurance_fund.user_shares;

	let n_shares = insurance_fund_stake.last_withdraw_request_shares;

	validate!(
		n_shares > 0,
		ErrorCode::InvalidIFUnstake,
		"Must submit withdraw request and wait the escrow period"
	)?;

	validate!(if_shares_before >= n_shares, ErrorCode::InsufficientIFShares)?;

	let amount = if_shares_to_vault_amount(
		n_shares,
		insurance_fund.total_shares,
		insurance_vault_amount
	)?;

	let _if_shares_lost = calculate_if_shares_lost(
		insurance_fund_stake,
		insurance_fund,
		insurance_vault_amount
	)?;

	let withdraw_amount = amount.min(
		insurance_fund_stake.last_withdraw_request_value
	);

	insurance_fund_stake.decrease_insurance_fund_shares(
		n_shares,
		insurance_fund
	)?;

	insurance_fund_stake.cost_basis = insurance_fund_stake.cost_basis.safe_sub(
		withdraw_amount.cast()?
	)?;

	insurance_fund.total_shares = insurance_fund.total_shares.safe_sub(n_shares)?;

	insurance_fund.user_shares = insurance_fund.user_shares.safe_sub(n_shares)?;

	// reset insurance_fund_stake withdraw request info
	insurance_fund_stake.last_withdraw_request_shares = 0;
	insurance_fund_stake.last_withdraw_request_value = 0;
	insurance_fund_stake.last_withdraw_request_ts = now;

	let if_shares_after =
		insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;

	update_user_stats_if_stake_amount(
		-withdraw_amount.cast()?,
		insurance_vault_amount,
		insurance_fund_stake,
		user_stats,
		insurance_fund,
		now
	)?;

	emit!(InsuranceFundStakeRecord {
		ts: now,
		user_authority: user_stats.authority,
		action: StakeAction::Unstake,
		amount: withdraw_amount,
		insurance_vault_amount_before: insurance_vault_amount,
		if_shares_before,
		user_if_shares_before,
		total_if_shares_before,
		if_shares_after,
		total_if_shares_after: insurance_fund.total_shares,
		user_if_shares_after: insurance_fund.user_shares,
	});

	Ok(withdraw_amount)
}

pub fn admin_remove_insurance_fund_stake(
	insurance_vault_amount: u64,
	n_shares: u128,
	insurance_fund: &mut InsuranceFund,
	now: i64,
	admin_pubkey: Pubkey
) -> NormalResult<u64> {
	apply_rebase_to_insurance_fund(insurance_vault_amount, insurance_fund)?;

	let total_if_shares_before = insurance_fund.total_shares;
	let user_if_shares_before = insurance_fund.user_shares;

	let if_shares_before = total_if_shares_before.safe_sub(
		user_if_shares_before
	)?;

	validate!(
		if_shares_before >= n_shares,
		ErrorCode::InsufficientIFShares,
		"if_shares_before={} < n_shares={}",
		if_shares_before,
		n_shares
	)?;

	let withdraw_amount = if_shares_to_vault_amount(
		n_shares,
		insurance_fund.total_shares,
		insurance_vault_amount
	)?;

	insurance_fund.total_shares = insurance_fund.total_shares.safe_sub(n_shares)?;

	let if_shares_after = insurance_fund.total_shares.safe_sub(
		user_if_shares_before
	)?;

	emit!(InsuranceFundStakeRecord {
		ts: now,
		user_authority: admin_pubkey,
		action: StakeAction::Unstake,
		amount: withdraw_amount,
		insurance_vault_amount_before: insurance_vault_amount,
		if_shares_before,
		user_if_shares_before,
		total_if_shares_before,
		if_shares_after,
		total_if_shares_after: insurance_fund.total_shares,
		user_if_shares_after: insurance_fund.user_shares,
	});

	Ok(withdraw_amount)
}

pub fn transfer_protocol_insurance_fund_stake(
	insurance_vault_amount: u64,
	n_shares: u128,
	target_insurance_fund_stake: &mut InsuranceFundStake,
	user_stats: &mut UserStats,
	insurance_fund: &mut InsuranceFund,
	now: i64,
	signer_pubkey: Pubkey
) -> NormalResult<u64> {
	apply_rebase_to_insurance_fund(insurance_vault_amount, insurance_fund)?;

	let total_if_shares_before = insurance_fund.total_shares;
	let user_if_shares_before = insurance_fund.user_shares;

	let if_shares_before = total_if_shares_before.safe_sub(
		user_if_shares_before
	)?;
	let target_if_shares_before =
		target_insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;
	validate!(
		if_shares_before >= n_shares,
		ErrorCode::InsufficientIFShares,
		"if_shares_before={} < n_shares={}",
		if_shares_before,
		n_shares
	)?;

	insurance_fund.user_shares = insurance_fund.user_shares.safe_add(n_shares)?;

	target_insurance_fund_stake.increase_insurance_fund_shares(
		n_shares,
		insurance_fund
	)?;

	let target_if_shares_after =
		target_insurance_fund_stake.checked_insurance_fund_shares(insurance_fund)?;

	user_stats.insurance_fund_staked_amount = if_shares_to_vault_amount(
		target_if_shares_after,
		insurance_fund.total_shares,
		insurance_vault_amount
	)?;

	let withdraw_amount = if_shares_to_vault_amount(
		n_shares,
		insurance_fund.total_shares,
		insurance_vault_amount
	)?;
	let user_if_shares_after = insurance_fund.user_shares;

	let protocol_if_shares_after =
		insurance_fund.total_shares.safe_sub(user_if_shares_after)?;

	emit!(InsuranceFundStakeRecord {
		ts: now,
		user_authority: signer_pubkey,
		action: StakeAction::UnstakeTransfer,
		amount: withdraw_amount,
		insurance_vault_amount_before: insurance_vault_amount,
		if_shares_before,
		user_if_shares_before,
		total_if_shares_before,
		if_shares_after: protocol_if_shares_after,
		total_if_shares_after: insurance_fund.total_shares,
		user_if_shares_after: insurance_fund.user_shares,
	});

	emit!(InsuranceFundStakeRecord {
		ts: now,
		user_authority: target_insurance_fund_stake.authority,
		action: StakeAction::StakeTransfer,
		amount: withdraw_amount,
		insurance_vault_amount_before: insurance_vault_amount,
		if_shares_before: target_if_shares_before,
		user_if_shares_before,
		total_if_shares_before,
		if_shares_after: target_insurance_fund_stake.checked_insurance_fund_shares(
			insurance_fund
		)?,
		total_if_shares_after: insurance_fund.total_shares,
		user_if_shares_after: insurance_fund.user_shares,
	});

	Ok(withdraw_amount)
}

pub fn attempt_transfer_fees_to_insurance_fund<'info>(
	market_vault: &InterfaceAccount<'info, TokenAccount>,
	insurance_fund_vault: &InterfaceAccount<'info, TokenAccount>,
	market: &mut Market,
	insurance_fund: &mut InsuranceFund,
	now: i64,
	token_program: &Interface<'info, TokenInterface>,
	normal_signer: &AccountInfo<'info>,
	state: &State,
	mint: &Option<InterfaceAccount<'info, Mint>>
) -> Result<()> {
	let _token_amount = {
		// uses proportion of revenue pool allocated to insurance fund
		let market_vault_amount = market_vault.amount;
		let insurance_fund_vault_amount = insurance_fund_vault.amount;

		let token_amount = transfer_fees_to_insurance_fund(
			market_vault_amount,
			insurance_fund_vault_amount,
			market,
			insurance_fund,
			now,
			false
		)?;

		if token_amount > 0 {
			msg!(
				"market_index={} sending {} to insurance_fund_vault",
				market.market_index,
				token_amount
			);

			send_from_program_vault(
				token_program,
				market_vault,
				insurance_fund_vault,
				normal_signer,
				state.signer_nonce,
				token_amount.cast()?,
				mint
			)?;
		}

		insurance_fund.last_revenue_settle_ts = now;

		token_amount
	};

	Ok(())
}

pub fn transfer_fees_to_insurance_fund(
	market_vault_amount: u64,
	insurance_vault_amount: u64,
	market: &mut Market,
	insurance_fund: &mut InsuranceFund,
	now: i64,
	check_invariants: bool
) -> NormalResult<u64> {
	validate!(
		insurance_fund.user_factor <= insurance_fund.total_factor,
		ErrorCode::RevenueSettingsCannotSettleToIF,
		"invalid if_factor settings on market"
	)?;

	let mut token_amount = get_token_amount(
		market.fee_pool.balance,
		market,
		&BalanceType::Deposit
	)?;

	let insurance_fund_token_amount = get_proportion_u128(
		token_amount,
		SHARE_OF_REVENUE_ALLOCATED_TO_INSURANCE_FUND_VAULT_NUMERATOR,
		SHARE_OF_REVENUE_ALLOCATED_TO_INSURANCE_FUND_VAULT_DENOMINATOR
	)?.cast::<u64>()?;

	if check_invariants {
		validate!(
			insurance_fund_token_amount != 0,
			ErrorCode::NoFeesToDepotiToInsuranceFund,
			"no amount to deposit to insurance fund"
		)?;
	}

	insurance_fund.last_fee_deposit_ts = now;

	let protocol_insurance_fund_factor = insurance_fund.total_factor.safe_sub(
		insurance_fund.user_factor
	)?;

	// give protocol its cut
	if protocol_insurance_fund_factor > 0 {
		let n_shares = vault_amount_to_if_shares(
			insurance_fund_token_amount
				.safe_mul(protocol_insurance_fund_factor.cast()?)?
				.safe_div(insurance_fund.total_factor.cast()?)?,
			insurance_fund.total_shares,
			insurance_vault_amount
		)?;

		insurance_fund.total_shares =
			insurance_fund.total_shares.safe_add(n_shares)?;
	}

	let total_if_shares_before = insurance_fund.total_shares;

	update_fee_pool_balances(
		insurance_fund_token_amount.cast::<u128>()?,
		&BalanceType::Borrow,
		market
	)?;

	emit!(InsuranceFundRecord {
		ts: now,
		market_index: market.market_index,
		market_index: 0, // todo: make option?
		amount: insurance_fund_token_amount.cast()?,

		user_insuranc_fund_factor: insurance_fund.user_factor,
		total_insuranc_fund_factor: insurance_fund.total_factor,
		vault_amount_before: market_vault_amount,
		insurance_vault_amount_before: insurance_vault_amount,
		total_insuranc_fund_shares_before,
		total_insuranc_fund_shares_after: insurance_fund.total_shares,
	});

	insurance_fund_token_amount.cast()
}

pub fn transfer_fees_to_treasury(
	market_vault_amount: u64,
	insurance_vault_amount: u64,
	market: &mut Market,
	insurance_fund: &mut InsuranceFund,
	now: i64,
	check_invariants: bool
) -> NormalResult<u64> {
	validate!(
		insurance_fund.user_factor <= insurance_fund.total_factor,
		ErrorCode::RevenueSettingsCannotSettleToIF,
		"invalid if_factor settings on market"
	)?;

	let mut token_amount = get_token_amount(
		market.fee_pool.balance,
		market,
		&BalanceType::Deposit
	)?;

	let insurance_fund_token_amount = get_proportion_u128(
		token_amount,
		SHARE_OF_REVENUE_ALLOCATED_TO_INSURANCE_FUND_VAULT_NUMERATOR,
		SHARE_OF_REVENUE_ALLOCATED_TO_INSURANCE_FUND_VAULT_DENOMINATOR
	)?.cast::<u64>()?;

	if check_invariants {
		validate!(
			insurance_fund_token_amount != 0,
			ErrorCode::NoFeesToDepotiToInsuranceFund,
			"no amount to deposit to insurance fund"
		)?;
	}

	insurance_fund.last_fee_deposit_ts = now;

	let protocol_insurance_fund_factor = insurance_fund.total_factor.safe_sub(
		insurance_fund.user_factor
	)?;

	// give protocol its cut
	if protocol_insurance_fund_factor > 0 {
		let n_shares = vault_amount_to_if_shares(
			insurance_fund_token_amount
				.safe_mul(protocol_insurance_fund_factor.cast()?)?
				.safe_div(insurance_fund.total_factor.cast()?)?,
			insurance_fund.total_shares,
			insurance_vault_amount
		)?;

		insurance_fund.total_shares =
			insurance_fund.total_shares.safe_add(n_shares)?;
	}

	let total_if_shares_before = insurance_fund.total_shares;

	update_fee_pool_balances(
		insurance_fund_token_amount.cast::<u128>()?,
		&BalanceType::Borrow,
		market
	)?;

	emit!(InsuranceFundRecord {
		ts: now,
		market_index: market.market_index,
		amount: insurance_fund_token_amount.cast()?,

		user_insuranc_fund_factor: insurance_fund.user_factor,
		total_insuranc_fund_factor: insurance_fund.total_factor,
		vault_amount_before: market_vault_amount,
		insurance_vault_amount_before: insurance_vault_amount,
		total_insuranc_fund_shares_before,
		total_insuranc_fund_shares_after: insurance_fund.total_shares,
	});

	insurance_fund_token_amount.cast()
}

pub fn resolve_pnl_deficit(
	vault_amount: u64,
	insurance_vault_amount: u64,
	market: &mut Market,
	insurance_fund: &mut InsuranceFund,
	now: i64
) -> NormalResult<u64> {
	validate!(
		market.amm.total_fee_minus_distributions < 0,
		ErrorCode::NoAmmPerpPnlDeficit,
		"market.amm.total_fee_minus_distributions={} must be negative",
		market.amm.total_fee_minus_distributions
	)?;

	let pnl_pool_token_amount = get_token_amount(
		market.pnl_pool.balance,
		market,
		&BalanceType::Deposit
	)?;

	validate!(
		pnl_pool_token_amount == 0,
		ErrorCode::SufficientPerpPnlPool,
		"pnl_pool_token_amount > 0 (={})",
		pnl_pool_token_amount
	)?;

	let total_if_shares_before = insurance_fund.total_shares;

	let max_insurance_withdraw = market.insurance_claim.quote_max_insurance
		.safe_sub(market.insurance_claim.quote_settled_insurance)?
		.cast::<i128>()?;

	validate!(
		max_insurance_withdraw > 0,
		ErrorCode::MaxIFWithdrawReached,
		"max_insurance_withdraw={}/{} as already been reached",
		market.insurance_claim.quote_settled_insurance,
		market.insurance_claim.quote_max_insurance
	)?;

	let insurance_withdraw = max_insurance_withdraw.min(
		insurance_vault_amount.saturating_sub(1).cast()?
	);

	validate!(
		insurance_withdraw > 0,
		ErrorCode::NoIFWithdrawAvailable,
		"No available funds for insurance_withdraw({}) for user_pnl_imbalance={}",
		insurance_withdraw,
		excess_user_pnl_imbalance
	)?;

	market.amm.total_fee_minus_distributions =
		market.amm.total_fee_minus_distributions.safe_add(insurance_withdraw)?;

	market.insurance_claim.revenue_withdraw_since_last_settle =
		market.insurance_claim.revenue_withdraw_since_last_settle.safe_add(
			insurance_withdraw.cast()?
		)?;

	market.insurance_claim.quote_settled_insurance =
		market.insurance_claim.quote_settled_insurance.safe_add(
			insurance_withdraw.cast()?
		)?;

	validate!(
		market.insurance_claim.quote_settled_insurance <=
			market.insurance_claim.quote_max_insurance,
		ErrorCode::MaxIFWithdrawReached,
		"quote_settled_insurance breached its max {}/{}",
		market.insurance_claim.quote_settled_insurance,
		market.insurance_claim.quote_max_insurance
	)?;

	market.insurance_claim.last_revenue_withdraw_ts = now;

	update_balances(
		insurance_withdraw.cast()?,
		&SpotBalanceType::Deposit,
		market,
		&mut market.pnl_pool,
		false
	)?;

	emit!(InsuranceFundRecord {
		ts: now,
		market_index: market.market_index,
		amount: -insurance_withdraw.cast()?,
		user_if_factor: insurance_fund.user_factor,
		total_if_factor: insurance_fund.total_factor,
		vault_amount_before: vault_amount,
		insurance_vault_amount_before: insurance_vault_amount,
		total_if_shares_before,
		total_if_shares_after: insurance_fund.total_shares,
	});

	insurance_withdraw.cast()
}
