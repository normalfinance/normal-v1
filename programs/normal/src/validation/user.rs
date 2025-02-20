use crate::errors::{ NormalResult, ErrorCode };
use crate::math::constants::THIRTEEN_DAY;
use crate::state::user::User;
use crate::state::user_stats::UserStats;
use crate::{ validate, State };
use solana_program::msg;

pub fn validate_user_deletion(
	user: &User,
	user_stats: &UserStats,
	state: &State,
	now: i64
) -> NormalResult {
	validate!(
		!user_stats.is_referrer || user.sub_account_id != 0,
		ErrorCode::UserCantBeDeleted,
		"user id 0 cant be deleted if user is a referrer"
	)?;

	validate!(
		!user.is_bankrupt(),
		ErrorCode::UserCantBeDeleted,
		"user bankrupt"
	)?;

	validate!(
		!user.is_being_liquidated(),
		ErrorCode::UserCantBeDeleted,
		"user being liquidated"
	)?;

	// for position in &user.positions {
	// 	validate!(
	// 		position.is_available(),
	// 		ErrorCode::UserCantBeDeleted,
	// 		"user has position for market {}",
	// 		position.market_index
	// 	)?;
	// }

	if state.max_initialize_user_fee > 0 {
		let estimated_user_stats_age = user_stats.get_age_ts(now);
		if estimated_user_stats_age < THIRTEEN_DAY {
			validate!(
				user.idle,
				ErrorCode::UserCantBeDeleted,
				"user is not idle with fresh user stats account creation ({} < {})",
				estimated_user_stats_age,
				THIRTEEN_DAY
			)?;
		}
	}

	Ok(())
}

pub fn validate_user_is_idle(
	user: &User,
	slot: u64,
	accelerated: bool
) -> NormalResult {
	let slots_since_last_active = slot.saturating_sub(user.last_active_slot);

	let slots_before_idle = if accelerated {
		9000_u64 // 60 * 60 / .4 (~1 hour)
	} else {
		1512000_u64 // 60 * 60 * 24 * 7 / .4 (~1 week)
	};

	validate!(
		slots_since_last_active >= slots_before_idle,
		ErrorCode::UserNotInactive,
		"user only been idle for {} slot",
		slots_since_last_active
	)?;

	validate!(!user.is_bankrupt(), ErrorCode::UserNotInactive, "user bankrupt")?;

	validate!(
		!user.is_being_liquidated(),
		ErrorCode::UserNotInactive,
		"user being liquidated"
	)?;

	// for position in &user.positions {
	// 	validate!(
	// 		position.is_available(),
	// 		ErrorCode::UserNotInactive,
	// 		"user has position for market {}",
	// 		position.market_index
	// 	)?;
	// }

	Ok(())
}
