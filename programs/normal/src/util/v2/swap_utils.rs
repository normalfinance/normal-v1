use anchor_lang::prelude::*;
use anchor_spl::memo::Memo;
use anchor_spl::token_interface::{ Mint, TokenAccount, TokenInterface };

use crate::state::amm::AMM;
use crate::{ manager::swap_manager::PostSwapUpdate, state::amm::AMM };

use super::{ transfer_from_owner_to_vault_v2, transfer_from_vault_to_owner_v2 };

#[allow(clippy::too_many_arguments)]
pub fn update_and_swap_amm_v2<'info>(
	amm: &mut Account<'info, AMM>,
	token_authority: &Signer<'info>,
	token_mint_synthetic: &InterfaceAccount<'info, Mint>,
	token_mint_quote: &InterfaceAccount<'info, Mint>,
	token_owner_account_synthetic: &InterfaceAccount<'info, TokenAccount>,
	token_owner_account_quote: &InterfaceAccount<'info, TokenAccount>,
	token_vault_synthetic: &InterfaceAccount<'info, TokenAccount>,
	token_vault_quote: &InterfaceAccount<'info, TokenAccount>,
	transfer_hook_accounts_a: &Option<Vec<AccountInfo<'info>>>,
	transfer_hook_accounts_b: &Option<Vec<AccountInfo<'info>>>,
	token_program_synthetic: &Interface<'info, TokenInterface>,
	token_program_quote: &Interface<'info, TokenInterface>,
	memo_program: &Program<'info, Memo>,
	swap_update: PostSwapUpdate,
	is_token_fee_in_synthetic: bool,
	reward_last_updated_timestamp: u64,
	memo: &[u8]
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

	perform_swap_v2(
		amm,
		token_authority,
		token_mint_synthetic,
		token_mint_quote,
		token_owner_account_synthetic,
		token_owner_account_quote,
		token_vault_synthetic,
		token_vault_quote,
		transfer_hook_accounts_a,
		transfer_hook_accounts_b,
		token_program_synthetic,
		token_program_quote,
		memo_program,
		swap_update.amount_synthetic,
		swap_update.amount_quote,
		is_token_fee_in_synthetic,
		memo
	)
}

