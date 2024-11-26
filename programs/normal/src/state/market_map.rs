use anchor_lang::accounts::account_loader::AccountLoader;
use std::cell::{ Ref, RefMut };
use std::collections::{ BTreeMap, BTreeSet };
use std::iter::Peekable;
use std::slice::Iter;

use anchor_lang::prelude::AccountInfo;

use anchor_lang::Discriminator;
use arrayref::array_ref;

use crate::error::{ NormalResult, ErrorCode };
use crate::state::pmarket::Market;
use crate::state::user::PerpPositions;

use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::traits::Size;
use solana_program::msg;
use std::panic::Location;

pub struct MarketMap<'a>(pub BTreeMap<u16, AccountLoader<'a, Market>>);

impl<'a> MarketMap<'a> {
	#[track_caller]
	#[inline(always)]
	pub fn get_ref(&self, market_index: &u16) -> NormalResult<Ref<Market>> {
		let loader = match self.0.get(market_index) {
			Some(loader) => loader,
			None => {
				let caller = Location::caller();
				msg!(
					"Could not find market {} at {}:{}",
					market_index,
					caller.file(),
					caller.line()
				);
				return Err(ErrorCode::MarketNotFound);
			}
		};

		match loader.load() {
			Ok(market) => Ok(market),
			Err(e) => {
				let caller = Location::caller();
				msg!("{:?}", e);
				msg!(
					"Could not load market {} at {}:{}",
					market_index,
					caller.file(),
					caller.line()
				);
				Err(ErrorCode::UnableToLoadMarketAccount)
			}
		}
	}

	#[track_caller]
	#[inline(always)]
	pub fn get_ref_mut(
		&self,
		market_index: &u16
	) -> NormalResult<RefMut<Market>> {
		let loader = match self.0.get(market_index) {
			Some(loader) => loader,
			None => {
				let caller = Location::caller();
				msg!(
					"Could not find market {} at {}:{}",
					market_index,
					caller.file(),
					caller.line()
				);
				return Err(ErrorCode::MarketNotFound);
			}
		};

		match loader.load_mut() {
			Ok(market) => Ok(market),
			Err(e) => {
				let caller = Location::caller();
				msg!("{:?}", e);
				msg!(
					"Could not load market {} at {}:{}",
					market_index,
					caller.file(),
					caller.line()
				);
				Err(ErrorCode::UnableToLoadMarketAccount)
			}
		}
	}

	pub fn load<'b, 'c>(
		writable_markets: &'b MarketSet,
		account_info_iter: &'c mut Peekable<Iter<'a, AccountInfo<'a>>>
	) -> NormalResult<MarketMap<'a>> {
		let mut market_map: MarketMap = MarketMap(BTreeMap::new());

		let market_discriminator: [u8; 8] = Market::discriminator();
		while let Some(account_info) = account_info_iter.peek() {
			let data = account_info
				.try_borrow_data()
				.or(Err(ErrorCode::CouldNotLoadMarketData))?;

			let expected_data_len = Market::SIZE;
			if data.len() < expected_data_len {
				break;
			}

			let account_discriminator = array_ref![data, 0, 8];
			if account_discriminator != &market_discriminator {
				break;
			}

			// market index 1160 bytes from front of account
			let market_index = u16::from_le_bytes(*array_ref![data, 1160, 2]);

			if market_map.0.contains_key(&market_index) {
				msg!("Can not include same market index twice {}", market_index);
				return Err(ErrorCode::InvalidMarketAccount);
			}

			let account_info = account_info_iter.next().safe_unwrap()?;

			let is_writable = account_info.is_writable;
			if writable_markets.contains(&market_index) && !is_writable {
				return Err(ErrorCode::MarketWrongMutability);
			}

			let account_loader: AccountLoader<Market> = AccountLoader::try_from(
				account_info
			).or(Err(ErrorCode::InvalidMarketAccount))?;

			market_map.0.insert(market_index, account_loader);
		}

		Ok(market_map)
	}
}

#[cfg(test)]
impl<'a> MarketMap<'a> {
	pub fn load_one<'c: 'a>(
		account_info: &'c AccountInfo<'a>,
		must_be_writable: bool
	) -> NormalResult<MarketMap<'a>> {
		let mut market_map: MarketMap = MarketMap(BTreeMap::new());

		let data = account_info
			.try_borrow_data()
			.or(Err(ErrorCode::CouldNotLoadMarketData))?;

		let expected_data_len = Market::SIZE;
		if data.len() < expected_data_len {
			return Err(ErrorCode::CouldNotLoadMarketData);
		}

		let market_discriminator: [u8; 8] = Market::discriminator();
		let account_discriminator = array_ref![data, 0, 8];
		if account_discriminator != &market_discriminator {
			return Err(ErrorCode::CouldNotLoadMarketData);
		}

		// market index 1160 bytes from front of account
		let market_index = u16::from_le_bytes(*array_ref![data, 1160, 2]);

		let is_writable = account_info.is_writable;
		let account_loader: AccountLoader<Market> = AccountLoader::try_from(
			account_info
		).or(Err(ErrorCode::InvalidMarketAccount))?;

		if must_be_writable && !is_writable {
			return Err(ErrorCode::MarketWrongMutability);
		}

		market_map.0.insert(market_index, account_loader);

		Ok(market_map)
	}

	pub fn empty() -> Self {
		MarketMap(BTreeMap::new())
	}

	pub fn load_multiple<'c: 'a>(
		account_infos: Vec<&'c AccountInfo<'a>>,
		must_be_writable: bool
	) -> NormalResult<MarketMap<'a>> {
		let mut market_map: MarketMap = MarketMap(BTreeMap::new());

		for account_info in account_infos {
			let data = account_info
				.try_borrow_data()
				.or(Err(ErrorCode::CouldNotLoadMarketData))?;

			let expected_data_len = Market::SIZE;
			if data.len() < expected_data_len {
				return Err(ErrorCode::CouldNotLoadMarketData);
			}

			let market_discriminator: [u8; 8] = Market::discriminator();
			let account_discriminator = array_ref![data, 0, 8];
			if account_discriminator != &market_discriminator {
				return Err(ErrorCode::CouldNotLoadMarketData);
			}

			// market index 1160 bytes from front of account
			let market_index = u16::from_le_bytes(*array_ref![data, 1160, 2]);

			let is_writable = account_info.is_writable;
			let account_loader: AccountLoader<Market> = AccountLoader::try_from(
				account_info
			).or(Err(ErrorCode::InvalidMarketAccount))?;

			if must_be_writable && !is_writable {
				return Err(ErrorCode::MarketWrongMutability);
			}

			market_map.0.insert(market_index, account_loader);
		}

		Ok(market_map)
	}
}

pub(crate) type MarketSet = BTreeSet<u16>;

pub fn get_writable_market_set(market_index: u16) -> MarketSet {
	let mut writable_markets = MarketSet::new();
	writable_markets.insert(market_index);
	writable_markets
}

pub fn get_writable_market_set_from_vec(market_indexes: &[u16]) -> MarketSet {
	let mut writable_markets = MarketSet::new();
	for market_index in market_indexes.iter() {
		writable_markets.insert(*market_index);
	}
	writable_markets
}

pub fn get_market_set_from_list(market_indexes: [u16; 5]) -> MarketSet {
	let mut writable_markets = MarketSet::new();
	for market_index in market_indexes.iter() {
		if *market_index == 100 {
			continue; // todo
		}
		writable_markets.insert(*market_index);
	}
	writable_markets
}
