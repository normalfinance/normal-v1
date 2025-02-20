use anchor_lang::prelude::*;

#[account(zero_copy(unsafe))]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct ReferrerName {
	pub authority: Pubkey,
	pub user: Pubkey,
	pub user_stats: Pubkey,
	pub name: [u8; 32],
}

impl Size for ReferrerName {
	const SIZE: usize = 136;
}
