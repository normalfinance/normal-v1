use anchor_lang::prelude::*;
use anchor_spl::token_interface::{ TokenAccount, TokenInterface };

use crate::controller::insurance::transfer_protocol_insurance_fund_stake;
use crate::error::ErrorCode;
use crate::instructions::constraints::*;
use crate::optional_accounts::get_token_mint;
use crate::state::insurance::{
	InsuranceFund,
	InsuranceFundStake,
	ProtocolInsuranceFundSharesTransferConfig,
};
use crate::state::paused_operations::InsuranceFundOperation;
use crate::state::market::{ Market, MarketStatus };
use crate::state::state::State;
use crate::state::traits::Size;
use crate::state::user::UserStats;
use crate::validate;
use crate::{ controller, math };
use crate::{ load_mut };

pub fn handle_initialize_insurance_fund_stake(
	ctx: Context<InitializeInsuranceFundStake>
) -> Result<()> {
	let mut if_stake = ctx.accounts.insurance_fund_stake
		.load_init()
		.or(Err(ErrorCode::UnableToLoadAccountLoader))?;

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	*if_stake = InsuranceFundStake::new(*ctx.accounts.authority.key, now);

	let insurance_fund = ctx.accounts.insurance_fund.load()?;

	validate!(
		!insurance_fund.is_insurance_fund_operation_paused(
			InsuranceFundOperation::Init
		),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking init disabled"
	)?;

	Ok(())
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
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(
		!insurance_fund.is_insurance_fund_operation_paused(
			InsuranceFundOperation::Add
		),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking add disabled"
	)?;

	validate!(
		insurance_fund_stake.last_withdraw_request_shares == 0 &&
			insurance_fund_stake.last_withdraw_request_value == 0,
		ErrorCode::IFWithdrawRequestInProgress,
		"withdraw request in progress"
	)?;

	{
		controller::insurance::attempt_transfer_fees_to_insurance_fund(
			&ctx.accounts.market_vault,
			&ctx.accounts.insurance_fund_vault,
			market,
			insurance_fund,
			now,
			&ctx.accounts.token_program,
			&ctx.accounts.normal_signer,
			state,
			&mint
		)?;

		// reload the vault balances so they're up-to-date
		ctx.accounts.market_vault.reload()?;
		ctx.accounts.insurance_fund_vault.reload()?;
	
	}

	controller::insurance::add_insurance_fund_stake(
		amount,
		ctx.accounts.insurance_fund_vault.amount,
		insurance_fund_stake,
		user_stats,
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

pub fn handle_request_remove_insurance_fund_stake(
	ctx: Context<RequestRemoveInsuranceFundStake>,
	amount: u64
) -> Result<()> {
	let clock = Clock::get()?;
	let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;

	validate!(
		!insurance_fund.is_insurance_fund_operation_paused(
			InsuranceFundOperation::RequestRemove
		),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking request remove disabled"
	)?;

	validate!(
		insurance_fund_stake.last_withdraw_request_shares == 0,
		ErrorCode::IFWithdrawRequestInProgress,
		"Withdraw request is already in progress"
	)?;

	let n_shares = math::insurance::vault_amount_to_if_shares(
		amount,
		market.insurance_fund.total_shares,
		ctx.accounts.insurance_fund_vault.amount
	)?;

	validate!(
		n_shares > 0,
		ErrorCode::IFWithdrawRequestTooSmall,
		"Requested lp_shares = 0"
	)?;

	let user_if_shares = insurance_fund_stake.checked_if_shares(market)?;
	validate!(user_if_shares >= n_shares, ErrorCode::InsufficientIFShares)?;

	controller::insurance::request_remove_insurance_fund_stake(
		n_shares,
		ctx.accounts.insurance_fund_vault.amount,
		insurance_fund_stake,
		user_stats,
		insurance_fund,
		clock.unix_timestamp
	)?;

	Ok(())
}

pub fn handle_cancel_request_remove_insurance_fund_stake(
	ctx: Context<RequestRemoveInsuranceFundStake>
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;

	validate!(
		insurance_fund_stake.last_withdraw_request_shares != 0,
		ErrorCode::NoIFWithdrawRequestInProgress,
		"No withdraw request in progress"
	)?;

	controller::insurance::cancel_request_remove_insurance_fund_stake(
		ctx.accounts.insurance_fund_vault.amount,
		insurance_fund_stake,
		user_stats,
		insurance_fund,
		now
	)?;

	Ok(())
}

// #[access_control(withdraw_not_paused(&ctx.accounts.state))]
pub fn handle_remove_insurance_fund_stake<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, RemoveInsuranceFundStake<'info>>
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(
		!insurance_fund.is_insurance_fund_operation_paused(
			InsuranceFundOperation::Remove
		),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking remove disabled"
	)?;

	let amount = controller::insurance::remove_insurance_fund_stake(
		ctx.accounts.insurance_fund_vault.amount,
		insurance_fund_stake,
		user_stats,
		insurance_fund,
		now
	)?;

	controller::token::send_from_program_vault(
		&ctx.accounts.token_program,
		&ctx.accounts.insurance_fund_vault,
		&ctx.accounts.user_token_account,
		&ctx.accounts.normal_signer,
		state.signer_nonce,
		amount,
		&mint
	)?;

	ctx.accounts.insurance_fund_vault.reload()?;
	validate!(
		ctx.accounts.insurance_fund_vault.amount > 0,
		ErrorCode::InvalidIFDetected,
		"insurance_fund_vault.amount must remain > 0"
	)?;

	Ok(())
}

pub fn handle_transfer_protocol_insurance_fund_shares(
	ctx: Context<TransferProtocolInsuranceFundShares>,
	shares: u128
) -> Result<()> {
	let now = Clock::get()?.unix_timestamp;

	let mut transfer_config = ctx.accounts.transfer_config.load_mut()?;

	transfer_config.validate_signer(ctx.accounts.signer.key)?;

	transfer_config.update_epoch(now)?;
	transfer_config.validate_transfer(shares)?;
	transfer_config.current_epoch_transfer += shares;

	let mut if_stake = ctx.accounts.insurance_fund_stake.load_mut()?;
	let mut insurance_fund = ctx.accounts.insurance_fund.load_mut()?;
	let mut user_stats = ctx.accounts.user_stats.load_mut()?;

	transfer_protocol_insurance_fund_stake(
		ctx.accounts.insurance_fund_vault.amount,
		shares,
		&mut if_stake,
		&mut user_stats,
		insurance_fund,
		Clock::get()?.unix_timestamp,
		ctx.accounts.state.signer
	)?;

	Ok(())
}

#[derive(Accounts)]
pub struct InitializeInsuranceFundStake<'info> {
	#[account(
		init,
		seeds = [b"insurance_fund_stake", authority.key.as_ref()],
		space = InsuranceFundStake::SIZE,
		bump,
		payer = payer
	)]
	pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
	#[account(
        mut,
        has_one = authority
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(mut)]
	pub payer: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AddInsuranceFundStake<'info> {
	pub state: Box<Account<'info, State>>,
	// TODO: review if this is correct
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
	#[account(
        mut,
        has_one = authority,
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        seeds = [b"market_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
    pub market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
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

#[derive(Accounts)]
pub struct RequestRemoveInsuranceFundStake<'info> {
	#[account(
        mut,
        has_one = authority,
    )]
	pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
	#[account(
        mut,
        has_one = authority,
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref()],
        bump,
    )]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,
}

#[derive(Accounts)]
pub struct RemoveInsuranceFundStake<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        has_one = authority,
    )]
	pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
	#[account(
        mut,
        has_one = authority,
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
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

#[derive(Accounts)]
pub struct TransferProtocolInsuranceFundShares<'info> {
	pub signer: Signer<'info>,
	#[account(mut)]
	pub transfer_config: AccountLoader<
		'info,
		ProtocolInsuranceFundSharesTransferConfig
	>,
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"insurance_fund_stake", authority.key.as_ref()],
        bump,
        has_one = authority,
    )]
	pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
	#[account(
        mut,
        has_one = authority,
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	#[account(seeds = [b"insurance_fund_vault".as_ref()], bump)]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,
}