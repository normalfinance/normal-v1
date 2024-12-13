use crate::{ load_mut, math, state::*, util::transfer_from_vault_to_owner, State };
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };
use index_market::IndexMarket;

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct CollectIndexFundProtocolFees<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"index_market", market_index.to_le_bytes().as_ref()],
        bump
    )]
	pub index_market: AccountLoader<'info, IndexMarket>,

	#[account(
        mut,
        seeds = [b"index_market_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub index_market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,

	// TODO: insert token destination

	pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_collect_index_market_protocol_fees<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, CollectIndexFundProtocolFees<'info>>,
	market_index: u16
) -> Result<()> {
	let state = &ctx.accounts.state;
	let index_market = &mut load_mut!(ctx.accounts.index_market)?;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(
		market_index == index_market.market_index,
		ErrorCode::InvalidSpotMarketAccount,
		"invalid index_market passed"
	)?;

	let index_vault_amount = ctx.accounts.index_market_vault.amount;
	let insurance_vault_amount = ctx.accounts.insurance_fund_vault.amount;

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let time_until_next_update = math::helpers::on_the_hour_update(
		now,
		index_market.insurance_fund.last_revenue_settle_ts,
		index_market.insurance_fund.revenue_settle_period
	)?;

	validate!(
		time_until_next_update == 0,
		ErrorCode::RevenueSettingsCannotSettleToIF,
		"Must wait {} seconds until next available settlement time",
		time_until_next_update
	)?;

	// uses proportion of revenue pool allocated to insurance fund
	let token_amount = controller::insurance::settle_revenue_to_insurance_fund(
		index_vault_amount,
		insurance_vault_amount,
		index_market,
		now,
		true
	)?;

	spot_market.insurance_fund.last_revenue_settle_ts = now;

	controller::token::send_from_program_vault(
		&ctx.accounts.token_program,
		&ctx.accounts.index_market_vault,
		&ctx.accounts.insurance_fund_vault, // TODO: update
		&ctx.accounts.normal_signer,
		state.signer_nonce,
		token_amount,
		&mint
	)?;

	// reload the index market vault balance so it's up-to-date
	ctx.accounts.index_market_vault.reload()?;
	math::spot_withdraw::validate_spot_market_vault_amount(
		spot_market,
		ctx.accounts.spot_market_vault.amount
	)?;

	Ok(())
}
