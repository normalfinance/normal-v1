#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct UpdateIndexFund<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"market", market_index.to_le_bytes().as_ref()],
        bump
    )]
	pub market: AccountLoader<'info, Market>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	pub oracle: AccountInfo<'info>,
	pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_set_fund_visibility(
	ctx: Context<UpdateIndexFund>,
	public: bool
) -> Result<()> {
	let market = ctx.accounts.market.load()?;
	msg!("market {}", market.market_index);

	validate!(
		market.is_index_fund_market() == true,
		ErrorCode::InvalidSpotMarketInitialization,
		"Must be Index Fund market"
	)?;

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	// TODO: validate can be rebalanced
	validate!(
		insurance_fund.user_factor <= insurance_fund.total_factor,
		ErrorCode::RevenueSettingsCannotSettleToIF,
		"invalid if_factor settings on market"
	)?;

	controller::fund::rebalance(market, now);

	Ok(())
}
