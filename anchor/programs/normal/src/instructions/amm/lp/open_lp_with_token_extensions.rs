use crate::state::*;
use crate::util::build_position_token_metadata;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_2022::spl_token_2022;
use anchor_spl::token_2022::Token2022;
use synth_market::SynthMarket;

use crate::constants::nft::amm_nft_update_auth::ID as WP_NFT_UPDATE_AUTH;
use crate::util::{
    initialize_position_mint_2022, initialize_position_token_account_2022,
    initialize_token_metadata_extension, mint_position_token_2022_and_remove_authority,
};

#[derive(Accounts)]
pub struct OpenLPWithTokenExtensions<'info> {
    #[account(mut)]
    pub funder: Signer<'info>,

    /// CHECK: safe, the account that will be the owner of the position can be arbitrary
    pub owner: UncheckedAccount<'info>,

    #[account(init,
      payer = funder,
      space = Position::LEN,
      seeds = [b"position".as_ref(), position_mint.key().as_ref()],
      bump,
    )]
    pub position: Box<Account<'info, Position>>,

    /// CHECK: initialized in the handler
    #[account(mut)]
    pub position_mint: Signer<'info>,

    /// CHECK: initialized in the handler
    #[account(mut)]
    pub position_token_account: UncheckedAccount<'info>,

    pub market: Box<Account<'info, Market>>,

    #[account(address = spl_token_2022::ID)]
    pub token_2022_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// CHECK: checked via account constraints
    #[account(address = WP_NFT_UPDATE_AUTH)]
    pub metadata_update_auth: UncheckedAccount<'info>,
}

/*
  Opens a new Market AMM Position with Mint and TokenAccount owned by Token-2022.
*/
pub fn handle_open_lp_with_token_extensions(
    ctx: Context<OpenLPWithTokenExtensions>,
    tick_lower_index: i32,
    tick_upper_index: i32,
    with_token_metadata: bool,
) -> Result<()> {
    let market = &ctx.accounts.market;
    let position_mint = &ctx.accounts.position_mint;
    let position = &mut ctx.accounts.position;

    let position_seeds = [
        b"position".as_ref(),
        position_mint.key.as_ref(),
        &[ctx.bumps.position],
    ];

    position.open_position(
        market,
        position_mint.key(),
        tick_lower_index,
        tick_upper_index,
    )?;

    initialize_position_mint_2022(
        position_mint,
        &ctx.accounts.funder,
        position,
        &ctx.accounts.system_program,
        &ctx.accounts.token_2022_program,
        with_token_metadata,
    )?;

    if with_token_metadata {
        let (name, symbol, uri) = build_position_token_metadata(position_mint, position, market);
    
        initialize_token_metadata_extension(
            name,
            symbol,
            uri,
            position_mint,
            position,
            &ctx.accounts.metadata_update_auth,
            &ctx.accounts.funder,
            &ctx.accounts.system_program,
            &ctx.accounts.token_2022_program,
            &position_seeds,
        )?;
    }

    initialize_position_token_account_2022(
        &ctx.accounts.position_token_account,
        position_mint,
        &ctx.accounts.funder,
        &ctx.accounts.owner,
        &ctx.accounts.token_2022_program,
        &ctx.accounts.system_program,
        &ctx.accounts.associated_token_program,
    )?;

    mint_position_token_2022_and_remove_authority(
        position,
        position_mint,
        &ctx.accounts.position_token_account,
        &ctx.accounts.token_2022_program,
        &position_seeds,
    )?;

    Ok(())
}
