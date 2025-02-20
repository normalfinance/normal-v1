use anchor_lang::prelude::*;

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct UserFees {
	/// Total taker fee paid
	/// precision: QUOTE_PRECISION
	pub total_fee_paid: u64,
	/// Total maker fee rebate
	/// precision: QUOTE_PRECISION
	pub total_fee_rebate: u64,
	/// Total discount from holding token
	/// precision: QUOTE_PRECISION
	pub total_token_discount: u64,
	/// Total discount from being referred
	/// precision: QUOTE_PRECISION
	pub total_referee_discount: u64,
	/// Total reward to referrer
	/// precision: QUOTE_PRECISION
	pub total_referrer_reward: u64,
	/// Total reward to referrer this epoch
	/// precision: QUOTE_PRECISION
	pub current_epoch_referrer_reward: u64,
}

#[account(zero_copy(unsafe))]
#[derive(Eq, PartialEq, Debug)]
#[repr(C)]
#[derive(Default)]
pub struct UserStats {
	/// The authority for all of a users sub accounts
	pub authority: Pubkey,
	/// The address that referred this user
	pub referrer: Pubkey,
	/// Stats on the fees paid by the user
	pub fees: UserFees,
	/// The timestamp of the next epoch
	/// Epoch is used to limit referrer rewards earned in single epoch
	pub next_epoch_ts: i64,
	/// Rolling 30day maker volume for user
	/// precision: QUOTE_PRECISION
	pub maker_volume_30d: u64,
	/// Rolling 30day taker volume for user
	/// precision: QUOTE_PRECISION
	pub taker_volume_30d: u64,
	/// last time the maker volume was updated
	pub last_maker_volume_30d_ts: i64,
	/// last time the taker volume was updated
	pub last_taker_volume_30d_ts: i64,
	/// The amount of tokens staked in the if
	pub if_staked_asset_amount: u64,
	/// The current number of sub accounts
	pub number_of_sub_accounts: u16,
	/// The number of sub accounts created. Can be greater than the number of sub accounts if user
	/// has deleted sub accounts
	pub number_of_sub_accounts_created: u16,
	/// Whether the user is a referrer. Sub account 0 can not be deleted if user is a referrer
	pub is_referrer: bool,
	pub disable_update_perp_bid_ask_twap: bool,
	pub padding: [u8; 12],
}

impl Size for UserStats {
	const SIZE: usize = 168;
}

impl UserStats {
	pub fn update_maker_volume_30d(
		&mut self,
		quote_asset_amount: u64,
		now: i64
	) -> NormalResult {
		let since_last = max(1_i64, now.safe_sub(self.last_maker_volume_30d_ts)?);

		self.maker_volume_30d = calculate_rolling_sum(
			self.maker_volume_30d,
			quote_asset_amount,
			since_last,
			THIRTY_DAY
		)?;
		self.last_maker_volume_30d_ts = now;

		Ok(())
	}

	pub fn update_taker_volume_30d(
		&mut self,
		quote_asset_amount: u64,
		now: i64
	) -> NormalResult {
		let since_last = max(1_i64, now.safe_sub(self.last_taker_volume_30d_ts)?);

		self.taker_volume_30d = calculate_rolling_sum(
			self.taker_volume_30d,
			quote_asset_amount,
			since_last,
			THIRTY_DAY
		)?;
		self.last_taker_volume_30d_ts = now;

		Ok(())
	}

	pub fn increment_total_fees(&mut self, fee: u64) -> NormalResult {
		self.fees.total_fee_paid = self.fees.total_fee_paid.safe_add(fee)?;

		Ok(())
	}

	pub fn increment_total_rebate(&mut self, fee: u64) -> NormalResult {
		self.fees.total_fee_rebate = self.fees.total_fee_rebate.safe_add(fee)?;

		Ok(())
	}

	pub fn increment_total_referrer_reward(
		&mut self,
		reward: u64,
		now: i64
	) -> NormalResult {
		self.fees.total_referrer_reward =
			self.fees.total_referrer_reward.safe_add(reward)?;

		self.fees.current_epoch_referrer_reward =
			self.fees.current_epoch_referrer_reward.safe_add(reward)?;

		if now > self.next_epoch_ts {
			let n_epoch_durations = now
				.safe_sub(self.next_epoch_ts)?
				.safe_div(EPOCH_DURATION)?
				.safe_add(1)?;

			self.next_epoch_ts = self.next_epoch_ts.safe_add(
				EPOCH_DURATION.safe_mul(n_epoch_durations)?
			)?;

			self.fees.current_epoch_referrer_reward = 0;
		}

		Ok(())
	}

	pub fn increment_total_referee_discount(
		&mut self,
		discount: u64
	) -> NormalResult {
		self.fees.total_referee_discount =
			self.fees.total_referee_discount.safe_add(discount)?;

		Ok(())
	}

	pub fn has_referrer(&self) -> bool {
		!self.referrer.eq(&Pubkey::default())
	}

	pub fn get_total_30d_volume(&self) -> NormalResult<u64> {
		self.taker_volume_30d.safe_add(self.maker_volume_30d)
	}

	pub fn get_age_ts(&self, now: i64) -> i64 {
		// upper bound of age of the user stats account
		let min_action_ts: i64 = self.last_filler_volume_30d_ts
			.min(self.last_maker_volume_30d_ts)
			.min(self.last_taker_volume_30d_ts);
		now.saturating_sub(min_action_ts).max(0)
	}
}
