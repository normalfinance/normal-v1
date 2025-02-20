use anchor_lang::prelude::*;
use anchor_spl::token_interface::{ Mint, TokenAccount, TokenInterface };

use crate::{
	errors::ErrorCode,
	math::{ casting::Cast, constants::THIRTEEN_DAY },
	state::{
		insurance::InsuranceFund,
		paused_operations::InsuranceFundOperation,
		state::State,
		traits::Size,
	},
	validate,
};

#[derive(Accounts)]
pub struct InitializeInsuranceFund<'info> {
	#[account(
		init,
		seeds = [b"insurance_fund"],
		space = InsuranceFund::SIZE,
		bump,
		payer = admin
	)]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
	pub insurance_fund_mint: Box<InterfaceAccount<'info, Mint>>,
	#[account(
		init,
		seeds = [
			b"insurance_fund_vault".as_ref(),
			state.number_of_markets.to_le_bytes().as_ref(),
		],
		bump,
		payer = admin,
		token::mint = insurance_fund_mint,
		token::authority = normal_signer
	)]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: program signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub admin: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
	pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_initialize_insurance_fund(
	ctx: Context<InitializeInsuranceFund>,
	if_total_factor: u32
) -> Result<()> {
	let state = &mut ctx.accounts.state;
	let insurance_fund_pubkey = ctx.accounts.insurance_fund.key();

	// protocol must be authority of collateral vault
	if ctx.accounts.insurance_fund_vault.owner != state.signer {
		return Err(ErrorCode::InvalidInsuranceFundAuthority.into());
	}

	let insurance_fund = &mut ctx.accounts.insurance_fund.load_init()?;
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	**insurance_fund = InsuranceFund {
		vault: *ctx.accounts.insurance_fund_vault.to_account_info().key,
		unstaking_period: THIRTEEN_DAY,
		total_factor: if_total_factor,
		user_factor: if_total_factor / 2,
		..InsuranceFund::default()
	};

	state.insurance_fund = insurance_fund_pubkey;

	validate!(
		!insurance_fund.is_operation_paused(InsuranceFundOperation::Init),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking init disabled"
	)?;

	Ok(())
}

#[derive(Accounts)]
pub struct AdminUpdateInsurnaceFund<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
}
