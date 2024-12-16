use anchor_lang::prelude::*;
use synth_market_map::get_writable_market_set;
use user::{ User, UserStats };
use vault::Vault;
use synth_market::VaultsConfig;

use crate::{ controller, state::*, State };

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct ResolveVaultBankruptcy<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_vault(&liquidator, &authority)?
    )]
	pub liquidator: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&liquidator, &liquidator_stats)?
    )]
	pub liquidator_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
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
	pub token_program: Interface<'info, TokenInterface>,
}

#[access_control(withdraw_not_paused(&ctx.accounts.state))]
pub fn handle_resolve_vault_bankruptcy<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, ResolveVaultBankruptcy<'info>>,
	vault_index: u16
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let user_key = ctx.accounts.user.key();
	let liquidator_key = ctx.accounts.liquidator.key();

	validate!(user_key != liquidator_key, ErrorCode::UserCantLiquidateThemself)?;

	let user = &mut load_mut!(ctx.accounts.user)?;
	let liquidator = &mut load_mut!(ctx.accounts.liquidator)?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { market_map, mut oracle_map } = load_maps(
		remaining_accounts_iter,
		&get_writable_market_set(market_index),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	let mint = get_token_mint(remaining_accounts_iter)?;

	// {
	// 	let spot_market = &mut spot_market_map.get_ref_mut(
	// 		&quote_spot_market_index
	// 	)?;
	// 	controller::insurance::attempt_settle_revenue_to_insurance_fund(
	// 		&ctx.accounts.spot_market_vault,
	// 		&ctx.accounts.insurance_fund_vault,
	// 		spot_market,
	// 		now,
	// 		&ctx.accounts.token_program,
	// 		&ctx.accounts.normal_signer,
	// 		state,
	// 		&mint
	// 	)?;

	// 	// reload the spot market vault balance so it's up-to-date
	// 	ctx.accounts.spot_market_vault.reload()?;
	// 	ctx.accounts.insurance_fund_vault.reload()?;
	// 	math::spot_withdraw::validate_spot_market_vault_amount(
	// 		spot_market,
	// 		ctx.accounts.spot_market_vault.amount
	// 	)?;
	// }

	let pay_from_insurance = controller::liquidation::resolve_vault_bankruptcy(
		market_index,
		user,
		&user_key,
		liquidator,
		&liquidator_key,
		&market_map,
		&mut oracle_map,
		now,
		ctx.accounts.insurance_fund_vault.amount
	)?;

	if pay_from_insurance > 0 {
		validate!(
			pay_from_insurance < ctx.accounts.insurance_fund_vault.amount,
			ErrorCode::InsufficientCollateral,
			"Insurance Fund balance InsufficientCollateral for payment: !{} < {}",
			pay_from_insurance,
			ctx.accounts.insurance_fund_vault.amount
		)?;

		controller::token::send_from_program_vault(
			&ctx.accounts.token_program,
			&ctx.accounts.insurance_fund_vault,
			&ctx.accounts.market_vault,
			&ctx.accounts.normal_signer,
			state.signer_nonce,
			pay_from_insurance,
			&mint
		)?;

		validate!(
			ctx.accounts.insurance_fund_vault.amount > 0,
			ErrorCode::InvalidIFDetected,
			"insurance_fund_vault.amount must remain > 0"
		)?;
	}

	// {
	// 	let spot_market = &mut spot_market_map.get_ref_mut(
	// 		&quote_spot_market_index
	// 	)?;
	// 	// reload the spot market vault balance so it's up-to-date
	// 	ctx.accounts.spot_market_vault.reload()?;
	// 	math::spot_withdraw::validate_spot_market_vault_amount(
	// 		spot_market,
	// 		ctx.accounts.spot_market_vault.amount
	// 	)?;
	// }

	Ok(())
}