#[allow(clippy::too_many_arguments)]
fn perform_swap_v2<'info>(
	amm: &Account<'info, AMM>,
	token_authority: &Signer<'info>,
	token_mint_synthetic: &InterfaceAccount<'info, Mint>,
	token_mint_quote: &InterfaceAccount<'info, Mint>,
	token_owner_account_synthetic: &InterfaceAccount<'info, TokenAccount>,
	token_owner_account_quote: &InterfaceAccount<'info, TokenAccount>,
	token_vault_synthetic: &InterfaceAccount<'info, TokenAccount>,
	token_vault_quote: &InterfaceAccount<'info, TokenAccount>,
	transfer_hook_accounts_a: &Option<Vec<AccountInfo<'info>>>,
	transfer_hook_accounts_b: &Option<Vec<AccountInfo<'info>>>,
	token_program_synthetic: &Interface<'info, TokenInterface>,
	token_program_quote: &Interface<'info, TokenInterface>,
	memo_program: &Program<'info, Memo>,
	amount_synthetic: u64,
	amount_quote: u64,
	synthetic_to_quote: bool,
	memo: &[u8]
) -> Result<()> {
	// Transfer from user to pool
	let deposit_token_program;
	let deposit_mint;
	let deposit_account_user;
	let deposit_account_pool;
	let deposit_transfer_hook_accounts;
	let deposit_amount;

	// Transfer from pool to user
	let withdrawal_token_program;
	let withdrawal_mint;
	let withdrawal_account_user;
	let withdrawal_account_pool;
	let withdrawal_transfer_hook_accounts;
	let withdrawal_amount;

	if synthetic_to_quote {
		deposit_token_program = token_program_synthetic;
		deposit_mint = token_mint_synthetic;
		deposit_account_user = token_owner_account_synthetic;
		deposit_account_pool = token_vault_synthetic;
		deposit_transfer_hook_accounts = transfer_hook_accounts_a;
		deposit_amount = amount_synthetic;

		withdrawal_token_program = token_program_quote;
		withdrawal_mint = token_mint_quote;
		withdrawal_account_user = token_owner_account_quote;
		withdrawal_account_pool = token_vault_quote;
		withdrawal_transfer_hook_accounts = transfer_hook_accounts_b;
		withdrawal_amount = amount_quote;
	} else {
		deposit_token_program = token_program_quote;
		deposit_mint = token_mint_quote;
		deposit_account_user = token_owner_account_quote;
		deposit_account_pool = token_vault_quote;
		deposit_transfer_hook_accounts = transfer_hook_accounts_b;
		deposit_amount = amount_quote;

		withdrawal_token_program = token_program_synthetic;
		withdrawal_mint = token_mint_synthetic;
		withdrawal_account_user = token_owner_account_synthetic;
		withdrawal_account_pool = token_vault_synthetic;
		withdrawal_transfer_hook_accounts = transfer_hook_accounts_a;
		withdrawal_amount = amount_synthetic;
	}

	transfer_from_owner_to_vault_v2(
		token_authority,
		deposit_mint,
		deposit_account_user,
		deposit_account_pool,
		deposit_token_program,
		memo_program,
		deposit_transfer_hook_accounts,
		deposit_amount
	)?;

	transfer_from_vault_to_owner_v2(
		amm,
		withdrawal_mint,
		withdrawal_account_pool,
		withdrawal_account_user,
		withdrawal_token_program,
		memo_program,
		withdrawal_transfer_hook_accounts,
		withdrawal_amount,
		memo
	)?;

	Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn update_and_two_hop_swap_amm_v2<'info>(
	// update
	swap_update_one: PostSwapUpdate,
	swap_update_two: PostSwapUpdate,
	// amm
	amm_one: &mut Account<'info, AMM>,
	amm_two: &mut Account<'info, AMM>,
	// direction
	is_token_fee_in_one_a: bool,
	is_token_fee_in_two_a: bool,
	// mint
	token_mint_input: &InterfaceAccount<'info, Mint>,
	token_mint_intermediate: &InterfaceAccount<'info, Mint>,
	token_mint_output: &InterfaceAccount<'info, Mint>,
	// token program
	token_program_input: &Interface<'info, TokenInterface>,
	token_program_intermediate: &Interface<'info, TokenInterface>,
	token_program_output: &Interface<'info, TokenInterface>,
	// token accounts
	token_owner_account_input: &InterfaceAccount<'info, TokenAccount>,
	token_vault_one_input: &InterfaceAccount<'info, TokenAccount>,
	token_vault_one_intermediate: &InterfaceAccount<'info, TokenAccount>,
	token_vault_two_intermediate: &InterfaceAccount<'info, TokenAccount>,
	token_vault_two_output: &InterfaceAccount<'info, TokenAccount>,
	token_owner_account_output: &InterfaceAccount<'info, TokenAccount>,
	// hook
	transfer_hook_accounts_input: &Option<Vec<AccountInfo<'info>>>,
	transfer_hook_accounts_intermediate: &Option<Vec<AccountInfo<'info>>>,
	transfer_hook_accounts_output: &Option<Vec<AccountInfo<'info>>>,
	// common
	token_authority: &Signer<'info>,
	memo_program: &Program<'info, Memo>,
	reward_last_updated_timestamp: u64,
	memo: &[u8]
) -> Result<()> {
	amm_one.update_after_swap(
		swap_update_one.next_liquidity,
		swap_update_one.next_tick_index,
		swap_update_one.next_sqrt_price,
		swap_update_one.next_fee_growth_global,
		swap_update_one.next_reward_infos,
		swap_update_one.next_protocol_fee,
		is_token_fee_in_one_a,
		reward_last_updated_timestamp
	);

	amm_two.update_after_swap(
		swap_update_two.next_liquidity,
		swap_update_two.next_tick_index,
		swap_update_two.next_sqrt_price,
		swap_update_two.next_fee_growth_global,
		swap_update_two.next_reward_infos,
		swap_update_two.next_protocol_fee,
		is_token_fee_in_two_a,
		reward_last_updated_timestamp
	);

	// amount
	let (input_amount, intermediate_amount) = if is_token_fee_in_one_a {
		(swap_update_one.amount_synthetic, swap_update_one.amount_quote)
	} else {
		(swap_update_one.amount_quote, swap_update_one.amount_synthetic)
	};
	let output_amount = if is_token_fee_in_two_a {
		swap_update_two.amount_quote
	} else {
		swap_update_two.amount_synthetic
	};

	transfer_from_owner_to_vault_v2(
		token_authority,
		token_mint_input,
		token_owner_account_input,
		token_vault_one_input,
		token_program_input,
		memo_program,
		transfer_hook_accounts_input,
		input_amount
	)?;

	// Transfer from pool to pool
	transfer_from_vault_to_owner_v2(
		amm_one,
		token_mint_intermediate,
		token_vault_one_intermediate,
		token_vault_two_intermediate,
		token_program_intermediate,
		memo_program,
		transfer_hook_accounts_intermediate,
		intermediate_amount,
		memo
	)?;

	transfer_from_vault_to_owner_v2(
		amm_two,
		token_mint_output,
		token_vault_two_output,
		token_owner_account_output,
		token_program_output,
		memo_program,
		transfer_hook_accounts_output,
		output_amount,
		memo
	)?;

	Ok(())
}
