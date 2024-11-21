use crate::{
	errors::ErrorCode,
	math::{
		self,
		get_amount_delta_quote,
		get_amount_delta_synthetic,
		sqrt_price_from_tick_index,
	},
	state::*,
};
use amm::AMM;
use position::Position;
use tick::TickArray;
use crate::controller::*;
use anchor_lang::prelude::{ AccountLoader, * };

pub fn deposit_collateral<'info>(vault: &Vault) -> Result<> {
	Ok(())
}

pub fn withdraw_collateral<'info>(vault: &Vault) -> Result<> {
	Ok(())
}
