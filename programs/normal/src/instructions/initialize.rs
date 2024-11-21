use crate::State;

#[derive(Accounts)]
pub struct Initialize<'info> {
	#[account(mut)]
	pub admin: Signer<'info>,
	#[account(
		init,
		seeds = [b"normal_state".as_ref()],
		space = State::SIZE,
		bump,
		payer = admin
	)]
	pub state: Box<Account<'info, State>>,
	/// CHECK: checked in `initialize`
	pub normal_signer: AccountInfo<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
	pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_initialize(ctx: Context<Initialize>) -> Result<()> {
	let (normal_signer, normal_signer_nonce) = Pubkey::find_program_address(
		&[b"normal_signer".as_ref()],
		ctx.program_id
	);

	**ctx.accounts.state = State {
		admin: *ctx.accounts.admin.key,
		signer: normal_signer,
		oracle_guard_rails: OracleGuardRails::default(),
		signer_nonce: normal_signer_nonce,
		min_collateral_auction_duration: 10,
		default_auction_duration: 10,
		exchange_status: ExchangeStatus::active(),
		padding: [0; 10],
	};

	Ok(())
}
