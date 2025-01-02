use crate::{ state::insurance::InsuranceFund, State };

pub mod initialize_insurance_fund;
pub mod update_if_unstaking_period;
pub mod update_if_max_insurance;
pub mod update_if_paused_operations;
pub mod settle_revenue_to_if;
pub mod staker;

#[derive(Accounts)]
pub struct AdminUpdateInsurnaceFund<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
}
