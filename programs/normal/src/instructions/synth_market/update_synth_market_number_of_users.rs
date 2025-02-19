use anchor_lang::prelude::*;
use market::{ Market, MarketStatus };

use crate::error::ErrorCode;
use super::AdminUpdateMarket;

pub fn handle_update_market_number_of_users(
	ctx: Context<AdminUpdateMarket>,
	number_of_users: Option<u32>
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("market {}", market.market_index);

	if let Some(number_of_users) = number_of_users {
		msg!(
			"market.number_of_users: {:?} -> {:?}",
			market.number_of_users,
			number_of_users
		);
		market.number_of_users = number_of_users;
	} else {
		msg!("market.number_of_users: unchanged");
	}

	validate!(
		market.number_of_users >= market.number_of_users_with_base,
		ErrorCode::DefaultError,
		"number_of_users must be >= number_of_users_with_base "
	)?;

	Ok(())
}
