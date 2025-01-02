#[derive(Accounts)]
#[instruction(feed_id : [u8; 32])]
pub struct InitPythPullPriceFeed<'info> {
	#[account(mut)]
	pub admin: Signer<'info>,
	pub pyth_solana_receiver: Program<'info, PythSolanaReceiver>,
	/// CHECK: This account's seeds are checked
	#[account(mut, seeds = [PTYH_PRICE_FEED_SEED_PREFIX, &feed_id], bump)]
	pub price_feed: AccountInfo<'info>,
	pub system_program: Program<'info, System>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
}

pub fn handle_initialize_pyth_pull_oracle(
	ctx: Context<InitPythPullPriceFeed>,
	feed_id: [u8; 32]
) -> Result<()> {
	let cpi_program = ctx.accounts.pyth_solana_receiver.to_account_info();
	let cpi_accounts = InitPriceUpdate {
		payer: ctx.accounts.admin.to_account_info(),
		price_update_account: ctx.accounts.price_feed.to_account_info(),
		system_program: ctx.accounts.system_program.to_account_info(),
		write_authority: ctx.accounts.price_feed.to_account_info(),
	};

	let seeds = &[
		PTYH_PRICE_FEED_SEED_PREFIX,
		feed_id.as_ref(),
		&[ctx.bumps.price_feed],
	];
	let signer_seeds = &[&seeds[..]];
	let cpi_context = CpiContext::new_with_signer(
		cpi_program,
		cpi_accounts,
		signer_seeds
	);

	pyth_solana_receiver_sdk::cpi::init_price_update(cpi_context, feed_id)?;

	Ok(())
}
