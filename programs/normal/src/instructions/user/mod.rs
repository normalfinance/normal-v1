use crate::state::user::User;

pub mod initialize_referrer_name;
pub mod initialize_user_stats;
pub mod initialize_user;
pub mod reclaim_rent;
pub mod update_user_name;
pub mod delete_user;
pub mod update_user_delegate;
pub mod update_user_reduce_only;
pub mod update_user_custom_margin_ratio;

#[derive(Accounts)]
#[instruction(
    sub_account_id: u16,
)]
pub struct UpdateUser<'info> {
	#[account(
        mut,
        seeds = [b"user", authority.key.as_ref(), sub_account_id.to_le_bytes().as_ref()],
        bump,
    )]
	pub user: AccountLoader<'info, User>,
	pub authority: Signer<'info>,
}
