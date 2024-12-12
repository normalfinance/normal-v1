use anchor_lang::prelude::*;
use vault::Vault;
use market::{ Market, MarketStatus, VaultsConfig };

use crate::{ error::ErrorCode, load_mut, state::*, validate, State };

#[derive(Accounts)]
#[instruction(feed_id : [u8; 32])]
pub struct InitVault<'info> {
	pub market: Box<Account<'info, Market>>,

	#[account(mut)]
	pub funder: Signer<'info>,

	#[account(mut)]
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
	#[account(
		init,
		seeds = [b"vault".as_ref(), state.number_of_markets.to_le_bytes().as_ref()],
		bump,
		payer = funder,
		space = Vault::LEN
	)]
	pub vault: Box<Account<'info, Vault>>,

	pub token_mint: Account<'info, Mint>,

	#[account(
		init,
		payer = funder,
		token::mint = token_mint,
		token::authority = amm
	)]
	pub token_vault: Box<Account<'info, TokenAccount>>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
	pub system_program: Program<'info, System>,
	pub rent: Sysvar<'info, Rent>,
}

pub fn handle_initialize_vault(
	ctx: Context<InitVault>,
	vault_index: u16
) -> Result<()> {
	let mut market = load_mut!(ctx.accounts.market)?;

	validate!(
		!matches!(market.status, MarketStatus::Initialized),
		ErrorCode::MarketBeingInitialized,
		"Market is being initialized"
	)?;

	let vault_pubkey = ctx.accounts.vault.to_account_info().key;
	let vault = &mut ctx.accounts.vault.load_init()?;

	**vault = Vault {
		pubkey: *vault_pubkey,
		authority: Pubkey::default(),
		delegate: Pubkey::default(),
		vault_index,
		market_index: market.market_index,
		collateral_loan_balance: 0,
		token_vault_collateral: ctx.accounts.token_vault_collateral.key(),
		status: 0,
		last_active_slot: 0,
		idle: false,
		collateralization_ratio: 0,
		synthetic_tokens_minted: 0,
		padding: [0; 12],
	};

	Ok(())
}
