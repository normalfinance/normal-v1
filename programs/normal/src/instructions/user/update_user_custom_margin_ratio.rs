use anchor_lang::prelude::*;

use super::UpdateUser;

pub fn handle_update_user_custom_margin_ratio(
	ctx: Context<UpdateUser>,
	_sub_account_id: u16,
	margin_ratio: u32
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;
	user.max_margin_ratio = margin_ratio;
	Ok(())
}
