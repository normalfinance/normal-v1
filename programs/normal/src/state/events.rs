use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use crate::{errors::NormalResult, math::safe_unwrap::SafeUnwrap};
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

// Insurance events

#[event]
#[derive(Default)]
pub struct InsuranceFundRecord {
	pub ts: i64,
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
