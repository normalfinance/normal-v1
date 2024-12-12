use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenInterface;

use crate::{ state::market::AuctionConfig, State };

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

pub fn handle_initialize_state(
	ctx: Context<Initialize>,
	total_debt_ceiling: u64
) -> Result<()> {
	let (normal_signer, normal_signer_nonce) = Pubkey::find_program_address(
		&[b"normal_signer".as_ref()],
		ctx.program_id
	);

	**ctx.accounts.state = State {
		admin: *ctx.accounts.admin.key,
		exchange_status: ExchangeStatus::active(),
		oracle_guard_rails: OracleGuardRails::default(),
		number_of_authorities: 0,
		number_of_sub_accounts: 0,
		number_of_markets: 0,
		number_of_vaults: 0,
		liquidation_margin_buffer_ratio: DEFAULT_LIQUIDATION_MARGIN_BUFFER_RATIO,
		signer: normal_signer,
		signer_nonce: normal_signer_nonce,
		liquidation_duration: 0,
		initial_pct_to_liquidate: 0,
		max_number_of_sub_accounts: 0,
		max_initialize_user_fee: 0,
		total_debt_ceiling,
		debt_auction_config: AuctionConfig::default(),
		insurance_fund: Pubkey::default(), // TODO: fix
		emergency_oracles: [], // TODO: fix
		padding: [0; 10],
	};

	Ok(())
}
