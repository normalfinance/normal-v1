use anchor_lang::prelude::*;

use crate::{ errors::ErrorCode, math::MAX_PROTOCOL_FEE_RATE };

use super::paused_operations::InsuranceFundOperation;

#[account]
pub struct InsuranceFund {
	pub authority: Pubkey,
	pub vault: Pubkey,
	pub total_shares: u128,
	pub user_shares: u128,
	pub shares_base: u128, // exponent for lp shares (for rebasing)
	pub unstaking_period: i64,
	pub last_revenue_settle_ts: i64,
	pub revenue_settle_period: i64,
	pub total_factor: u32, // percentage of interest for total insurance
	pub user_factor: u32, // percentage of interest for user staked insurance
	pub max_insurance: u64,
	pub paused_operations: u8,
}

impl InsuranceFund {
	pub fn is_operation_paused(&self, operation: InsuranceFundOperation) -> bool {
		InsuranceFundOperation::is_operation_paused(
			self.paused_operations,
			operation
		)
	}
}

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct InsuranceClaim {
	/// The amount of revenue last settled
	/// Positive if funds left the market,
	/// negative if funds were pulled into the market
	/// precision: QUOTE_PRECISION
	pub revenue_withdraw_since_last_settle: i64,
	/// The max amount of revenue that can be withdrawn per period
	/// precision: QUOTE_PRECISION
	pub max_revenue_withdraw_per_period: u64,
	/// The max amount of insurance that market can use to resolve bankruptcy and pnl deficits
	/// precision: QUOTE_PRECISION
	pub quote_max_insurance: u64,
	/// The amount of insurance that has been used to resolve bankruptcy and pnl deficits
	/// precision: QUOTE_PRECISION
	pub quote_settled_insurance: u64,
	/// The last time revenue was settled in/out of market
	pub last_revenue_withdraw_ts: i64,
}

#[account(zero_copy(unsafe))]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct InsuranceFundStake {
	pub authority: Pubkey,
	if_shares: u128,
	pub last_withdraw_request_shares: u128, // get zero as 0 when not in escrow
	pub if_base: u128, // exponent for if_shares decimal places (for rebase)
	pub last_valid_ts: i64,
	pub last_withdraw_request_value: u64,
	pub last_withdraw_request_ts: i64,
	pub cost_basis: i64,
	pub padding: [u8; 14],
}

// implement SIZE const for InsuranceFundStake
impl Size for InsuranceFundStake {
	const SIZE: usize = 136;
}

impl InsuranceFundStake {
	pub fn new(authority: Pubkey, now: i64) -> Self {
		InsuranceFundStake {
			authority,
			last_withdraw_request_shares: 0,
			last_withdraw_request_value: 0,
			last_withdraw_request_ts: 0,
			cost_basis: 0,
			if_base: 0,
			last_valid_ts: now,
			if_shares: 0,
			padding: [0; 14],
		}
	}

	fn validate_base(&self, spot_market: &SpotMarket) -> NormalResult {
		validate!(
			self.if_base == spot_market.insurance_fund.shares_base,
			ErrorCode::InvalidIFRebase,
			"if stake bases mismatch. user base: {} market base {}",
			self.if_base,
			spot_market.insurance_fund.shares_base
		)?;

		Ok(())
	}

	pub fn checked_if_shares(
		&self,
		spot_market: &SpotMarket
	) -> NormalResult<u128> {
		self.validate_base(spot_market)?;
		Ok(self.if_shares)
	}

	pub fn unchecked_if_shares(&self) -> u128 {
		self.if_shares
	}

	pub fn increase_if_shares(
		&mut self,
		delta: u128,
		spot_market: &SpotMarket
	) -> NormalResult {
		self.validate_base(spot_market)?;
		safe_increment!(self.if_shares, delta);
		Ok(())
	}

	pub fn decrease_if_shares(
		&mut self,
		delta: u128,
		spot_market: &SpotMarket
	) -> NormalResult {
		self.validate_base(spot_market)?;
		safe_decrement!(self.if_shares, delta);
		Ok(())
	}

	pub fn update_if_shares(
		&mut self,
		new_shares: u128,
		spot_market: &SpotMarket
	) -> NormalResult {
		self.validate_base(spot_market)?;
		self.if_shares = new_shares;

		Ok(())
	}
}
