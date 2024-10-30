use anchor_lang::prelude::*;

use crate::state::amm::AMM;

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct SetAMMRewardAuthority<'info> {
	#[account(mut)]
	pub amm: Account<'info, AMM>,

	#[account(address = amm.reward_infos[reward_index as usize].authority)]
	pub reward_authority: Signer<'info>,

	/// CHECK: safe, the account that will be new authority can be arbitrary
	pub new_reward_authority: UncheckedAccount<'info>,
}

pub fn handle_set_amm_reward_authority(
	ctx: Context<SetAMMRewardAuthority>,
	reward_index: u8
) -> Result<()> {
	ctx.accounts.amm.update_reward_authority(
		reward_index as usize,
		ctx.accounts.new_reward_authority.key()
	)
}
