use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::metadata::Metadata;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };
use crate::state::liquidity_position_bundle::LiquidityPositionBundle;

use crate::constants::nft::amm_nft_update_auth::ID as WPB_NFT_UPDATE_AUTH;
use crate::{
	state::*,
	util::mint_position_bundle_token_with_metadata_and_remove_authority,
};

#[derive(Accounts)]
pub struct InitializeLiquidityPositionBundleWithMetadata<'info> {
	#[account(
		init,
		payer = funder,
		space = LiquidityPositionBundle::LEN,
		seeds = [
			b"liquidity_position_bundle".as_ref(),
			position_bundle_mint.key().as_ref(),
		],
		bump
	)]
	pub position_bundle: Box<Account<'info, LiquidityPositionBundle>>,

	#[account(
		init,
		payer = funder,
		mint::authority = position_bundle, // will be removed in the transaction
		mint::decimals = 0
	)]
	pub position_bundle_mint: Account<'info, Mint>,

	/// CHECK: checked via the Metadata CPI call
	/// https://github.com/metaplex-foundation/metaplex-program-library/blob/773a574c4b34e5b9f248a81306ec24db064e255f/token-metadata/program/src/utils/metadata.rs#L100
	#[account(mut)]
	pub position_bundle_metadata: UncheckedAccount<'info>,

	#[account(
		init,
		payer = funder,
		associated_token::mint = position_bundle_mint,
		associated_token::authority = position_bundle_owner
	)]
	pub position_bundle_token_account: Box<Account<'info, TokenAccount>>,

	/// CHECK: safe, the account that will be the owner of the position bundle can be arbitrary
	pub position_bundle_owner: UncheckedAccount<'info>,

	#[account(mut)]
	pub funder: Signer<'info>,

	/// CHECK: checked via account constraints
	#[account(address = WPB_NFT_UPDATE_AUTH)]
	pub metadata_update_auth: UncheckedAccount<'info>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
	pub system_program: Program<'info, System>,
	pub rent: Sysvar<'info, Rent>,
	pub associated_token_program: Program<'info, AssociatedToken>,

	pub metadata_program: Program<'info, Metadata>,
}

pub fn handle_initialize_liquidity_position_bundle_with_metadata(
	ctx: Context<InitializeLiquidityPositionBundleWithMetadata>
) -> Result<()> {
	let position_bundle_mint = &ctx.accounts.position_bundle_mint;
	let position_bundle = &mut ctx.accounts.position_bundle;

	position_bundle.initialize(position_bundle_mint.key())?;

	let bump = ctx.bumps.position_bundle;

	mint_position_bundle_token_with_metadata_and_remove_authority(
		&ctx.accounts.funder,
		&ctx.accounts.position_bundle,
		position_bundle_mint,
		&ctx.accounts.position_bundle_token_account,
		&ctx.accounts.position_bundle_metadata,
		&ctx.accounts.metadata_update_auth,
		&ctx.accounts.metadata_program,
		&ctx.accounts.token_program,
		&ctx.accounts.system_program,
		&ctx.accounts.rent,
		&[
			b"liquidity_position_bundle".as_ref(),
			position_bundle_mint.key().as_ref(),
			&[bump],
		]
	)
}
