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
pub struct DepositCollateral<'info> {
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
}

pub fn handle_vault_deposit(
	ctx: Context<DepositCollateral>,
	amount: u128
) -> Result<()> {
	verify_position_authority_interface(
		&ctx.accounts.position_token_account,
		&ctx.accounts.position_authority
	)?;

	let clock = Clock::get()?;

	if amount == 0 {
		return Err(ErrorCode::LiquidityZero.into());
	}
	let liquidity_delta = math::amm::convert_to_liquidity_delta(amount, true)?;
	let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

	let update = controller::liquidity::calculate_modify_liquidity(
		&ctx.accounts.amm,
		&ctx.accounts.position,
		&ctx.accounts.tick_array_lower,
		&ctx.accounts.tick_array_upper,
		liquidity_delta,
		timestamp
	)?;

	controller::liquidity::sync_modify_liquidity_values(
		&mut ctx.accounts.amm,
		&mut ctx.accounts.position,
		&ctx.accounts.tick_array_lower,
		&ctx.accounts.tick_array_upper,
		update,
		timestamp
	)?;

	let (delta_synthetic, delta_quote) =
		controller::liquidity::calculate_liquidity_token_deltas(
			ctx.accounts.amm.tick_current_index,
			ctx.accounts.amm.sqrt_price,
			&ctx.accounts.position,
			liquidity_delta
		)?;

	controller::vault::deposit_collateral(&ctx.accounts.vault);

	// Mint vault.collateralization_ratio much synthetic to the AMM
	mint_synthetic_to_amm(
		authority,
		token_owner_account,
		token_vault,
		token_program,
		amount
	);

	// Send 1 - vault.default_liquidity_utilization much collateral to the Vault
	transfer_from_owner_to_vault(
		&ctx.accounts.position_authority,
		&ctx.accounts.token_owner_account_quote,
		&ctx.accounts.token_vault_quote,
		&ctx.accounts.token_program,
		delta_quote
	)?;

	// Send vault.default_liquidity_utilization much collateral to the AMM
	transfer_from_owner_to_amm(
		&ctx.accounts.position_authority,
		&ctx.accounts.token_owner_account_quote,
		&ctx.accounts.token_vault_quote,
		&ctx.accounts.token_program,
		delta_quote
	)?;

	Ok(())
}
