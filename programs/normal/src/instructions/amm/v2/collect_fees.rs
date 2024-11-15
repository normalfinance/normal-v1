use anchor_lang::prelude::*;
use anchor_spl::memo::Memo;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::util::{parse_remaining_accounts, AccountsType, RemainingAccountsInfo};
use crate::{
    constants::transfer_memo,
    state::*,
    util::{v2::transfer_from_vault_to_owner_v2, verify_position_authority_interface},
};

#[derive(Accounts)]
pub struct CollectFeesV2<'info> {
    pub amm: Box<Account<'info, AMM>>,

    pub position_authority: Signer<'info>,

    #[account(mut, has_one = amm)]
    pub position: Box<Account<'info, Position>>,
    #[account(
        constraint = position_token_account.mint == position.position_mint,
        constraint = position_token_account.amount == 1
    )]
    pub position_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

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

    #[account(address = *token_mint_synthetic.to_account_info().owner)]
    pub token_program_synthetic: Interface<'info, TokenInterface>,
    #[account(address = *token_mint_quote.to_account_info().owner)]
    pub token_program_quote: Interface<'info, TokenInterface>,
    pub memo_program: Program<'info, Memo>,
    // remaining accounts
    // - accounts for transfer hook program of token_mint_synthetic
    // - accounts for transfer hook program of token_mint_quote
}

pub fn handler<'info>(
    ctx: Context<'_, '_, '_, 'info, CollectFeesV2<'info>>,
    remaining_accounts_info: Option<RemainingAccountsInfo>,
) -> Result<()> {
    verify_position_authority_interface(
        &ctx.accounts.position_token_account,
        &ctx.accounts.position_authority,
    )?;

    // Process remaining accounts
    let remaining_accounts = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[AccountsType::TransferHookA, AccountsType::TransferHookB],
    )?;

    let position = &mut ctx.accounts.position;

    // Store the fees owed to use as transfer amounts.
    let fee_owed_a = position.fee_owed_a;
    let fee_owed_b = position.fee_owed_b;

    position.reset_fees_owed();

    transfer_from_vault_to_owner_v2(
        &ctx.accounts.amm,
        &ctx.accounts.token_mint_synthetic,
        &ctx.accounts.token_vault_synthetic,
        &ctx.accounts.token_owner_account_synthetic,
        &ctx.accounts.token_program_synthetic,
        &ctx.accounts.memo_program,
        &remaining_accounts.transfer_hook_a,
        fee_owed_a,
        transfer_memo::TRANSFER_MEMO_COLLECT_FEES.as_bytes(),
    )?;

    transfer_from_vault_to_owner_v2(
        &ctx.accounts.amm,
        &ctx.accounts.token_mint_quote,
        &ctx.accounts.token_vault_quote,
        &ctx.accounts.token_owner_account_quote,
        &ctx.accounts.token_program_quote,
        &ctx.accounts.memo_program,
        &remaining_accounts.transfer_hook_b,
        fee_owed_b,
        transfer_memo::TRANSFER_MEMO_COLLECT_FEES.as_bytes(),
    )?;

    Ok(())
}
