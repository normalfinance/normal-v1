use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use crate::controller::position::OrderSide;
use crate::error::{ NormalResult, ErrorCode::InvalidOrder };
use crate::math::casting::Cast;
use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::traits::Size;
use crate::state::user::{ MarketType, Order };
use anchor_lang::Discriminator;
use std::io::Write;



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
