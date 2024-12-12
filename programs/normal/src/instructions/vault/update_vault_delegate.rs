use anchor_lang::prelude::*;

use crate::load_mut;

use super::UpdateVault;

pub fn handle_update_vault_delegate(
	ctx: Context<UpdateVault>,
	delegate: Pubkey
) -> Result<()> {
	let mut vault = load_mut!(ctx.accounts.vault)?;
	vault.delegate = delegate;
	Ok(())
}
