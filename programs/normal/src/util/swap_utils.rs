use std::ptr::null;

use anchor_lang::prelude::*;
use anchor_spl::token::{ Token, TokenAccount };

use crate::{ manager::swap_manager::PostSwapUpdate, state::amm::AMM };

use super::{
	mint_synthetic_to_vault,
	transfer_from_owner_to_vault,
	transfer_from_vault_to_owner,
};

#[allow(clippy::too_many_arguments)]
pub fn update_and_swap_amm<'info>(
	amm: &mut Account<'info, AMM>,
	token_authority: &Signer<'info>,
	token_owner_account_synthetic: &Account<'info, TokenAccount>,
	token_owner_account_quote: &Account<'info, TokenAccount>,
	token_vault_synthetic: &Account<'info, TokenAccount>,
	token_vault_quote: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	swap_update: PostSwapUpdate,
	is_token_fee_in_synthetic: bool,
	reward_last_updated_timestamp: u64,
	inside_range: bool
) -> Result<()> {
	amm.update_after_swap(
		swap_update.next_liquidity,
		swap_update.next_tick_index,
		swap_update.next_sqrt_price,
		swap_update.next_fee_growth_global,
		swap_update.next_reward_infos,
		swap_update.next_protocol_fee,
		is_token_fee_in_synthetic,
		reward_last_updated_timestamp
	);

	perform_swap(
		amm,
		token_authority,
		token_owner_account_synthetic,
		token_owner_account_quote,
		token_vault_synthetic,
		token_vault_quote,
		token_program,
		swap_update.amount_synthetic,
		swap_update.amount_quote,
		is_token_fee_in_synthetic,
		inside_range
	)
}

#[allow(clippy::too_many_arguments)]
fn perform_swap<'info>(
	amm: &Account<'info, AMM>,
	token_authority: &Signer<'info>,
	token_owner_account_synthetic: &Account<'info, TokenAccount>,
	token_owner_account_quote: &Account<'info, TokenAccount>,
	token_vault_synthetic: &Account<'info, TokenAccount>,
	token_vault_quote: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	amount_synthetic: u64,
	amount_quote: u64,
	synthetic_to_quote: bool,
	inside_range: bool
) -> Result<()> {
	// Transfer from user to pool
	let deposit_account_user;
	let deposit_account_pool;
	let deposit_amount;

	// Transfer from pool to user
	let withdrawal_account_user;
	let withdrawal_account_pool;
	let withdrawal_amount;

	if synthetic_to_quote {
		deposit_account_user = token_owner_account_synthetic;
		deposit_account_pool = token_vault_synthetic;
		deposit_amount = amount_synthetic;

		withdrawal_account_user = token_owner_account_quote;
		withdrawal_account_pool = token_vault_quote;
		withdrawal_amount = amount_quote;
	} else {
		deposit_account_user = token_owner_account_quote;
		deposit_account_pool = token_vault_quote;
		deposit_amount = amount_quote;

		// Only send synthetic tokens from the vault if inside the range,
		// otherwise we mint new synthetic tokens to the user
		if inside_range {
			withdrawal_account_pool = token_vault_synthetic;
		}

		withdrawal_account_user = token_owner_account_synthetic;
		withdrawal_amount = amount_synthetic;
	}

	// Mint synthetic tokens instead of using LP if outside range
	if withdrawal_account_pool == null() {
		mint_synthetic_to_owner(
			authority,
			token_owner_account,
			token_vault,
			token_program,
			amount
		);
	} 

	transfer_from_owner_to_vault(
		token_authority,
		deposit_account_user,
		deposit_account_pool,
		token_program,
		deposit_amount
	)?;

	transfer_from_vault_to_owner(
		amm,
		withdrawal_account_pool,
		withdrawal_account_user,
		token_program,
		withdrawal_amount
	)?;

	ctx.output_vault.reload()?;
    ctx.input_vault.reload()?;

	Ok(())
}
