use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };
use anchor_spl::token_interface::TokenAccount as TokenAccountInterface;

use amm::AMM;
use crate::state::liquidity_position::LiquidityPosition;
use crate::util::{
	transfer_from_vault_to_owner,
	verify_position_authority_interface,
};

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct CollectLiquidityPositionReward<'info> {
	pub amm: Box<Account<'info, AMM>>,

	pub position_authority: Signer<'info>,

	#[account(mut, has_one = amm)]
	pub position: Box<Account<'info, LiquidityPosition>>,
	#[account(
		constraint = position_token_account.mint == position.position_mint,
		constraint = position_token_account.amount == 1
	)]
	pub position_token_account: Box<
		InterfaceAccount<'info, TokenAccountInterface>
	>,

	#[account(mut,
        constraint = reward_owner_account.mint == amm.reward_infos[reward_index as usize].mint
    )]
	pub reward_owner_account: Box<Account<'info, TokenAccount>>,

	#[account(mut, address = amm.reward_infos[reward_index as usize].vault)]
	pub reward_vault: Box<Account<'info, TokenAccount>>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
}

/// Collects all harvestable tokens for a specified reward.
///
/// If the AMM reward vault does not have enough tokens, the maximum number of available
/// tokens will be debited to the user. The unharvested amount remains tracked, and it can be
/// harvested in the future.
///
/// # Parameters
/// - `reward_index` - The reward to harvest. Acceptable values are 0, 1, and 2.
///
/// # Returns
/// - `Ok`: Reward tokens at the specified reward index have been successfully harvested
/// - `Err`: `RewardNotInitialized` if the specified reward has not been initialized
///          `InvalidRewardIndex` if the reward index is not 0, 1, or 2
pub fn handle_collect_liquidity_position_reward(
	ctx: Context<CollectLiquidityPositionReward>,
	reward_index: u8
) -> Result<()> {
	verify_position_authority_interface(
		&ctx.accounts.position_token_account,
		&ctx.accounts.position_authority
	)?;

	let index = reward_index as usize;

	let position = &mut ctx.accounts.position;
	let (transfer_amount, updated_amount_owed) = calculate_collect_reward(
		position.reward_infos[index],
		ctx.accounts.reward_vault.amount
	);

	position.update_reward_owed(index, updated_amount_owed);

	transfer_from_vault_to_owner(
		&ctx.accounts.amm,
		&ctx.accounts.reward_vault,
		&ctx.accounts.reward_owner_account,
		&ctx.accounts.token_program,
		transfer_amount
	)
}

fn calculate_collect_reward(
	position_reward: PositionRewardInfo,
	vault_amount: u64
) -> (u64, u64) {
	let amount_owed = position_reward.amount_owed;
	let (transfer_amount, updated_amount_owed) = if amount_owed > vault_amount {
		(vault_amount, amount_owed - vault_amount)
	} else {
		(amount_owed, 0)
	};

	(transfer_amount, updated_amount_owed)
}
