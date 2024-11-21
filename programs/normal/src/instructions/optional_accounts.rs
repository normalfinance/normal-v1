use crate::error::{ NormalResult, ErrorCode };
use std::convert::TryFrom;

use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::oracle_map::OracleMap;
use crate::state::state::OracleGuardRails;
use crate::state::traits::Size;
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
	pub oracle_map: OracleMap<'a>,
}

pub fn load_maps<'a, 'b>(
	account_info_iter: &mut Peekable<Iter<'a, AccountInfo<'a>>>,
	writable_markets: &'b MarketSet,
	slot: u64,
	oracle_guard_rails: Option<OracleGuardRails>
) -> NormalResult<AccountMaps<'a>> {
	let oracle_map = OracleMap::load(
		account_info_iter,
		slot,
		oracle_guard_rails
	)?;

	Ok(AccountMaps {
		oracle_map,
	})
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
