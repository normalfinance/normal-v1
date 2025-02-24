use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };

use crate::state::{ synth_market::SynthMarket };

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct InitializeReward<'info> {
	#[account(mut)]
	pub market: AccountLoader<'info, Market>, // TODO: do we need Box<>?

	#[account(address = amm.reward_infos[reward_index as usize].authority)]
	pub reward_authority: Signer<'info>,

	#[account(mut)]
	pub funder: Signer<'info>,

	pub reward_mint: Box<Account<'info, Mint>>,

	#[account(
		init,
		payer = funder,
		token::mint = reward_mint,
		token::authority = market
	)]
	pub reward_vault: Box<Account<'info, TokenAccount>>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
	pub system_program: Program<'info, System>,
	pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<InitializeReward>, reward_index: u8) -> Result<()> {
	let market = &mut ctx.accounts.market.load_init()?;

	let index: usize = reward_index as usize;

	if index >= NUM_REWARDS {
		return Err(ErrorCode::InvalidRewardIndex.into());
	}

	let lowest_index = match
		amm.reward_infos.iter().position(|r| !r.initialized())
	{
		Some(lowest_index) => lowest_index,
		None => {
			return Err(ErrorCode::InvalidRewardIndex.into());
		}
	};

	if lowest_index != index {
		return Err(ErrorCode::InvalidRewardIndex.into());
	}

	amm.reward_infos[index].mint = ctx.accounts.reward_mint.key();
	amm.reward_infos[index].vault = ctx.accounts.reward_vault.key();

	Ok(())
}
