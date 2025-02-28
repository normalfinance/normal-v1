pub mod initialize_state;
pub mod update_state_admin;
pub mod update_state_initial_pct_to_liquidate;
pub mod update_state_liquidation_duration;
pub mod update_state_liquidation_margin_buffer_ratio;
pub mod update_oracle_guard_rails;
pub mod update_state_max_initialize_user_fee;
pub mod update_state_max_number_of_sub_accounts;
pub mod update_state_exchange_status;
pub mod update_state_protocol_index_fee;

#[derive(Accounts)]
pub struct AdminUpdateState<'info> {
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
}
