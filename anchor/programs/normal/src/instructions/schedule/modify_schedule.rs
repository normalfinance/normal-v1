use anchor_lang::prelude::*;

use crate::{ controller, load_mut, state::user::User };

#[derive(Accounts)]
pub struct ModifySchedule<'info> {
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	pub authority: Signer<'info>,
}

pub fn handle_modify_schedule<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, ModifySchedule<'info>>,
	order_id: Option<u32>,
	modify_order_params: ModifyOrderParams
) -> Result<()> {
	let clock = &Clock::get()?;

	let order_id = match order_id {
		Some(order_id) => order_id,
		None => load!(ctx.accounts.user)?.get_last_order_id(),
	};

	controller::schedule::modify_schedule(
		ModifyOrderId::OrderId(order_id),
		modify_order_params,
		&ctx.accounts.user,
		state,
		&perp_market_map,
		&spot_market_map,
		&mut oracle_map,
		clock
	)?;

	Ok(())
}
