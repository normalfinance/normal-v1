use anchor_lang::prelude::*;
use anchor_spl::memo::Memo;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::util::{
    calculate_transfer_fee_excluded_amount, calculate_transfer_fee_included_amount,
    parse_remaining_accounts, AccountsType, RemainingAccountsInfo,
};
use crate::{
    constants::transfer_memo,
    errors::ErrorCode,
    manager::swap_manager::*,
    state::AMM,
    util::{
        to_timestamp_u64, v2::update_and_swap_amm_v2, SparseSwapTickSequenceBuilder,
        SwapTickSequence,
    },
};

#[derive(Accounts)]
pub struct SwapV2<'info> {
    #[account(address = *token_mint_synthetic.to_account_info().owner)]
    pub token_program_synthetic: Interface<'info, TokenInterface>,
    #[account(address = *token_mint_quote.to_account_info().owner)]
    pub token_program_quote: Interface<'info, TokenInterface>,

    pub memo_program: Program<'info, Memo>,

    pub token_authority: Signer<'info>,

    #[account(mut)]
    pub amm: Box<Account<'info, AMM>>,

    #[account(address = amm.token_mint_synthetic)]
    pub token_mint_synthetic: InterfaceAccount<'info, Mint>,
    #[account(address = amm.token_mint_quote)]
    pub token_mint_quote: InterfaceAccount<'info, Mint>,

    #[account(mut, constraint = token_owner_account_synthetic.mint == amm.token_mint_synthetic)]
    pub token_owner_account_synthetic: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = amm.token_vault_synthetic)]
    pub token_vault_synthetic: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, constraint = token_owner_account_quote.mint == amm.token_mint_quote)]
    pub token_owner_account_quote: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = amm.token_vault_quote)]
    pub token_vault_quote: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_0: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_1: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: checked in the handler
    pub tick_array_2: UncheckedAccount<'info>,

    // remaining accounts
    // - accounts for transfer hook program of token_mint_synthetic
    // - accounts for transfer hook program of token_mint_quote
    // - supplemental TickArray accounts
}

