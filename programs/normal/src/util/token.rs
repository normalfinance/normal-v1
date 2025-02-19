use crate::state::market::Market;
use crate::state::{ PositionBundle, AMM };
use anchor_lang::prelude::*;
use anchor_spl::metadata::{
	self,
	mpl_token_metadata::types::DataV2,
	CreateMetadataAccountsV3,
};
use anchor_spl::token::{ self, Mint, Token, TokenAccount, Transfer };
use solana_program::program::invoke_signed;
use spl_token::instruction::{
	burn_checked,
	close_account,
	mint_to,
	set_authority,
	AuthorityType,
};

use crate::constants::nft::{
	WPB_METADATA_NAME_PREFIX,
	WPB_METADATA_SYMBOL,
	WPB_METADATA_URI,
	WP_METADATA_NAME,
	WP_METADATA_SYMBOL,
	WP_METADATA_URI,
};

pub fn initialize_synthetic_token<'info>(
	// mint: &InterfaceAccount<'info, Mint>,
	// rent,
	// token_program: &Interface<'info, TokenInterface>,
	// signer,
	// decimals: u8
) -> Result<()> {
	let cpi_accounts = token::InitializeMint {
		mint: ctx.accounts.mint.to_account_info(),
		rent: ctx.accounts.rent.to_account_info(),
	};
	let cpi_program = ctx.accounts.token_program.to_account_info();
	let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
	token::initialize_mint(
		cpi_ctx,
		decimals,
		ctx.accounts.payer.key,
		Some(ctx.accounts.payer.key)
	)?;

	let cpi_accounts = token::MintTo {
		mint: ctx.accounts.mint.to_account_info(),
		to: ctx.accounts.token_account.to_account_info(),
		authority: ctx.accounts.payer.to_account_info(),
	};
	let cpi_ctx = CpiContext::new(
		ctx.accounts.token_program.to_account_info(),
		cpi_accounts
	);
	token::mint_to(cpi_ctx, initial_supply)?;

	Ok(())
}

pub fn mint_synthetic_to_amm<'info>(
	authority: &Signer<'info>,
	token_owner_account: &Account<'info, TokenAccount>,
	token_vault: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	amount: u64
) -> Result<()> {
	mint_synthetic_token(amm, mint, token_vault, token_program)?;
	Ok(())
}

pub fn mint_synthetic_to_owner<'info>(
	authority: &Signer<'info>,
	token_owner_account: &Account<'info, TokenAccount>,
	token_vault: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	amount: u64
) -> Result<()> {
	mint_synthetic_token(amm, mint, token_vault, token_program)?;
	Ok(())
}

fn mint_synthetic_token<'info>(
	amm: &Account<'info, AMM>,
	mint: &Account<'info, Mint>,
	token_account: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>
) -> Result<()> {
	invoke_signed(
		&mint_to(
			token_program.key,
			mint.to_account_info().key,
			token_account.to_account_info().key,
			amm.to_account_info().key,
			&[amm.to_account_info().key],
			1
		)?,
		&[
			mint.to_account_info(),
			token_account.to_account_info(),
			amm.to_account_info(),
			token_program.to_account_info(),
		],
		&[&amm.seeds()]
	)?;
	Ok(())
}

pub fn burn_synthetic_from_vault<'info>(
	authority: &Signer<'info>,
	token_owner_account: &Account<'info, TokenAccount>,
	token_vault: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	amount: u64
) -> Result<()> {
	burn_synthetic_token(amm, mint, token_vault, token_program)?;
	Ok(())
}

fn burn_synthetic_token<'info>(
	token_authority: &Signer<'info>,
	receiver: &UncheckedAccount<'info>,
	mint: &Account<'info, Mint>,
	token_account: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>
) -> Result<()> {
	invoke_signed(
		&burn_checked(
			token_program.key,
			token_account.to_account_info().key,
			mint.to_account_info().key,
			token_authority.key,
			&[],
			1,
			mint.decimals
		)?,
		&[
			token_program.to_account_info(),
			token_account.to_account_info(),
			mint.to_account_info(),
			token_authority.to_account_info(),
		],
		&[]
	)?;
	Ok(())
}

// ==========

pub fn transfer_from_owner_to_vault<'info>(
	position_authority: &Signer<'info>,
	token_owner_account: &Account<'info, TokenAccount>,
	token_vault: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	amount: u64
) -> Result<()> {
	token::transfer(
		CpiContext::new(token_program.to_account_info(), Transfer {
			from: token_owner_account.to_account_info(),
			to: token_vault.to_account_info(),
			authority: position_authority.to_account_info(),
		}),
		amount
	)
}

pub fn transfer_from_vault_to_owner<'info>(
	market: &Account<'info, Market>,
	token_vault: &Account<'info, TokenAccount>,
	token_owner_account: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	amount: u64
) -> Result<()> {
	token::transfer(
		CpiContext::new_with_signer(
			token_program.to_account_info(),
			Transfer {
				from: token_vault.to_account_info(),
				to: token_owner_account.to_account_info(),
				authority: market.to_account_info(),
			},
			&[&market.seeds()]
		),
		amount
	)
}

