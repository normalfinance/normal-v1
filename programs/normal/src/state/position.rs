use anchor_lang::prelude::*;

#[zero_copy(unsafe)]
#[derive(Default, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Position {
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

impl SpotBalance for Position {
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

impl Position {
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

	// From Vault
	// pub fn is_for(&self, market_index: u16) -> bool {
	// 	self.market_index == market_index
	// }

	// pub fn is_being_liquidated(&self) -> bool {
	// 	self.status &
	// 		((VaultStatus::BeingLiquidated as u8) | (VaultStatus::Bankrupt as u8)) > 0
	// }

	// pub fn is_bankrupt(&self) -> bool {
	// 	self.status & (VaultStatus::Bankrupt as u8) > 0
	// }

	// pub fn is_reduce_only(&self) -> bool {
	// 	self.status & (VaultStatus::ReduceOnly as u8) > 0
	// }

	// pub fn add_user_status(&mut self, status: VaultStatus) {
	// 	self.status |= status as u8;
	// }

	// pub fn remove_user_status(&mut self, status: VaultStatus) {
	// 	self.status &= !(status as u8);
	// }

	// pub fn enter_liquidation(&mut self, slot: u64) -> NormalResult<u16> {
	// 	if self.is_being_liquidated() {
	// 		return self.next_liquidation_id.safe_sub(1);
	// 	}

	// 	self.add_user_status(VaultStatus::BeingLiquidated);
	// 	self.liquidation_margin_freed = 0;
	// 	self.last_active_slot = slot;
	// 	Ok(get_then_update_id!(self, next_liquidation_id))
	// }

	// pub fn exit_liquidation(&mut self) {
	// 	self.remove_user_status(VaultStatus::BeingLiquidated);
	// 	self.remove_user_status(VaultStatus::Bankrupt);
	// 	self.liquidation_margin_freed = 0;
	// }

	// pub fn enter_bankruptcy(&mut self) {
	// 	self.remove_user_status(VaultStatus::BeingLiquidated);
	// 	self.add_user_status(VaultStatus::Bankrupt);
	// }

	// pub fn exit_bankruptcy(&mut self) {
	// 	self.remove_user_status(VaultStatus::BeingLiquidated);
	// 	self.remove_user_status(VaultStatus::Bankrupt);
	// 	self.liquidation_margin_freed = 0;
	// }

	// pub fn update_last_active_slot(&mut self, slot: u64) {
	// 	if !self.is_being_liquidated() {
	// 		self.last_active_slot = slot;
	// 	}
	// 	self.idle = false;
	// }

	// pub fn update_reduce_only_status(
	// 	&mut self,
	// 	reduce_only: bool
	// ) -> NormalResult {
	// 	if reduce_only {
	// 		self.add_user_status(VaultStatus::ReduceOnly);
	// 	} else {
	// 		self.remove_user_status(VaultStatus::ReduceOnly);
	// 	}

	// 	Ok(())
	// }
}

pub(crate) type VaultPositions = [VaultPosition; 8];
