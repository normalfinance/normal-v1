use crate::error::{ NormalResult, ErrorCode };
use crate::state::spot_market::SpotBalanceType;
use crate::state::user::{ OrderStatus, User, UserStats };
use crate::state::vault::Vault;
use crate::{ validate, State, THIRTEEN_DAY };
use solana_program::msg;

pub fn validate_vault_is_idle(
	vault: &Vault,
	slot: u64,
	accelerated: bool
) -> NormalResult {
	let slots_since_last_active = slot.saturating_sub(vault.last_active_slot);

	let slots_before_idle = if accelerated {
		9000_u64 // 60 * 60 / .4 (~1 hour)
	} else {
		1512000_u64 // 60 * 60 * 24 * 7 / .4 (~1 week)
	};

	validate!(
		slots_since_last_active >= slots_before_idle,
		ErrorCode::UserNotInactive,
		"vault only been idle for {} slot",
		slots_since_last_active
	)?;

	validate!(
		!vault.is_bankrupt(),
		ErrorCode::UserNotInactive,
		"vault bankrupt"
	)?;

	// TODO: update to check collateral
	for position in &vault.positions {
		validate!(
			position.is_available(),
			ErrorCode::UserNotInactive,
			"vault has position for market {}",
			position.market_index
		)?;
	}

	Ok(())
}
