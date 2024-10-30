use crate::error::{ NormalResult, ErrorCode };
use crate::state::market::Market;
use anchor_lang::prelude::{ AccountInfo, AccountLoader };
use std::cell::{ Ref, RefMut };
use std::collections::{ BTreeMap, BTreeSet };

use std::iter::Peekable;
use std::slice::Iter;

use crate::constants::constants::QUOTE_SPOT_MARKET_INDEX;
use anchor_lang::Discriminator;
use arrayref::array_ref;

use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::traits::Size;
use solana_program::msg;
use std::panic::Location;

pub struct MarketMap<'a>(
	pub BTreeMap<u16, AccountLoader<'a, Market>>,
	MarketSet,
);

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
		if !self.1.contains(market_index) {
			let caller = Location::caller();
			msg!(
				"Market {} not expected to be mutable at {}:{}",
				market_index,
				caller.file(),
				caller.line()
			);
			return Err(ErrorCode::MarketWrongMutability);
		}

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

	#[track_caller]
	#[inline(always)]
	pub fn get_quote_market(&self) -> NormalResult<Ref<Market>> {
		let loader = match self.0.get(&QUOTE_SPOT_MARKET_INDEX) {
			Some(loader) => loader,
			None => {
				let caller = Location::caller();
				msg!(
					"Could not find market {} at {}:{}",
					QUOTE_SPOT_MARKET_INDEX,
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
					QUOTE_SPOT_MARKET_INDEX,
					caller.file(),
					caller.line()
				);
				Err(ErrorCode::UnableToLoadMarketAccount)
			}
		}
	}

	#[track_caller]
	#[inline(always)]
	pub fn get_quote_market_mut(&self) -> NormalResult<RefMut<Market>> {
		if !self.1.contains(&QUOTE_SPOT_MARKET_INDEX) {
			let caller = Location::caller();
			msg!(
				"Market {} not expected to be mutable at {}:{}",
				QUOTE_SPOT_MARKET_INDEX,
				caller.file(),
				caller.line()
			);
			return Err(ErrorCode::MarketWrongMutability);
		}

		let loader = match self.0.get(&QUOTE_SPOT_MARKET_INDEX) {
			Some(loader) => loader,
			None => {
				let caller = Location::caller();
				msg!(
					"Could not find market {} at {}:{}",
					QUOTE_SPOT_MARKET_INDEX,
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
					QUOTE_SPOT_MARKET_INDEX,
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
		let mut market_map: MarketMap = MarketMap(
			BTreeMap::new(),
			writable_markets.clone()
		);

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

			let market_index = u16::from_le_bytes(*array_ref![data, 684, 2]);

			if market_map.0.contains_key(&market_index) {
				msg!("Can not include same market index twice {}", market_index);
				return Err(ErrorCode::InvalidMarketAccount);
			}

			let account_info = account_info_iter.next().safe_unwrap()?;
			let is_writable = account_info.is_writable;
			let account_loader: AccountLoader<Market> = AccountLoader::try_from(
				account_info
			).or(Err(ErrorCode::InvalidMarketAccount))?;

			if writable_markets.contains(&market_index) && !is_writable {
				return Err(ErrorCode::MarketWrongMutability);
			}

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
		let mut writable_markets = MarketSet::new();
		let mut map = BTreeMap::new();

		let market_discriminator: [u8; 8] = Market::discriminator();
		let data = account_info
			.try_borrow_data()
			.or(Err(ErrorCode::CouldNotLoadMarketData))?;

		let expected_data_len = Market::SIZE;
		if data.len() < expected_data_len {
			return Err(ErrorCode::CouldNotLoadMarketData);
		}

		let account_discriminator = array_ref![data, 0, 8];
		if account_discriminator != &market_discriminator {
			return Err(ErrorCode::CouldNotLoadMarketData);
		}

		let market_index = u16::from_le_bytes(*array_ref![data, 684, 2]);

		let is_writable = account_info.is_writable;
		let account_loader: AccountLoader<SpotMarket> = AccountLoader::try_from(
			account_info
		).or(Err(ErrorCode::InvalidMarketAccount))?;

		if must_be_writable && !is_writable {
			return Err(ErrorCode::SpotMarketWrongMutability);
		}

		if must_be_writable {
			writable_markets.insert(market_index);
		}

		if !must_be_writable && is_writable {
			msg!("market {} not expected to be writeable", market_index);
			return Err(ErrorCode::MarketWrongMutability);
		}

		map.insert(market_index, account_loader);

		Ok(MarketMap(map, writable_markets))
	}

	pub fn load_multiple<'c: 'a>(
		account_info: Vec<&'c AccountInfo<'a>>,
		must_be_writable: bool
	) -> NormalResult<MarketMap<'a>> {
		let mut writable_markets = MarketSet::new();
		let mut map = BTreeMap::new();

		let account_info_iter = account_info.into_iter();
		for account_info in account_info_iter {
			let market_discriminator: [u8; 8] = Market::discriminator();
			let data = account_info
				.try_borrow_data()
				.or(Err(ErrorCode::CouldNotLoadMarketData))?;

			let expected_data_len = Market::SIZE;
			if data.len() < expected_data_len {
				return Err(ErrorCode::CouldNotLoadMarketData);
			}

			let account_discriminator = array_ref![data, 0, 8];
			if account_discriminator != &market_discriminator {
				return Err(ErrorCode::CouldNotLoadMarketData);
			}

			let market_index = u16::from_le_bytes(*array_ref![data, 684, 2]);

			let is_writable = account_info.is_writable;
			let account_loader: AccountLoader<Market> = AccountLoader::try_from(
				account_info
			).or(Err(ErrorCode::InvalidMarketAccount))?;

			if must_be_writable {
				writable_markets.insert(market_index);
			}

			if must_be_writable && !is_writable {
				return Err(ErrorCode::MarketWrongMutability);
			}

			map.insert(market_index, account_loader);
		}

		Ok(MarketMap(map, writable_markets))
	}
}

pub(crate) type MarketSet = BTreeSet<u16>;

pub fn get_writable_market_set(market_index: u16) -> MarketSet {
	let mut writable_markets = MarketSet::new();
	writable_markets.insert(market_index);
	writable_markets
}

pub fn get_writable_market_set_from_many(
	market_indexes: Vec<u16>
) -> MarketSet {
	let mut writable_markets = MarketSet::new();
	for market_index in market_indexes {
		writable_markets.insert(market_index);
	}
	writable_markets
}
