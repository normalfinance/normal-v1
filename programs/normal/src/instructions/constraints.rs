use anchor_lang::accounts::account::Account;
use anchor_lang::accounts::account_loader::AccountLoader;
use anchor_lang::accounts::signer::Signer;
use anchor_lang::prelude::{ AccountInfo, Pubkey };

use crate::errors::ErrorCode;
use crate::state::insurance::InsuranceFundStake;
use crate::state::state::{ ExchangeStatus, State };
use crate::state::synth_market::SynthMarket;
use crate::state::user::User;
use crate::state::user_stats::UserStats;
// use crate::validate;
// use solana_program::msg;

pub fn can_sign_for_user(
	user: &AccountLoader<User>,
	signer: &Signer
) -> anchor_lang::Result<bool> {
	user
		.load()
		.map(|user| {
			user.authority.eq(signer.key) ||
				(user.delegate.eq(signer.key) && !user.delegate.eq(&Pubkey::default()))
		})
}

pub fn is_stats_for_user(
	user: &AccountLoader<User>,
	user_stats: &AccountLoader<UserStats>
) -> anchor_lang::Result<bool> {
	let user = user.load()?;
	let user_stats = user_stats.load()?;
	Ok(user_stats.authority.eq(&user.authority))
}

pub fn is_stats_for_if_stake(
	if_stake: &AccountLoader<InsuranceFundStake>,
	user_stats: &AccountLoader<UserStats>
) -> anchor_lang::Result<bool> {
	let if_stake = if_stake.load()?;
	let user_stats = user_stats.load()?;
	Ok(user_stats.authority.eq(&if_stake.authority))
}

pub fn market_valid(
	market: &AccountLoader<SynthMarket>
) -> anchor_lang::Result<()> {
	if market.load()?.status == SynthMarketStatus::Delisted {
		return Err(ErrorCode::MarketDelisted.into());
	}
	Ok(())
}

pub fn valid_oracle_for_market(
	oracle: &AccountInfo,
	market: &AccountLoader<SynthMarket>
) -> anchor_lang::Result<()> {
	validate!(
		market.load()?.amm.oracle.eq(oracle.key),
		ErrorCode::InvalidOracle,
		"not valid_oracle_for_synth_market"
	)?;
	Ok(())
}

pub fn liq_not_paused(state: &Account<State>) -> anchor_lang::Result<()> {
	if state.get_exchange_status()?.contains(ExchangeStatus::LiqPaused) {
		return Err(ErrorCode::ExchangePaused.into());
	}
	Ok(())
}

pub fn amm_not_paused(state: &Account<State>) -> anchor_lang::Result<()> {
	if state.amm_paused()? {
		return Err(ErrorCode::ExchangePaused.into());
	}
	Ok(())
}

pub fn deposit_not_paused(state: &Account<State>) -> anchor_lang::Result<()> {
	if state.get_exchange_status()?.contains(ExchangeStatus::DepositPaused) {
		return Err(ErrorCode::ExchangePaused.into());
	}
	Ok(())
}

pub fn withdraw_not_paused(state: &Account<State>) -> anchor_lang::Result<()> {
	if state.get_exchange_status()?.contains(ExchangeStatus::WithdrawPaused) {
		return Err(ErrorCode::ExchangePaused.into());
	}
	Ok(())
}

pub fn exchange_not_paused(state: &Account<State>) -> anchor_lang::Result<()> {
	if state.get_exchange_status()?.is_all() {
		return Err(ErrorCode::ExchangePaused.into());
	}
	Ok(())
}
