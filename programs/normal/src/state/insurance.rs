use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use crate::error::{ NormalResult, ErrorCode };

use crate::state::market::Market;
use crate::state::paused_operations::InsuranceFundOperation;

// #[cfg(test)]
// mod tests;

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct InsuranceFund {
	/// The address of the market. It is a pda of the market index
	pub pubkey: Pubkey,
	pub vault: Pubkey,
	pub total_shares: u128,
	pub user_shares: u128,
	/// exponent for lp shares (for rebasing)
	pub shares_base: u128,
	/// time one must wait before removing deposited funds
	pub unstaking_period: i64,
	pub last_fee_deposit_ts: i64,
	/// the max insurance value needed (equal to the total open interest of all markets)
	pub max_insurance: u64,
	/// percentage of interest for total insurance
	pub total_factor: u32,
	/// percentage of interest for user staked insurance
	pub user_factor: u32,
}

impl InsuranceFund {
	pub fn is_insurance_fund_operation_paused(
		&self,
		operation: InsuranceFundOperation
	) -> bool {
		InsuranceFundOperation::is_operation_paused(
			self.insurance_fund_paused_operations,
			operation
		)
	}
}

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct InsuranceClaim {
	/// The amount of revenue last settled
	/// Positive if funds left the perp market,
	/// negative if funds were pulled into the perp market
	/// precision: QUOTE_PRECISION
	pub revenue_withdraw_since_last_settle: i64,
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
	insurance_fund_shares: u128,
	pub last_withdraw_request_shares: u128, // get zero as 0 when not in escrow
	pub insurance_fund_base: u128, // exponent for insurance_fund_shares decimal places (for rebase)
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
			insurance_fund_base: 0,
			last_valid_ts: now,
			insurance_fund_shares: 0,
			padding: [0; 14],
		}
	}

	fn validate_base(&self, insurance_fund: &InsuranceFund) -> NormalResult {
		validate!(
			self.insurance_fund_base == insurance_fund.shares_base,
			ErrorCode::InvalidIFRebase,
			"insurance_fund stake bases mismatch. user base: {} market base {}",
			self.insurance_fund_base,
			insurance_fund.shares_base
		)?;

		Ok(())
	}

	pub fn checked_insurance_fund_shares(
		&self,
		insurance_fund: &InsuranceFund
	) -> NormalResult<u128> {
		self.validate_base(insurance_fund)?;
		Ok(self.insurance_fund_shares)
	}

	pub fn unchecked_insurance_fund_shares(&self) -> u128 {
		self.insurance_fund_shares
	}

	pub fn increase_insurance_fund_shares(
		&mut self,
		delta: u128,
		insurance_fund: &InsuranceFund
	) -> NormalResult {
		self.validate_base(insurance_fund)?;
		safe_increment!(self.insurance_fund_shares, delta);
		Ok(())
	}

	pub fn decrease_insurance_fund_shares(
		&mut self,
		delta: u128,
		insurance_fund: &InsuranceFund
	) -> NormalResult {
		self.validate_base(insurance_fund)?;
		safe_decrement!(self.insurance_fund_shares, delta);
		Ok(())
	}

	pub fn update_insurance_fund_shares(
		&mut self,
		new_shares: u128,
		insurance_fund: &InsuranceFund
	) -> NormalResult {
		self.validate_base(insurance_fund)?;
		self.insurance_fund_shares = new_shares;

		Ok(())
	}
}

#[account(zero_copy(unsafe))]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct ProtocolInsuranceFundSharesTransferConfig {
	pub whitelisted_signers: [Pubkey; 4],
	pub max_transfer_per_epoch: u128,
	pub current_epoch_transfer: u128,
	pub next_epoch_ts: i64,
	pub padding: [u128; 8],
}

// implement SIZE const for ProtocolInsuranceFundSharesTransferConfig
impl Size for ProtocolInsuranceFundSharesTransferConfig {
	const SIZE: usize = 304;
}

impl ProtocolInsuranceFundSharesTransferConfig {
	pub fn validate_signer(&self, signer: &Pubkey) -> NormalResult {
		validate!(
			self.whitelisted_signers.contains(signer) && *signer != Pubkey::default(),
			ErrorCode::DefaultError,
			"signer {} not whitelisted",
			signer
		)?;

		Ok(())
	}

	pub fn update_epoch(&mut self, now: i64) -> NormalResult {
		if now > self.next_epoch_ts {
			let n_epoch_durations = now
				.safe_sub(self.next_epoch_ts)?
				.safe_div(EPOCH_DURATION)?
				.safe_add(1)?;

			self.next_epoch_ts = self.next_epoch_ts.safe_add(
				EPOCH_DURATION.safe_mul(n_epoch_durations)?
			)?;

			self.current_epoch_transfer = 0;
		}

		Ok(())
	}

	pub fn validate_transfer(&self, requested_transfer: u128) -> NormalResult {
		let max_transfer = self.max_transfer_per_epoch.saturating_sub(
			self.current_epoch_transfer
		);

		validate!(
			requested_transfer < max_transfer,
			ErrorCode::DefaultError,
			"requested transfer {} exceeds max transfer {}",
			requested_transfer,
			max_transfer
		)?;

		Ok(())
	}
}
