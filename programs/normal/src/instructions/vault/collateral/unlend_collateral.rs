use amm::AMM;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };
use anchor_spl::token_interface::TokenAccount as TokenAccountInterface;
use vault::Vault;

use crate::errors::ErrorCode;
use crate::manager::liquidity_manager::{
	calculate_liquidity_token_deltas,
	calculate_modify_liquidity,
	sync_modify_liquidity_values,
};
use crate::math::{ self, convert_to_liquidity_delta };
use crate::{ controller, state::* };
use crate::util::{
	mint_synthetic_to_vault,
	to_timestamp_u64,
	transfer_from_owner_to_vault,
	verify_position_authority_interface,
};

#[derive(Accounts)]
pub struct UnlendCollateral<'info> {
	#[account(mut)]
	pub vault: Account<'info, Vault>,

	#[account(mut)]
	pub amm: Account<'info, AMM>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,

	pub position_authority: Signer<'info>,

	#[account(mut, has_one = amm)]
	pub position: Account<'info, Position>,
	#[account(
		constraint = position_token_account.mint == position.position_mint,
		constraint = position_token_account.amount == 1
	)]
	pub position_token_account: Box<
		InterfaceAccount<'info, TokenAccountInterface>
	>,

	#[account(mut, constraint = token_owner_account_quote.mint == amm.token_mint_quote)]
	pub token_owner_account_quote: Box<Account<'info, TokenAccount>>,

	#[account(mut, constraint = token_vault_synthetic.key() == amm.token_vault_synthetic)]
	pub token_vault_synthetic: Box<Account<'info, TokenAccount>>,
	#[account(mut, constraint = token_vault_quote.key() == amm.token_vault_quote)]
	pub token_vault_quote: Box<Account<'info, TokenAccount>>,

	#[account(mut, has_one = amm)]
	pub tick_array_lower: AccountLoader<'info, TickArray>,
	#[account(mut, has_one = amm)]
	pub tick_array_upper: AccountLoader<'info, TickArray>,

	/// CHECK: Kamino's program account
	#[account(mut, owner = kamino_program::id())]
	pub kamino_program: AccountInfo<'info>,
}

pub fn handle_unlend_collateral(
	ctx: Context<UnlendCollateral>,
	amount: u128
) -> Result<()> {
	
	Ok(())
}
