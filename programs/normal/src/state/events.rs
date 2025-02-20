use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use crate::errors::{ NormalResult};
use crate::math::safe_unwrap::SafeUnwrap;
use anchor_lang::Discriminator;
use std::io::Write;

#[event]
pub struct NewUserRecord {
	/// unix_timestamp of action
	pub ts: i64,
	pub user_authority: Pubkey,
	pub user: Pubkey,
	pub sub_account_id: u16,
	pub name: [u8; 32],
	pub referrer: Pubkey,
}

#[event]
pub struct DepositRecord {
	/// unix_timestamp of action
	pub ts: i64,
	pub user_authority: Pubkey,
	/// user account public key
	pub user: Pubkey,
	pub deposit_record_id: u64,
	/// precision: token mint precision
	pub amount: u64,
	/// spot market index
	pub market_index: u16,
	/// precision: PRICE_PRECISION
	pub oracle_price: i64,
	/// precision: SPOT_BALANCE_PRECISION
	pub market_deposit_balance: u128,
	/// precision: SPOT_BALANCE_PRECISION
	pub market_withdraw_balance: u128,
	/// precision: SPOT_CUMULATIVE_INTEREST_PRECISION
	pub market_cumulative_deposit_interest: u128,
	/// precision: SPOT_CUMULATIVE_INTEREST_PRECISION
	pub market_cumulative_borrow_interest: u128,
	/// precision: QUOTE_PRECISION
	pub total_deposits_after: u64,
	/// precision: QUOTE_PRECISION
	pub total_withdraws_after: u64,
	pub explanation: DepositExplanation,
	pub transfer_user: Option<Pubkey>,
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Eq, Default)]
pub enum DepositExplanation {
	#[default]
	None,
	Transfer,
	// Borrow,
	// RepayBorrow,
}

#[event]
#[derive(Default)]
pub struct LiquidationRecord {
	pub ts: i64,
	pub liquidation_type: LiquidationType,
	pub user: Pubkey,
	pub liquidator: Pubkey,
	pub margin_requirement: u128,
	pub total_collateral: i128,
	pub margin_freed: u64,
	pub liquidation_id: u16,
	pub bankrupt: bool,
	pub liquidate_vault: LiquidateVaultRecord,
	pub vault_bankruptcy: VaultBankruptcyRecord,
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Eq, Default)]
pub enum LiquidationType {
	#[default]
	LiquidateVault,
	VaultBankruptcy,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct LiquidateVaultRecord {
	pub market_index: u16,
	pub vault_index: u16,
	pub oracle_price: i64,
	pub base_asset_amount: i64,
	pub quote_asset_amount: i64,
	/// precision: QUOTE_PRECISION
	pub liquidator_fee: u64,
	/// precision: QUOTE_PRECISION
	pub if_fee: u64,
}
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct VaultBankruptcyRecord {
	pub market_index: u16,
	pub vault_index: u16,
	pub pnl: i128,
	pub if_payment: u128,
	pub clawback_user: Option<Pubkey>,
	pub clawback_user_payment: Option<u128>,
}

// Insurance events

#[event]
#[derive(Default)]
pub struct InsuranceFundRecord {
	pub ts: i64,
	pub market_index: u16,
	/// precision: PERCENTAGE_PRECISION
	pub user_if_factor: u32,
	/// precision: PERCENTAGE_PRECISION
	pub total_if_factor: u32,
	/// precision: token mint precision
	pub vault_amount_before: u64,
	/// precision: token mint precision
	pub insurance_vault_amount_before: u64,
	pub total_if_shares_before: u128,
	pub total_if_shares_after: u128,
	/// precision: token mint precision
	pub amount: i64,
}

#[event]
#[derive(Default)]
pub struct InsuranceFundStakeRecord {
	pub ts: i64,
	pub user_authority: Pubkey,
	pub action: StakeAction,
	/// precision: token mint precision
	pub amount: u64,

	/// precision: token mint precision
	pub insurance_vault_amount_before: u64,
	pub if_shares_before: u128,
	pub user_if_shares_before: u128,
	pub total_if_shares_before: u128,
	pub if_shares_after: u128,
	pub user_if_shares_after: u128,
	pub total_if_shares_after: u128,
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Eq, Default)]
pub enum StakeAction {
	#[default]
	Stake,
	UnstakeRequest,
	UnstakeCancelRequest,
	Unstake,
	UnstakeTransfer,
	StakeTransfer,
}

pub fn emit_stack<T: AnchorSerialize + Discriminator, const N: usize>(
	event: T
) -> NormalResult {
	let mut data_buf = [0u8; N];
	let mut out_buf = [0u8; N];

	emit_buffers(event, &mut data_buf[..], &mut out_buf[..])
}

pub fn emit_buffers<T: AnchorSerialize + Discriminator>(
	event: T,
	data_buf: &mut [u8],
	out_buf: &mut [u8]
) -> NormalResult {
	let mut data_writer = std::io::Cursor::new(data_buf);
	data_writer.write_all(&<T as Discriminator>::discriminator()).safe_unwrap()?;
	borsh::to_writer(&mut data_writer, &event).safe_unwrap()?;
	let data_len = data_writer.position() as usize;

	let out_len = base64::encode_config_slice(
		&data_writer.into_inner()[0..data_len],
		base64::STANDARD,
		out_buf
	);

	let msg_bytes = &out_buf[0..out_len];
	let msg_str = unsafe { std::str::from_utf8_unchecked(msg_bytes) };

	msg!(msg_str);

	Ok(())
}
