use anchor_lang::prelude::*;

use super::UpdateUser;

pub fn handle_update_user_delegate(
	ctx: Context<UpdateUser>,
	_sub_account_id: u16,
	delegate: Pubkey
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;
	user.delegate = delegate;
	Ok(())
}
