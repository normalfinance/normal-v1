use anchor_lang::accounts::account_loader::AccountLoader;
use std::cell::{ Ref, RefMut };
use std::collections::{ BTreeMap, BTreeSet };
use std::iter::Peekable;
use std::slice::Iter;

use anchor_lang::prelude::AccountInfo;

use anchor_lang::Discriminator;
use arrayref::array_ref;

use crate::error::{ NormalResult, ErrorCode };
use crate::state::vault::Vault;

use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::traits::Size;
use solana_program::msg;
use std::panic::Location;

pub struct VaultMap<'a>(pub BTreeMap<u16, AccountLoader<'a, Vault>>);

impl<'a> VaultMap<'a> {
	#[track_caller]
	#[inline(always)]
	pub fn get_ref(&self, vault_index: &u16) -> NormalResult<Ref<Vault>> {
		let loader = match self.0.get(vault_index) {
			Some(loader) => loader,
			None => {
				let caller = Location::caller();
				msg!(
					"Could not find vault {} at {}:{}",
					vault_index,
					caller.file(),
					caller.line()
				);
				return Err(ErrorCode::VaultNotFound);
			}
		};

		match loader.load() {
			Ok(vault) => Ok(vault),
			Err(e) => {
				let caller = Location::caller();
				msg!("{:?}", e);
				msg!(
					"Could not load vault {} at {}:{}",
					vault_index,
					caller.file(),
					caller.line()
				);
				Err(ErrorCode::UnableToLoadVaultAccount)
			}
		}
	}

	#[track_caller]
	#[inline(always)]
	pub fn get_ref_mut(&self, vault_index: &u16) -> NormalResult<RefMut<Vault>> {
		let loader = match self.0.get(vault_index) {
			Some(loader) => loader,
			None => {
				let caller = Location::caller();
				msg!(
					"Could not find vault {} at {}:{}",
					vault_index,
					caller.file(),
					caller.line()
				);
				return Err(ErrorCode::VaultNotFound);
			}
		};

		match loader.load_mut() {
			Ok(vault) => Ok(vault),
			Err(e) => {
				let caller = Location::caller();
				msg!("{:?}", e);
				msg!(
					"Could not load vault {} at {}:{}",
					vault_index,
					caller.file(),
					caller.line()
				);
				Err(ErrorCode::UnableToLoadVaultAccount)
			}
		}
	}

	pub fn load<'b, 'c>(
		writable_vaults: &'b MarketSet,
		account_info_iter: &'c mut Peekable<Iter<'a, AccountInfo<'a>>>
	) -> NormalResult<VaultMap<'a>> {
		let mut vault_map: VaultMap = VaultMap(BTreeMap::new());

		let vault_discriminator: [u8; 8] = Vault::discriminator();
		while let Some(account_info) = account_info_iter.peek() {
			let data = account_info
				.try_borrow_data()
				.or(Err(ErrorCode::CouldNotLoadMarketData))?;

			let expected_data_len = Vault::SIZE;
			if data.len() < expected_data_len {
				break;
			}

			let account_discriminator = array_ref![data, 0, 8];
			if account_discriminator != &vault_discriminator {
				break;
			}

			// vault index 1160 bytes from front of account
			let vault_index = u16::from_le_bytes(*array_ref![data, 1160, 2]);

			if vault_map.0.contains_key(&vault_index) {
				msg!("Can not include same vault index twice {}", vault_index);
				return Err(ErrorCode::InvalidMarketAccount);
			}

			let account_info = account_info_iter.next().safe_unwrap()?;

			let is_writable = account_info.is_writable;
			if writable_vaults.contains(&vault_index) && !is_writable {
				return Err(ErrorCode::MarketWrongMutability);
			}

			let account_loader: AccountLoader<Vault> = AccountLoader::try_from(
				account_info
			).or(Err(ErrorCode::InvalidMarketAccount))?;

			vault_map.0.insert(vault_index, account_loader);
		}

		Ok(vault_map)
	}
}

#[cfg(test)]
impl<'a> VaultMap<'a> {
	pub fn load_one<'c: 'a>(
		account_info: &'c AccountInfo<'a>,
		must_be_writable: bool
	) -> NormalResult<VaultMap<'a>> {
		let mut vault_map: VaultMap = VaultMap(BTreeMap::new());

		let data = account_info
			.try_borrow_data()
			.or(Err(ErrorCode::CouldNotLoadMarketData))?;

		let expected_data_len = Vault::SIZE;
		if data.len() < expected_data_len {
			return Err(ErrorCode::CouldNotLoadMarketData);
		}

		let vault_discriminator: [u8; 8] = Vault::discriminator();
		let account_discriminator = array_ref![data, 0, 8];
		if account_discriminator != &vault_discriminator {
			return Err(ErrorCode::CouldNotLoadMarketData);
		}

		// vault index 1160 bytes from front of account
		let vault_index = u16::from_le_bytes(*array_ref![data, 1160, 2]);

		let is_writable = account_info.is_writable;
		let account_loader: AccountLoader<Vault> = AccountLoader::try_from(
			account_info
		).or(Err(ErrorCode::InvalidMarketAccount))?;

		if must_be_writable && !is_writable {
			return Err(ErrorCode::MarketWrongMutability);
		}

		vault_map.0.insert(vault_index, account_loader);

		Ok(vault_map)
	}

	pub fn empty() -> Self {
		VaultMap(BTreeMap::new())
	}

	pub fn load_multiple<'c: 'a>(
		account_infos: Vec<&'c AccountInfo<'a>>,
		must_be_writable: bool
	) -> NormalResult<VaultMap<'a>> {
		let mut vault_map: VaultMap = VaultMap(BTreeMap::new());

		for account_info in account_infos {
			let data = account_info
				.try_borrow_data()
				.or(Err(ErrorCode::CouldNotLoadMarketData))?;

			let expected_data_len = Vault::SIZE;
			if data.len() < expected_data_len {
				return Err(ErrorCode::CouldNotLoadMarketData);
			}

			let vault_discriminator: [u8; 8] = Vault::discriminator();
			let account_discriminator = array_ref![data, 0, 8];
			if account_discriminator != &vault_discriminator {
				return Err(ErrorCode::CouldNotLoadMarketData);
			}

			// vault index 1160 bytes from front of account
			let vault_index = u16::from_le_bytes(*array_ref![data, 1160, 2]);

			let is_writable = account_info.is_writable;
			let account_loader: AccountLoader<Vault> = AccountLoader::try_from(
				account_info
			).or(Err(ErrorCode::InvalidMarketAccount))?;

			if must_be_writable && !is_writable {
				return Err(ErrorCode::MarketWrongMutability);
			}

			vault_map.0.insert(vault_index, account_loader);
		}

		Ok(vault_map)
	}
}

pub(crate) type MarketSet = BTreeSet<u16>;

pub fn get_writable_vault_set(vault_index: u16) -> MarketSet {
	let mut writable_vaults = MarketSet::new();
	writable_vaults.insert(vault_index);
	writable_vaults
}

pub fn get_writable_vault_set_from_vec(vault_indexes: &[u16]) -> MarketSet {
	let mut writable_vaults = MarketSet::new();
	for vault_index in vault_indexes.iter() {
		writable_vaults.insert(*vault_index);
	}
	writable_vaults
}

pub fn get_market_set_from_list(vault_indexes: [u16; 5]) -> MarketSet {
	let mut writable_vaults = MarketSet::new();
	for vault_index in vault_indexes.iter() {
		if *vault_index == 100 {
			continue; // todo
		}
		writable_vaults.insert(*vault_index);
	}
	writable_vaults
}
