use anchor_lang::accounts::account::Account;
use anchor_lang::accounts::account_loader::AccountLoader;
use anchor_lang::accounts::signer::Signer;
use anchor_lang::prelude::{ AccountInfo, Pubkey };

use spl_governance::state::{get_proposal, Proposal, ProposalState};

use crate::error::ErrorCode;
use crate::state::market::{ Market, MarketStatus };
use crate::state::state::{ ExchangeStatus, State };
use crate::state::user::{ User, UserStats };
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

pub fn can_sign_for_user(user: &AccountLoader<User>, signer: &Signer) -> anchor_lang::Result<bool> {
    user.load().map(|user| {
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

pub fn market_valid(market: &AccountLoader<Market>) -> anchor_lang::Result<()> {
    if market.load()?.status == MarketStatus::Delisted {
        return Err(ErrorCode::MarketDelisted.into());
    }
    Ok(())
}

pub fn valid_oracle_for_market(
    oracle: &AccountInfo,
    market: &AccountLoader<Market>
) -> anchor_lang::Result<()> {
    validate!(
        market.load()?.oracle.eq(oracle.key),
        ErrorCode::InvalidOracle,
        "not valid_oracle_for_market"
    )?;
    Ok(())
}

pub fn amm_not_paused(state: &Account<State>) -> anchor_lang::Result<()> {
    if state.amm_paused()? {
        return Err(ErrorCode::ExchangePaused.into());
    }
    Ok(())
}

pub fn fill_not_paused(state: &Account<State>) -> anchor_lang::Result<()> {
    if state.get_exchange_status()?.contains(ExchangeStatus::FillPaused) {
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