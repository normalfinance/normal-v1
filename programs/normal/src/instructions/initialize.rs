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
	let (drift_signer, drift_signer_nonce) = Pubkey::find_program_address(
		&[b"drift_signer".as_ref()],
		ctx.program_id
	);

	**ctx.accounts.state = State {
		admin: *ctx.accounts.admin.key,
		exchange_status: ExchangeStatus::active(),
		whitelist_mint: Pubkey::default(),
		discount_mint: Pubkey::default(),
		oracle_guard_rails: OracleGuardRails::default(),
		number_of_authorities: 0,
		number_of_sub_accounts: 0,
		number_of_markets: 0,
		number_of_spot_markets: 0,
		min_perp_auction_duration: 10,
		default_market_order_time_in_force: 60,
		default_spot_auction_duration: 10,
		liquidation_margin_buffer_ratio: DEFAULT_LIQUIDATION_MARGIN_BUFFER_RATIO,
		settlement_duration: 0, // extra duration after market expiry to allow settlement
		signer: drift_signer,
		signer_nonce: drift_signer_nonce,
		srm_vault: Pubkey::default(),
		perp_fee_structure: FeeStructure::perps_default(),
		spot_fee_structure: FeeStructure::spot_default(),
		lp_cooldown_time: 0,
		liquidation_duration: 0,
		initial_pct_to_liquidate: 0,
		max_number_of_sub_accounts: 0,
		max_initialize_user_fee: 0,
		padding: [0; 10],
	};

	Ok(())
}
