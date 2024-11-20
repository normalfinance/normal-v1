use anchor_lang::prelude::*;

use crate::{ errors::ErrorCode, math::MAX_PROTOCOL_FEE_RATE };

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct InsuranceFund {
	pub vault: Pubkey,
	pub total_shares: u128,
	pub user_shares: u128,
	pub shares_base: u128, // exponent for lp shares (for rebasing)
	pub unstaking_period: i64, // if_unstaking_period
	pub last_revenue_settle_ts: i64,
	pub revenue_settle_period: i64,
	pub total_factor: u32, // percentage of interest for total insurance
	pub user_factor: u32, // percentage of interest for user staked insurance
}
