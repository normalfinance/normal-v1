use anchor_lang::prelude::*;

use crate::errors::ErrorCode;

use super::{ market::Market, tick::Tick };

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Copy)]
pub struct OpenLiquidityPositionWithMetadataBumps {
	pub position_bump: u8,
	pub metadata_bump: u8,
}

#[account]
#[derive(Default)]
pub struct LiquidityPosition {
	pub market: Pubkey,
	pub position_mint: Pubkey,

	pub liquidity: u128,
	pub tick_lower_index: i32,
	pub tick_upper_index: i32,

	// Q64.64
	pub fee_growth_checkpoint_a: u128,
	pub fee_owed_a: u64,

	// Q64.64
	pub fee_growth_checkpoint_b: u128,
	pub fee_owed_b: u64,

	pub reward_infos: [LiquidityPositionRewardInfo; NUM_REWARDS], // 72
}

impl LiquidityPosition {
	pub const LEN: usize = 8 + 136 + 72;

	pub fn is_position_empty(position: &LiquidityPosition) -> bool {
		let fees_not_owed = position.fee_owed_a == 0 && position.fee_owed_b == 0;
		let mut rewards_not_owed = true;
		for i in 0..NUM_REWARDS {
			rewards_not_owed =
				rewards_not_owed && position.reward_infos[i].amount_owed == 0;
		}
		position.liquidity == 0 && fees_not_owed && rewards_not_owed
	}

	pub fn update(&mut self, update: &LiquidityPositionUpdate) {
		self.liquidity = update.liquidity;
		self.fee_growth_checkpoint_a = update.fee_growth_checkpoint_a;
		self.fee_growth_checkpoint_b = update.fee_growth_checkpoint_b;
		self.fee_owed_a = update.fee_owed_a;
		self.fee_owed_b = update.fee_owed_b;
		self.reward_infos = update.reward_infos;
	}

	pub fn open_position(
		&mut self,
		market: &Account<Market>,
		position_mint: Pubkey,
		tick_lower_index: i32,
		tick_upper_index: i32
	) -> Result<()> {
		if
			!Tick::check_is_usable_tick(tick_lower_index, market.amm.tick_spacing) ||
			!Tick::check_is_usable_tick(tick_upper_index, amm.tick_spacing) ||
			tick_lower_index >= tick_upper_index
		{
			return Err(ErrorCode::InvalidTickIndex.into());
		}

		// On tick spacing >= 2^15, should only be able to open full range positions
		if amm.tick_spacing >= FULL_RANGE_ONLY_TICK_SPACING_THRESHOLD {
			let (full_range_lower_index, full_range_upper_index) =
				Tick::full_range_indexes(amm.tick_spacing);
			if
				tick_lower_index != full_range_lower_index ||
				tick_upper_index != full_range_upper_index
			{
				return Err(ErrorCode::FullRangeOnlyPool.into());
			}
		}

		self.market = market.key();
		self.position_mint = position_mint;

		self.tick_lower_index = tick_lower_index;
		self.tick_upper_index = tick_upper_index;
		Ok(())
	}

	pub fn reset_fees_owed(&mut self) {
		self.fee_owed_a = 0;
		self.fee_owed_b = 0;
	}

	pub fn update_reward_owed(&mut self, index: usize, amount_owed: u64) {
		self.reward_infos[index].amount_owed = amount_owed;
	}
}

#[derive(
	Copy,
	Clone,
	AnchorSerialize,
	AnchorDeserialize,
	Default,
	Debug,
	PartialEq
)]
pub struct LiquidityPositionRewardInfo {
	// Q64.64
	pub growth_inside_checkpoint: u128,
	pub amount_owed: u64,
}

#[derive(Default, Debug, PartialEq)]
pub struct LiquidityPositionUpdate {
	pub liquidity: u128,
	pub fee_growth_checkpoint_a: u128,
	pub fee_owed_a: u64,
	pub fee_growth_checkpoint_b: u128,
	pub fee_owed_b: u64,
	pub reward_infos: [LiquidityPositionRewardInfo; NUM_REWARDS],
}
