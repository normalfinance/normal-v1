use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use crate::controller::position::PositionDirection;
use crate::error::{ NormalResult, ErrorCode::InvalidOrder };
use crate::math::casting::Cast;
use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::traits::Size;
use crate::state::user::{ MarketType, Order };
use anchor_lang::Discriminator;
use std::io::Write;

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
	pub canceled_order_ids: Vec<u32>,
	pub liquidate_vault: LiquidateVaultRecord,
	pub liquidate_borrow_for_perp_pnl: LiquidateBorrowForPerpPnlRecord,
	pub liquidate_perp_pnl_for_deposit: LiquidatePerpPnlForDepositRecord,
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
	pub oracle_price: i64,
	pub base_asset_amount: i64,
	pub quote_asset_amount: i64,
	/// precision: AMM_RESERVE_PRECISION
	pub lp_shares: u64,
	pub fill_record_id: u64,
	pub user_order_id: u32,
	pub liquidator_order_id: u32,
	/// precision: QUOTE_PRECISION
	pub liquidator_fee: u64,
	/// precision: QUOTE_PRECISION
	pub if_fee: u64,
}
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct VaultBankruptcyRecord {
	pub market_index: u16,
	pub pnl: i128,
	pub if_payment: u128,
	pub clawback_user: Option<Pubkey>,
	pub clawback_user_payment: Option<u128>,
	pub cumulative_funding_rate_delta: i128,
}

#[event]
#[derive(Default)]
pub struct InsuranceFundRecord {
	pub ts: i64,
	pub spot_market_index: u16,
	pub perp_market_index: u16,
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
	pub market_index: u16,

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
