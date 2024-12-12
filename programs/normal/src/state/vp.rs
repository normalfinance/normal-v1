use anchor_lang::prelude::*;

#[zero_copy(unsafe)]
#[derive(Default, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct VaultPosition {
	/// The scaled balance of the position. To get the token amount, multiply by the cumulative deposit/borrow
	/// interest of corresponding market.
	/// precision: SPOT_BALANCE_PRECISION
	pub scaled_balance: u64,
	/// The cumulative deposits/borrows a user has made into a market
	/// precision: token mint precision
	pub cumulative_deposits: i64,
	/// The market index of the corresponding market
	pub market_index: u16,
	pub padding: [u8; 4],
}

impl SpotBalance for VaultPosition {
	fn market_index(&self) -> u16 {
		self.market_index
	}

	fn balance(&self) -> u128 {
		self.scaled_balance as u128
	}

	fn increase_balance(&mut self, delta: u128) -> NormalResult {
		self.scaled_balance = self.scaled_balance.safe_add(delta.cast()?)?;
		Ok(())
	}

	fn decrease_balance(&mut self, delta: u128) -> NormalResult {
		self.scaled_balance = self.scaled_balance.safe_sub(delta.cast()?)?;
		Ok(())
	}
}

impl VaultPosition {
	pub fn is_for(&self, market_index: u16) -> bool {
		self.market_index == market_index && !self.is_available()
	}

	pub fn is_available(&self) -> bool {
		self.scaled_balance == 0
	}

	pub fn get_token_amount(
		&self,
		spot_market: &SpotMarket
	) -> NormalResult<u128> {
		get_token_amount(
			self.scaled_balance.cast()?,
			spot_market,
			&self.balance_type
		)
	}

	pub fn get_signed_token_amount(
		&self,
		spot_market: &SpotMarket
	) -> NormalResult<i128> {
		get_signed_token_amount(
			get_token_amount(
				self.scaled_balance.cast()?,
				spot_market,
				&self.balance_type
			)?,
			&self.balance_type
		)
	}
}

pub(crate) type VaultPositions = [VaultPosition; 8];
