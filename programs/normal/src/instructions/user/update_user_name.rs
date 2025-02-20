use anchor_lang::prelude::*;

use crate::load_mut;

use super::initialize_user::UpdateUser;

pub fn handle_update_user_name(
	ctx: Context<UpdateUser>,
	_sub_account_id: u16,
	name: [u8; 32]
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;
	user.name = name;
	Ok(())
}