pub fn transfer_from_owner_to_amm<'info>(
	position_authority: &Signer<'info>,
	token_owner_account: &Account<'info, TokenAccount>,
	token_vault: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	amount: u64
) -> Result<()> {
	token::transfer(
		CpiContext::new(token_program.to_account_info(), Transfer {
			from: token_owner_account.to_account_info(),
			to: token_vault.to_account_info(),
			authority: position_authority.to_account_info(),
		}),
		amount
	)
}

pub fn burn_and_close_user_position_token<'info>(
	token_authority: &Signer<'info>,
	receiver: &UncheckedAccount<'info>,
	position_mint: &Account<'info, Mint>,
	position_token_account: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>
) -> Result<()> {
	// Burn a single token in user account
	invoke_signed(
		&burn_checked(
			token_program.key,
			position_token_account.to_account_info().key,
			position_mint.to_account_info().key,
			token_authority.key,
			&[],
			1,
			position_mint.decimals
		)?,
		&[
			token_program.to_account_info(),
			position_token_account.to_account_info(),
			position_mint.to_account_info(),
			token_authority.to_account_info(),
		],
		&[]
	)?;

	// Close user account
	invoke_signed(
		&close_account(
			token_program.key,
			position_token_account.to_account_info().key,
			receiver.key,
			token_authority.key,
			&[]
		)?,
		&[
			token_program.to_account_info(),
			position_token_account.to_account_info(),
			receiver.to_account_info(),
			token_authority.to_account_info(),
		],
		&[]
	)?;
	Ok(())
}

pub fn mint_position_token_and_remove_authority<'info>(
	market: &Account<'info, Market>,
	position_mint: &Account<'info, Mint>,
	position_token_account: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>
) -> Result<()> {
	mint_position_token(
		market,
		position_mint,
		position_token_account,
		token_program
	)?;
	remove_position_token_mint_authority(market, position_mint, token_program)
}

#[allow(clippy::too_many_arguments)]
pub fn mint_position_token_with_metadata_and_remove_authority<'info>(
	market: &Account<'info, Market>,
	position_mint: &Account<'info, Mint>,
	position_token_account: &Account<'info, TokenAccount>,
	position_metadata_account: &UncheckedAccount<'info>,
	metadata_update_auth: &UncheckedAccount<'info>,
	funder: &Signer<'info>,
	metadata_program: &Program<'info, metadata::Metadata>,
	token_program: &Program<'info, Token>,
	system_program: &Program<'info, System>,
	rent: &Sysvar<'info, Rent>
) -> Result<()> {
	mint_position_token(
		market,
		position_mint,
		position_token_account,
		token_program
	)?;

	let metadata_mint_auth_account = market;
	metadata::create_metadata_accounts_v3(
		CpiContext::new_with_signer(
			metadata_program.to_account_info(),
			CreateMetadataAccountsV3 {
				metadata: position_metadata_account.to_account_info(),
				mint: position_mint.to_account_info(),
				mint_authority: metadata_mint_auth_account.to_account_info(),
				update_authority: metadata_update_auth.to_account_info(),
				payer: funder.to_account_info(),
				rent: rent.to_account_info(),
				system_program: system_program.to_account_info(),
			},
			&[&metadata_mint_auth_account.seeds()]
		),
		DataV2 {
			name: WP_METADATA_NAME.to_string(),
			symbol: WP_METADATA_SYMBOL.to_string(),
			uri: WP_METADATA_URI.to_string(),
			creators: None,
			seller_fee_basis_points: 0,
			collection: None,
			uses: None,
		},
		true,
		false,
		None
	)?;

	remove_position_token_mint_authority(
		market,
		position_mint,
		token_program
	)
}

fn mint_position_token<'info>(
	market: &Account<'info, Market>,
	position_mint: &Account<'info, Mint>,
	position_token_account: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>
) -> Result<()> {
	invoke_signed(
		&mint_to(
			token_program.key,
			position_mint.to_account_info().key,
			position_token_account.to_account_info().key,
			market.to_account_info().key,
			&[market.to_account_info().key],
			1
		)?,
		&[
			position_mint.to_account_info(),
			position_token_account.to_account_info(),
			market.to_account_info(),
			token_program.to_account_info(),
		],
		&[&market.seeds()]
	)?;
	Ok(())
}

fn remove_position_token_mint_authority<'info>(
	market: &Account<'info, Market>,
	position_mint: &Account<'info, Mint>,
	token_program: &Program<'info, Token>
) -> Result<()> {
	invoke_signed(
		&set_authority(
			token_program.key,
			position_mint.to_account_info().key,
			Option::None,
			AuthorityType::MintTokens,
			market.to_account_info().key,
			&[market.to_account_info().key]
		)?,
		&[
			position_mint.to_account_info(),
			market.to_account_info(),
			token_program.to_account_info(),
		],
		&[&market.seeds()]
	)?;
	Ok(())
}

