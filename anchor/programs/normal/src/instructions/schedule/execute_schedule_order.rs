use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct ExecuteScheduleOrder<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_user(&filler, &authority)?
    )]
	pub filler: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&filler, &filler_stats)?
    )]
	pub filler_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
}

#[access_control(fill_not_paused(&ctx.accounts.state))]
pub fn handle_execute_schedule_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, ExecuteScheduleOrder<'info>>,
	order_id: Option<u32>
) -> Result<()> {
	let (order_id, market_index) = {
		let user = &load!(ctx.accounts.user)?;
		// if there is no order id, use the users last order id
		let order_id = order_id.unwrap_or_else(|| user.get_last_order_id());
		let market_index = match user.get_order(order_id) {
			Some(order) => order.market_index,
			None => {
				msg!("Order does not exist {}", order_id);
				return Ok(());
			}
		};
		(order_id, market_index)
	};

	let user_key = &ctx.accounts.user.key();
	execute_schedule(ctx, order_id, market_index).map_err(|e| {
		msg!(
			"Err filling order id {} for user {} for market index {}",
			order_id,
			user_key,
			market_index
		);
		e
	})?;

	Ok(())
}

fn execute_schedule<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, FillOrder<'info>>,
	order_id: u32,
	market_index: u16
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { perp_market_map, spot_market_map, mut oracle_map } =
		load_maps(
			remaining_accounts_iter,
			&get_writable_perp_market_set(market_index),
			&MarketSet::new(),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	controller::schedule::execute_schedule_order(
		order_id,
		&ctx.accounts.state,
		&ctx.accounts.user,
		&ctx.accounts.user_stats,
		&spot_market_map,
		&perp_market_map,
		&mut oracle_map,
		clock
	)?;

	Ok(())
}
