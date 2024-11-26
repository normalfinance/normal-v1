use anchor_lang::prelude::*;
use vault::Vault;
use market::VaultsConfig;

use crate::{ state::* };

#[derive(Accounts)]
#[instruction(feed_id : [u8; 32])]
pub struct InitVault<'info> {
	pub vaults_config: Box<Account<'info, VaultsConfig>>,

	#[account(mut)]
	pub funder: Signer<'info>,

	#[account(
		init,
		seeds = [
			b"vault".as_ref(),
			vaults_config.key().as_ref(),
			token_mint_synthetic.key().as_ref(),
			token_mint_quote.key().as_ref(),
			tick_spacing.to_le_bytes().as_ref(),
		],
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

pub fn handle_initialize_vault(ctx: Context<InitVault>) -> Result<()> {
	let vault = &mut ctx.accounts.vault;

	vault.initialize();

	Ok(())
}
