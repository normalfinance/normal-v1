use anchor_lang::prelude::*;

use crate::state::*;

#[derive(Accounts)]
#[instruction(feed_id : [u8; 32])]
pub struct AdminUpdateVaultConfig<'info> {
	pub vaults_config: Account<'info, VaultsConfig>,
}

pub fn handle_set_liquidation_penalty(
	ctx: Context<AdminUpdateVaultConfig>,
	new_liquidation_penalty: u64
) -> Result<()> {
	ctx.accounts.vaults_config.update_liquidation_penalty(new_liquidation_penalty)
}
