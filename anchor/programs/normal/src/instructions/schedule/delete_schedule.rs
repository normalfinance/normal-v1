#[derive(Accounts)]
pub struct CancelOrder<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	pub authority: Signer<'info>,
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_cancel_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, CancelOrder>,
	order_id: Option<u32>
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let AccountMaps { perp_market_map, spot_market_map, mut oracle_map } =
		load_maps(
			&mut ctx.remaining_accounts.iter().peekable(),
			&MarketSet::new(),
			&MarketSet::new(),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	let order_id = match order_id {
		Some(order_id) => order_id,
		None => load!(ctx.accounts.user)?.get_last_order_id(),
	};

	controller::schedule::delete_schedule(
		order_id,
		&ctx.accounts.user,
		&perp_market_map,
		&spot_market_map,
		&mut oracle_map,
		clock
	)?;

	Ok(())
}
