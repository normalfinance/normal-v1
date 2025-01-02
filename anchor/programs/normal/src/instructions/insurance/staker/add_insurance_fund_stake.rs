use anchor_lang::prelude::*;
use anchor_spl::token_interface::{ TokenAccount, TokenInterface };

use crate::error::ErrorCode;
use crate::instructions::constraints::*;
use crate::optional_accounts::get_token_mint;
use crate::state::insurance::{ InsuranceFund, InsuranceFundStake };
use crate::state::paused_operations::InsuranceFundOperation;
use crate::state::state::State;
use crate::state::traits::Size;
use crate::util::transfer_from_owner_to_vault;
use crate::validate;
use crate::{ controller, math };
use crate::load_mut;

#[derive(Accounts)]
pub struct AddInsuranceFundStake<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"insurance_fund"],
        bump
    )]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
	#[account(
        mut,
        has_one = authority,
    )]
	pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref()],
        bump,
    )]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,

	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        token::mint = insurance_fund_vault.mint,
        token::authority = authority
    )]
	pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_add_insurance_fund_stake<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, AddInsuranceFundStake<'info>>,
	amount: u64
) -> Result<()> {
	if amount == 0 {
		return Err(ErrorCode::InsufficientDeposit.into());
	}

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(
		!insurance_fund.is_operation_paused(InsuranceFundOperation::Add),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking add disabled"
	)?;

	// TODO: Ensure amount will not put Insurance Fund over max_insurance
	// validate!(
	// 	insurance_fund.max_insurance >,
	// 	ErrorCode::InsuranceFundOperationPaused,
	// 	"if staking add disabled"
	// )?;

	validate!(
		insurance_fund_stake.last_withdraw_request_shares == 0 &&
			insurance_fund_stake.last_withdraw_request_value == 0,
		ErrorCode::IFWithdrawRequestInProgress,
		"withdraw request in progress"
	)?;

	// TODO:
	// {
	//     controller::insurance::attempt_settle_revenue_to_insurance_fund(
	//         &ctx.accounts.spot_market_vault,
	//         &ctx.accounts.insurance_fund_vault,
	//         spot_market,
	//         now,
	//         &ctx.accounts.token_program,
	//         &ctx.accounts.normal_signer,
	//         state,
	//         &mint,
	//     )?;

	//     // reload the vault balances so they're up-to-date
	//     ctx.accounts.spot_market_vault.reload()?;
	//     ctx.accounts.insurance_fund_vault.reload()?;
	//     math::spot_withdraw::validate_spot_market_vault_amount(
	//         spot_market,
	//         ctx.accounts.spot_market_vault.amount,
	//     )?;
	// }

	controller::insurance::add_insurance_fund_stake(
		amount,
		ctx.accounts.insurance_fund_vault.amount,
		insurance_fund_stake,
		insurance_fund,
		clock.unix_timestamp
	)?;

	controller::token::receive(
		&ctx.accounts.token_program,
		&ctx.accounts.user_token_account,
		&ctx.accounts.insurance_fund_vault,
		&ctx.accounts.authority,
		amount,
		&mint
	)?;

	Ok(())
}
