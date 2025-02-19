use anchor_lang::accounts::account::Account;
use anchor_lang::accounts::account_loader::AccountLoader;
use anchor_lang::accounts::signer::Signer;
use anchor_lang::prelude::{ AccountInfo, Pubkey };

use spl_governance::state::{ get_proposal, Proposal, ProposalState };

use crate::error::ErrorCode;
use crate::state::amm::AMM;
use crate::state::market::{ Market, MarketStatus };
use crate::state::state::{ ExchangeStatus, State };
use crate::state::user::{ User, UserStats };
use crate::state::vault::Vault;
use crate::validate;
use solana_program::msg;

pub fn has_been_approved(proposal_id: Pubkey) -> anchor_lang::Result<()> {
	// Fetch the governance proposal using the Realms DAO program
	let governance_proposal = spl_governance::get_proposal(
		&ctx.accounts.governance_program, // Governance Program Account
		&proposal_id
	)?;

	if governance_proposal.state != ProposalState::Approved {
		return Err(ErrorCode::InvalidGovernanceProposial.into());
	}

	Ok(())
}

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

pub fn can_sign_for_vault(
	vault: &AccountLoader<Vault>,
	signer: &Signer
) -> anchor_lang::Result<bool> {
	vault
		.load()
		.map(|vault| {
			vault.authority.eq(signer.key) ||
				(vault.delegate.eq(signer.key) &&
					!vault.delegate.eq(&Pubkey::default()))
		})
}

pub fn market_valid(market: &AccountLoader<Market>) -> anchor_lang::Result<()> {
	if market.load()?.status == MarketStatus::Delisted {
		return Err(ErrorCode::MarketDelisted.into());
	}
	Ok(())
}

pub fn valid_oracle_for_amm(
	oracle: &AccountInfo,
	amm: &AccountLoader<AMM>
) -> anchor_lang::Result<()> {
	validate!(
		amm.load()?.oracle.eq(oracle.key),
		ErrorCode::InvalidOracle,
		"not valid_oracle_for_amm"
	)?;
	Ok(())
}

pub fn liq_not_paused(state: &Account<State>) -> anchor_lang::Result<()> {
	if state.get_exchange_status()?.contains(ExchangeStatus::LiqPaused) {
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
