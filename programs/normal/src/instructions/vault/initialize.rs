use anchor_lang::prelude::*;

use crate::state::*;

#[derive(Accounts)]
#[instruction(feed_id : [u8; 32])]
pub struct InitVault<'info> {
	#[account(mut)]
	pub admin: Signer<'info>,

	#[account(init, payer = admin, space = VaultsConfig::LEN)]
	pub config: Account<'info, VaultsConfig>,

	pub system_program: Program<'info, System>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
}

pub fn handle_initialize_vault(ctx: Context<InitVault>) -> Result<()> {
	let vault = &mut ctx.accounts.vault;

	vault.initialize(
		fee_authority,
		collect_protocol_fees_authority,
		reward_emissions_super_authority,
		default_protocol_fee_rate
	);

	mint_position_token_and_remove_authority(
        amm,
        position_mint,
        &ctx.accounts.position_token_account,
        &ctx.accounts.token_program,
    )

	Ok(())
}
