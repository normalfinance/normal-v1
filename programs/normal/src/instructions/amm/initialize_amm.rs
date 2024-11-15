use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

#[derive(Accounts)]
// now we don't use bumps, but we must list args in the same order to use tick_spacing arg.
#[instruction(bumps: AMMBumps, tick_spacing: u16)]
pub struct InitializeAMM<'info> {
    pub amms_config: Box<Account<'info, AMMsConfig>>,

    pub token_mint_synthetic: Account<'info, Mint>,
    pub token_mint_quote: Account<'info, Mint>,

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
      token::mint = token_mint_synthetic,
      token::authority = amm)]
    pub token_vault_synthetic: Box<Account<'info, TokenAccount>>,

    #[account(init,
      payer = funder,
      token::mint = token_mint_quote,
      token::authority = amm)]
    pub token_vault_quote: Box<Account<'info, TokenAccount>>,

    #[account(has_one = amms_config, constraint = fee_tier.tick_spacing == tick_spacing)]
    pub fee_tier: Account<'info, FeeTier>,

    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handle_initialize_amm(
    ctx: Context<InitializeAMM>,
    _bumps: AMMBumps,
    tick_spacing: u16,
    oracle_source: OracleSource,

) -> Result<()> {
    let token_mint_synthetic = ctx.accounts.token_mint_synthetic.key();
    let token_mint_quote = ctx.accounts.token_mint_quote.key();

    let amm = &mut ctx.accounts.amm;
    let amms_config = &ctx.accounts.amms_config;

    let default_fee_rate = ctx.accounts.fee_tier.default_fee_rate;

    // ignore the bump passed and use one Anchor derived
    let bump = ctx.bumps.amm;

    amm.initialize(
        amms_config,
        bump,
        tick_spacing,
        default_fee_rate,
        token_mint_synthetic,
        ctx.accounts.token_vault_synthetic.key(),
        token_mint_quote,
        ctx.accounts.token_vault_quote.key(),
    )
}
