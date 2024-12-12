#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct RebalanceIndexFund<'info> {
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

pub fn handle_rebalance_fund(
	ctx: Context<RebalanceIndexFund>,
	market_index: u16
) -> Result<()> {
	Ok(())
}
