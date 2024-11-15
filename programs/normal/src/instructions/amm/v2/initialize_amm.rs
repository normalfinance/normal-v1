use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{
    errors::ErrorCode,
    state::*,
    util::{v2::is_supported_token_mint},
};

#[derive(Accounts)]
#[instruction(tick_spacing: u16)]
pub struct InitializeAMMV2<'info> {
    pub amms_config: Box<Account<'info, AMMsConfig>>,

    pub token_mint_synthetic: InterfaceAccount<'info, Mint>,
    pub token_mint_quote: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub funder: Signer<'info>,

    #[account(init,
      seeds = [
        b"amm".as_ref(),
        amms_config.key().as_ref(),
        token_mint_synthetic.key().as_ref(),
        token_mint_quote.key().as_ref(),
        tick_spacing.to_le_bytes().as_ref()
      ],
      bump,
      payer = funder,
      space = AMM::LEN)]
    pub amm: Box<Account<'info, AMM>>,

    #[account(init,
      payer = funder,
      token::token_program = token_program_synthetic,
      token::mint = token_mint_synthetic,
      token::authority = amm)]
    pub token_vault_synthetic: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(init,
      payer = funder,
      token::token_program = token_program_quote,
      token::mint = token_mint_quote,
      token::authority = amm)]
    pub token_vault_quote: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(has_one = amms_config, constraint = fee_tier.tick_spacing == tick_spacing)]
    pub fee_tier: Account<'info, FeeTier>,

    #[account(address = *token_mint_synthetic.to_account_info().owner)]
    pub token_program_synthetic: Interface<'info, TokenInterface>,
    #[account(address = *token_mint_quote.to_account_info().owner)]
    pub token_program_quote: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<InitializeAMMV2>,
    tick_spacing: u16,
    initial_sqrt_price: u128,
) -> Result<()> {
    let token_mint_synthetic = ctx.accounts.token_mint_synthetic.key();
    let token_mint_quote = ctx.accounts.token_mint_quote.key();

    let amm = &mut ctx.accounts.amm;
    let amms_config = &ctx.accounts.amms_config;

    let default_fee_rate = ctx.accounts.fee_tier.default_fee_rate;

    // ignore the bump passed and use one Anchor derived
    let bump = ctx.bumps.amm;

    if !is_supported_token_mint(&ctx.accounts.token_mint_synthetic).unwrap() {
        return Err(ErrorCode::UnsupportedTokenMint.into());
    }

    
    if !is_supported_token_mint(&ctx.accounts.token_mint_quote).unwrap() {
        return Err(ErrorCode::UnsupportedTokenMint.into());
    }

    amm.initialize(
        amms_config,
        bump,
        tick_spacing,
        initial_sqrt_price,
        default_fee_rate,
        token_mint_synthetic,
        ctx.accounts.token_vault_synthetic.key(),
        token_mint_quote,
        ctx.accounts.token_vault_quote.key(),
    )
}
