use crate::{ state::synth_market::{ SynthMarket }, State };

pub mod initialize_synth_market;
pub mod update_synth_market_amm;
pub mod update_synth_market_liquidation_penalty;
pub mod update_synth_market_debt_ceiling;
pub mod update_synth_market_debt_floor;
pub mod update_synth_market_margin_ratio;
pub mod update_synth_market_synthetic_tier;
pub mod update_synth_market_paused_operations;
pub mod update_synth_market_status;
pub mod update_synth_market_name;
pub mod update_synth_market_liquidation_fee;
pub mod initialize_synth_market_shutdown;
pub mod delete_initialized_synth_market;
pub mod update_synth_market_imf_factor;
pub mod update_synth_market_oracle;
pub mod freeze_synth_market_oracle;
pub mod update_synth_market_number_of_users;
pub mod update_synth_market_expiry;

#[derive(Accounts)]
pub struct AdminUpdateSynthMarket<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub synth_market: AccountLoader<'info, SynthMarket>,
}
