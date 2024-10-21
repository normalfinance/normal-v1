use crate::error::{ NormalResult, ErrorCode };
use std::convert::TryFrom;

use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::oracle_map::OracleMap;
use crate::state::market_map::MarketMap;
use crate::state::state::OracleGuardRails;
use crate::state::traits::Size;
use crate::state::user::{ User, UserStats };
use crate::validate;
use anchor_lang::accounts::account::Account;
use anchor_lang::prelude::{ AccountInfo, Interface };
use anchor_lang::prelude::{ AccountLoader, InterfaceAccount };
use anchor_lang::Discriminator;
use anchor_spl::token::TokenAccount;
use anchor_spl::token_interface::{ Mint, TokenInterface };
use arrayref::array_ref;
use solana_program::account_info::next_account_info;
use solana_program::msg;
use std::iter::Peekable;
use std::slice::Iter;

pub struct AccountMaps<'a> {
    pub market_map: MarketMap<'a>,
    pub oracle_map: OracleMap<'a>,
}

pub fn load_maps<'a, 'b>(
    account_info_iter: &mut Peekable<Iter<'a, AccountInfo<'a>>>,
    writable_markets: &'b MarketSet,
    slot: u64,
    oracle_guard_rails: Option<OracleGuardRails>
) -> NormalResult<AccountMaps<'a>> {
    let oracle_map = OracleMap::load(account_info_iter, slot, oracle_guard_rails)?;
    let market_map = MarketMap::load(writable_markets, account_info_iter)?;

    Ok(AccountMaps {
        market_map,
        oracle_map,
    })
}

pub fn get_maker_and_maker_stats<'a>(
    account_info_iter: &mut Peekable<Iter<'a, AccountInfo<'a>>>
) -> NormalResult<(AccountLoader<'a, User>, AccountLoader<'a, UserStats>)> {
    let maker_account_info = next_account_info(account_info_iter).or(
        Err(ErrorCode::MakerNotFound)
    )?;

    validate!(maker_account_info.is_writable, ErrorCode::MakerMustBeWritable)?;

    let maker: AccountLoader<User> = AccountLoader::try_from(maker_account_info).or(
        Err(ErrorCode::CouldNotDeserializeMaker)
    )?;

    let maker_stats_account_info = next_account_info(account_info_iter).or(
        Err(ErrorCode::MakerStatsNotFound)
    )?;

    validate!(maker_stats_account_info.is_writable, ErrorCode::MakerStatsMustBeWritable)?;

    let maker_stats: AccountLoader<UserStats> = AccountLoader::try_from(
        maker_stats_account_info
    ).or(Err(ErrorCode::CouldNotDeserializeMakerStats))?;

    Ok((maker, maker_stats))
}

#[allow(clippy::type_complexity)]
pub fn get_referrer_and_referrer_stats<'a>(
    account_info_iter: &mut Peekable<Iter<'a, AccountInfo<'a>>>
) -> NormalResult<(Option<AccountLoader<'a, User>>, Option<AccountLoader<'a, UserStats>>)> {
    let referrer_account_info = account_info_iter.peek();

    if referrer_account_info.is_none() {
        return Ok((None, None));
    }

    let referrer_account_info = referrer_account_info.safe_unwrap()?;
    let data = referrer_account_info.try_borrow_data().map_err(|e| {
        msg!("{:?}", e);
        ErrorCode::CouldNotDeserializeReferrer
    })?;

    if data.len() < User::SIZE {
        return Ok((None, None));
    }

    let user_discriminator: [u8; 8] = User::discriminator();
    let account_discriminator = array_ref![data, 0, 8];
    if account_discriminator != &user_discriminator {
        return Ok((None, None));
    }

    let referrer_account_info = next_account_info(account_info_iter).safe_unwrap()?;

    validate!(referrer_account_info.is_writable, ErrorCode::ReferrerMustBeWritable)?;

    let referrer: AccountLoader<User> = AccountLoader::try_from(referrer_account_info).or(
        Err(ErrorCode::CouldNotDeserializeReferrer)
    )?;

    let referrer_stats_account_info = account_info_iter.peek();
    if referrer_stats_account_info.is_none() {
        return Ok((None, None));
    }

    let referrer_stats_account_info = referrer_stats_account_info.safe_unwrap()?;
    let data = referrer_stats_account_info.try_borrow_data().map_err(|e| {
        msg!("{:?}", e);
        ErrorCode::CouldNotDeserializeReferrerStats
    })?;

    if data.len() < UserStats::SIZE {
        return Ok((None, None));
    }

    let user_stats_discriminator: [u8; 8] = UserStats::discriminator();
    let account_discriminator = array_ref![data, 0, 8];
    if account_discriminator != &user_stats_discriminator {
        return Ok((None, None));
    }

    let referrer_stats_account_info = next_account_info(account_info_iter).safe_unwrap()?;

    validate!(referrer_stats_account_info.is_writable, ErrorCode::ReferrerMustBeWritable)?;

    let referrer_stats: AccountLoader<UserStats> = AccountLoader::try_from(
        referrer_stats_account_info
    ).or(Err(ErrorCode::CouldNotDeserializeReferrerStats))?;

    Ok((Some(referrer), Some(referrer_stats)))
}

pub fn get_whitelist_token<'a>(
    account_info_iter: &mut Peekable<Iter<'a, AccountInfo<'a>>>
) -> NormalResult<Account<'a, TokenAccount>> {
    let token_account_info = account_info_iter.peek();
    if token_account_info.is_none() {
        msg!("Could not find whitelist token");
        return Err(ErrorCode::InvalidWhitelistToken);
    }

    let token_account_info = token_account_info.safe_unwrap()?;
    let whitelist_token: Account<TokenAccount> = Account::try_from(token_account_info).map_err(|e| {
        msg!("Unable to deserialize whitelist token");
        msg!("{:?}", e);
        ErrorCode::InvalidWhitelistToken
    })?;

    Ok(whitelist_token)
}

pub fn get_token_interface<'a>(
    account_info_iter: &mut Peekable<Iter<'a, AccountInfo<'a>>>
) -> NormalResult<Option<Interface<'a, TokenInterface>>> {
    let token_interface_account_info = account_info_iter.peek();
    if token_interface_account_info.is_none() {
        return Ok(None);
    }

    let token_interface_account_info = account_info_iter.next().safe_unwrap()?;
    let token_interface: Interface<TokenInterface> = Interface::try_from(
        token_interface_account_info
    ).map_err(|e| {
        msg!("Unable to deserialize token interface");
        msg!("{:?}", e);
        ErrorCode::DefaultError
    })?;

    Ok(Some(token_interface))
}

pub fn get_token_mint<'a>(
    account_info_iter: &mut Peekable<Iter<'a, AccountInfo<'a>>>
) -> NormalResult<Option<InterfaceAccount<'a, Mint>>> {
    let mint_account_info = account_info_iter.peek();
    if mint_account_info.is_none() {
        return Ok(None);
    }

    let mint_account_info = account_info_iter.next().safe_unwrap()?;

    match InterfaceAccount::try_from(mint_account_info) {
        Ok(mint) => Ok(Some(mint)),
        Err(_) => Ok(None),
    }
}