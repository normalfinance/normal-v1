use anchor_lang::prelude::*;

use super::UpdateUser;

pub fn handle_update_user_reduce_only(
	ctx: Context<UpdateUser>,
	_sub_account_id: u16,
	reduce_only: bool
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;

	validate!(!user.is_being_liquidated(), ErrorCode::LiquidationsOngoing)?;

	user.update_reduce_only_status(reduce_only)?;
	Ok(())
}
