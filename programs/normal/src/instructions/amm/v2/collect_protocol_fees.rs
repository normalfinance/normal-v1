use crate::util::{parse_remaining_accounts, AccountsType, RemainingAccountsInfo};
use crate::{constants::transfer_memo, state::*, util::v2::transfer_from_vault_to_owner_v2};
use anchor_lang::prelude::*;
use anchor_spl::memo::Memo;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct CollectProtocolFeesV2<'info> {
    pub amms_config: Box<Account<'info, AMMsConfig>>,

    #[account(mut, has_one = amms_config)]
    pub amm: Box<Account<'info, AMM>>,

    #[account(address = amms_config.collect_protocol_fees_authority)]
    pub collect_protocol_fees_authority: Signer<'info>,

    #[account(address = amm.token_mint_synthetic)]
    pub token_mint_synthetic: InterfaceAccount<'info, Mint>,
    #[account(address = amm.token_mint_quote)]
    pub token_mint_quote: InterfaceAccount<'info, Mint>,

    #[account(mut, address = amm.token_vault_synthetic)]
    pub token_vault_synthetic: InterfaceAccount<'info, TokenAccount>,

    #[account(mut, address = amm.token_vault_quote)]
    pub token_vault_quote: InterfaceAccount<'info, TokenAccount>,

    #[account(mut, constraint = token_destination_a.mint == amm.token_mint_synthetic)]
    pub token_destination_a: InterfaceAccount<'info, TokenAccount>,

    #[account(mut, constraint = token_destination_b.mint == amm.token_mint_quote)]
    pub token_destination_b: InterfaceAccount<'info, TokenAccount>,

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
    ctx: Context<'_, '_, '_, 'info, CollectProtocolFeesV2<'info>>,
    remaining_accounts_info: Option<RemainingAccountsInfo>,
) -> Result<()> {
    let amm = &ctx.accounts.amm;

    // Process remaining accounts
    let remaining_accounts = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[AccountsType::TransferHookA, AccountsType::TransferHookB],
    )?;

    transfer_from_vault_to_owner_v2(
        amm,
        &ctx.accounts.token_mint_synthetic,
        &ctx.accounts.token_vault_synthetic,
        &ctx.accounts.token_destination_a,
        &ctx.accounts.token_program_synthetic,
        &ctx.accounts.memo_program,
        &remaining_accounts.transfer_hook_a,
        amm.protocol_fee_owed_synthetic,
        transfer_memo::TRANSFER_MEMO_COLLECT_PROTOCOL_FEES.as_bytes(),
    )?;

    transfer_from_vault_to_owner_v2(
        amm,
        &ctx.accounts.token_mint_quote,
        &ctx.accounts.token_vault_quote,
        &ctx.accounts.token_destination_b,
        &ctx.accounts.token_program_quote,
        &ctx.accounts.memo_program,
        &remaining_accounts.transfer_hook_b,
        amm.protocol_fee_owed_quote,
        transfer_memo::TRANSFER_MEMO_COLLECT_PROTOCOL_FEES.as_bytes(),
    )?;

    ctx.accounts.amm.reset_protocol_fees_owed();
    Ok(())
}
