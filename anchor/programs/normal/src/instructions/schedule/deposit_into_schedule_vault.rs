use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct DepositIntoScheduleVault<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub spot_market: AccountLoader<'info, SpotMarket>,
	#[account(
		constraint = admin.key() == admin_hot_wallet::id() ||
		admin.key() == state.admin
	)]
	pub admin: Signer<'info>,
	#[account(
        mut,
        token::authority = admin
    )]
	pub source_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(
        mut,
        constraint = spot_market.load()?.vault == spot_market_vault.key()
    )]
	pub spot_market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

#[access_control(
    deposit_not_paused(&ctx.accounts.state)
    spot_market_valid(&ctx.accounts.spot_market)
)]
pub fn handle_deposit_into_schedule_vault<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, DepositIntoScheduleVault<'info>>,
	amount: u64
) -> Result<()> {
	let spot_market = &mut load_mut!(ctx.accounts.spot_market)?;

	validate!(
		!spot_market.is_operation_paused(SpotOperation::Deposit),
		ErrorCode::DefaultError,
		"spot market deposits paused"
	)?;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();

	let mint = get_token_mint(remaining_accounts_iter)?;

	msg!(
		"depositing {} into spot market {} vault",
		amount,
		spot_market.market_index
	);

	let deposit_token_amount_before = spot_market.get_deposits()?;

	let deposit_token_amount_after = deposit_token_amount_before.safe_add(
		amount.cast()?
	)?;

	validate!(
		deposit_token_amount_after > deposit_token_amount_before,
		ErrorCode::DefaultError,
		"new_deposit_token_amount ({}) <= deposit_token_amount ({})",
		deposit_token_amount_after,
		deposit_token_amount_before
	)?;

	let token_precision = spot_market.get_precision();

	let cumulative_deposit_interest_before =
		spot_market.cumulative_deposit_interest;

	let cumulative_deposit_interest_after = deposit_token_amount_after
		.safe_mul(SPOT_CUMULATIVE_INTEREST_PRECISION)?
		.safe_div(spot_market.deposit_balance)?
		.safe_mul(SPOT_BALANCE_PRECISION)?
		.safe_div(token_precision.cast()?)?;

	validate!(
		cumulative_deposit_interest_after > cumulative_deposit_interest_before,
		ErrorCode::DefaultError,
		"cumulative_deposit_interest_after ({}) <= cumulative_deposit_interest_before ({})",
		cumulative_deposit_interest_after,
		cumulative_deposit_interest_before
	)?;

	spot_market.cumulative_deposit_interest = cumulative_deposit_interest_after;

	controller::token::receive(
		&ctx.accounts.token_program,
		&ctx.accounts.source_vault,
		&ctx.accounts.spot_market_vault,
		&ctx.accounts.admin.to_account_info(),
		amount,
		&mint
	)?;

	ctx.accounts.spot_market_vault.reload()?;
	validate_spot_market_vault_amount(
		spot_market,
		ctx.accounts.spot_market_vault.amount
	)?;

	spot_market.validate_max_token_deposits_and_borrows(false)?;

	emit!(SpotMarketVaultDepositRecord {
		ts: Clock::get()?.unix_timestamp,
		market_index: spot_market.market_index,
		deposit_balance: spot_market.deposit_balance,
		cumulative_deposit_interest_before,
		cumulative_deposit_interest_after,
		deposit_token_amount_before: deposit_token_amount_before.cast()?,
		amount,
	});

	Ok(())
}