pub fn handler<'info>(
    ctx: Context<'_, '_, '_, 'info, SwapV2<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit: u128,
    amount_specified_is_input: bool,
    synthetic_to_quote: bool, // Zero for one
    remaining_accounts_info: Option<RemainingAccountsInfo>,
) -> Result<()> {
    let amm = &mut ctx.accounts.amm;
    let clock = Clock::get()?;
    // Update the global reward growth which increases as a function of time.
    let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

    // Process remaining accounts
    let remaining_accounts = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[
            AccountsType::TransferHookA,
            AccountsType::TransferHookB,
            AccountsType::SupplementalTickArrays,
        ],
    )?;

    let builder = SparseSwapTickSequenceBuilder::try_from(
        amm,
        synthetic_to_quote,
        vec![
            ctx.accounts.tick_array_0.to_account_info(),
            ctx.accounts.tick_array_1.to_account_info(),
            ctx.accounts.tick_array_2.to_account_info(),
        ],
        remaining_accounts.supplemental_tick_arrays,
    )?;
    let mut swap_tick_sequence = builder.build()?;

    let swap_update = swap_with_transfer_fee_extension(
        amm,
        &ctx.accounts.token_mint_synthetic,
        &ctx.accounts.token_mint_quote,
        &mut swap_tick_sequence,
        amount,
        sqrt_price_limit,
        amount_specified_is_input,
        synthetic_to_quote,
        timestamp,
    )?;

    if amount_specified_is_input {
        let transfer_fee_excluded_output_amount = if synthetic_to_quote {
            calculate_transfer_fee_excluded_amount(
                &ctx.accounts.token_mint_quote,
                swap_update.amount_quote,
            )?
            .amount
        } else {
            calculate_transfer_fee_excluded_amount(
                &ctx.accounts.token_mint_synthetic,
                swap_update.amount_synthetic,
            )?
            .amount
        };
        if transfer_fee_excluded_output_amount < other_amount_threshold {
            return Err(ErrorCode::AmountOutBelowMinimum.into());
        }
    } else {
        let transfer_fee_included_input_amount = if synthetic_to_quote {
            swap_update.amount_synthetic
        } else {
            swap_update.amount_quote
        };
        if transfer_fee_included_input_amount > other_amount_threshold {
            return Err(ErrorCode::AmountInAboveMaximum.into());
        }
    }

    update_and_swap_amm_v2(
        amm,
        &ctx.accounts.token_authority,
        &ctx.accounts.token_mint_synthetic,
        &ctx.accounts.token_mint_quote,
        &ctx.accounts.token_owner_account_synthetic,
        &ctx.accounts.token_owner_account_quote,
        &ctx.accounts.token_vault_synthetic,
        &ctx.accounts.token_vault_quote,
        &remaining_accounts.transfer_hook_a,
        &remaining_accounts.transfer_hook_b,
        &ctx.accounts.token_program_synthetic,
        &ctx.accounts.token_program_quote,
        &ctx.accounts.memo_program,
        swap_update,
        synthetic_to_quote,
        timestamp,
        transfer_memo::TRANSFER_MEMO_SWAP.as_bytes(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn swap_with_transfer_fee_extension<'info>(
    amm: &AMM,
    token_mint_synthetic: &InterfaceAccount<'info, Mint>,
    token_mint_quote: &InterfaceAccount<'info, Mint>,
    swap_tick_sequence: &mut SwapTickSequence,
    amount: u64,
    sqrt_price_limit: u128,
    amount_specified_is_input: bool,
    synthetic_to_quote: bool,
    timestamp: u64,
) -> Result<PostSwapUpdate> {
    let (input_token_mint, output_token_mint) = if synthetic_to_quote {
        (token_mint_synthetic, token_mint_quote)
    } else {
        (token_mint_quote, token_mint_synthetic)
    };

    // ExactIn
    if amount_specified_is_input {
        let transfer_fee_included_input = amount;
        let transfer_fee_excluded_input =
            calculate_transfer_fee_excluded_amount(input_token_mint, transfer_fee_included_input)?
                .amount;

        let swap_update = swap(
            amm,
            swap_tick_sequence,
            transfer_fee_excluded_input,
            sqrt_price_limit,
            amount_specified_is_input,
            synthetic_to_quote,
            timestamp,
        )?;

        let (swap_update_amount_input, swap_update_amount_output) = if synthetic_to_quote {
            (swap_update.amount_synthetic, swap_update.amount_quote)
        } else {
            (swap_update.amount_quote, swap_update.amount_synthetic)
        };

        let fullfilled = swap_update_amount_input == transfer_fee_excluded_input;

        let adjusted_transfer_fee_included_input = if fullfilled {
            transfer_fee_included_input
        } else {
            calculate_transfer_fee_included_amount(input_token_mint, swap_update_amount_input)?
                .amount
        };

        let transfer_fee_included_output = swap_update_amount_output;

        let (amount_synthetic, amount_quote) = if synthetic_to_quote {
            (
                adjusted_transfer_fee_included_input,
                transfer_fee_included_output,
            )
        } else {
            (
                transfer_fee_included_output,
                adjusted_transfer_fee_included_input,
            )
        };
        return Ok(PostSwapUpdate {
            amount_synthetic, // updated (transfer fee included)
            amount_quote, // updated (transfer fee included)
            next_liquidity: swap_update.next_liquidity,
            next_tick_index: swap_update.next_tick_index,
            next_sqrt_price: swap_update.next_sqrt_price,
            next_fee_growth_global: swap_update.next_fee_growth_global,
            next_reward_infos: swap_update.next_reward_infos,
            next_protocol_fee: swap_update.next_protocol_fee,
        });
    }

    // ExactOut
    let transfer_fee_excluded_output = amount;
    let transfer_fee_included_output =
        calculate_transfer_fee_included_amount(output_token_mint, transfer_fee_excluded_output)?
            .amount;

    let swap_update = swap(
        amm,
        swap_tick_sequence,
        transfer_fee_included_output,
        sqrt_price_limit,
        amount_specified_is_input,
        synthetic_to_quote,
        timestamp,
    )?;

    let (swap_update_amount_input, swap_update_amount_output) = if synthetic_to_quote {
        (swap_update.amount_synthetic, swap_update.amount_quote)
    } else {
        (swap_update.amount_quote, swap_update.amount_synthetic)
    };

    let transfer_fee_included_input =
        calculate_transfer_fee_included_amount(input_token_mint, swap_update_amount_input)?.amount;

    let adjusted_transfer_fee_included_output = swap_update_amount_output;

    let (amount_synthetic, amount_quote) = if synthetic_to_quote {
        (
            transfer_fee_included_input,
            adjusted_transfer_fee_included_output,
        )
    } else {
        (
            adjusted_transfer_fee_included_output,
            transfer_fee_included_input,
        )
    };
    Ok(PostSwapUpdate {
        amount_synthetic, // updated (transfer fee included)
        amount_quote, // updated (transfer fee included)
        next_liquidity: swap_update.next_liquidity,
        next_tick_index: swap_update.next_tick_index,
        next_sqrt_price: swap_update.next_sqrt_price,
        next_fee_growth_global: swap_update.next_fee_growth_global,
        next_reward_infos: swap_update.next_reward_infos,
        next_protocol_fee: swap_update.next_protocol_fee,
    })
}