pub fn mint_position_bundle_token_and_remove_authority<'info>(
	position_bundle: &Account<'info, LiquidityPositionBundle>,
	position_bundle_mint: &Account<'info, Mint>,
	position_bundle_token_account: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	position_bundle_seeds: &[&[u8]]
) -> Result<()> {
	mint_position_bundle_token(
		position_bundle,
		position_bundle_mint,
		position_bundle_token_account,
		token_program,
		position_bundle_seeds
	)?;
	remove_position_bundle_token_mint_syntheticuthority(
		position_bundle,
		position_bundle_mint,
		token_program,
		position_bundle_seeds
	)
}

#[allow(clippy::too_many_arguments)]
pub fn mint_position_bundle_token_with_metadata_and_remove_authority<'info>(
	funder: &Signer<'info>,
	position_bundle: &Account<'info, LiquidityPositionBundle>,
	position_bundle_mint: &Account<'info, Mint>,
	position_bundle_token_account: &Account<'info, TokenAccount>,
	position_bundle_metadata: &UncheckedAccount<'info>,
	metadata_update_auth: &UncheckedAccount<'info>,
	metadata_program: &Program<'info, metadata::Metadata>,
	token_program: &Program<'info, Token>,
	system_program: &Program<'info, System>,
	rent: &Sysvar<'info, Rent>,
	position_bundle_seeds: &[&[u8]]
) -> Result<()> {
	mint_position_bundle_token(
		position_bundle,
		position_bundle_mint,
		position_bundle_token_account,
		token_program,
		position_bundle_seeds
	)?;

	// Create Metadata
	// Orca Position Bundle xxxx...yyyy
	// xxxx and yyyy are the first and last 4 chars of mint address
	let mint_address = position_bundle_mint.key().to_string();
	let mut nft_name = String::from(WPB_METADATA_NAME_PREFIX);
	nft_name += " ";
	nft_name += &mint_address[0..4];
	nft_name += "...";
	nft_name += &mint_address[mint_address.len() - 4..];

	metadata::create_metadata_accounts_v3(
		CpiContext::new_with_signer(
			metadata_program.to_account_info(),
			CreateMetadataAccountsV3 {
				metadata: position_bundle_metadata.to_account_info(),
				mint: position_bundle_mint.to_account_info(),
				mint_authority: position_bundle.to_account_info(),
				update_authority: metadata_update_auth.to_account_info(),
				payer: funder.to_account_info(),
				rent: rent.to_account_info(),
				system_program: system_program.to_account_info(),
			},
			&[position_bundle_seeds]
		),
		DataV2 {
			name: nft_name,
			symbol: WPB_METADATA_SYMBOL.to_string(),
			uri: WPB_METADATA_URI.to_string(),
			creators: None,
			seller_fee_basis_points: 0,
			collection: None,
			uses: None,
		},
		true,
		false,
		None
	)?;

	remove_position_bundle_token_mint_syntheticuthority(
		position_bundle,
		position_bundle_mint,
		token_program,
		position_bundle_seeds
	)
}

fn mint_position_bundle_token<'info>(
	position_bundle: &Account<'info, LiquidityPositionBundle>,
	position_bundle_mint: &Account<'info, Mint>,
	position_bundle_token_account: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>,
	position_bundle_seeds: &[&[u8]]
) -> Result<()> {
	invoke_signed(
		&mint_to(
			token_program.key,
			position_bundle_mint.to_account_info().key,
			position_bundle_token_account.to_account_info().key,
			position_bundle.to_account_info().key,
			&[],
			1
		)?,
		&[
			position_bundle_mint.to_account_info(),
			position_bundle_token_account.to_account_info(),
			position_bundle.to_account_info(),
			token_program.to_account_info(),
		],
		&[position_bundle_seeds]
	)?;

	Ok(())
}

fn remove_position_bundle_token_mint_syntheticuthority<'info>(
	position_bundle: &Account<'info, LiquidityPositionBundle>,
	position_bundle_mint: &Account<'info, Mint>,
	token_program: &Program<'info, Token>,
	position_bundle_seeds: &[&[u8]]
) -> Result<()> {
	invoke_signed(
		&set_authority(
			token_program.key,
			position_bundle_mint.to_account_info().key,
			Option::None,
			AuthorityType::MintTokens,
			position_bundle.to_account_info().key,
			&[]
		)?,
		&[
			position_bundle_mint.to_account_info(),
			position_bundle.to_account_info(),
			token_program.to_account_info(),
		],
		&[position_bundle_seeds]
	)?;

	Ok(())
}

pub fn burn_and_close_position_bundle_token<'info>(
	position_bundle_authority: &Signer<'info>,
	receiver: &UncheckedAccount<'info>,
	position_bundle_mint: &Account<'info, Mint>,
	position_bundle_token_account: &Account<'info, TokenAccount>,
	token_program: &Program<'info, Token>
) -> Result<()> {
	// use same logic
	burn_and_close_user_position_token(
		position_bundle_authority,
		receiver,
		position_bundle_mint,
		position_bundle_token_account,
		token_program
	)
}
