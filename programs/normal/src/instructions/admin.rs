use std::convert::identity;
use std::mem::size_of;

use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::{ Mint, TokenAccount, TokenInterface };
use pyth_solana_receiver_sdk::cpi::accounts::InitPriceUpdate;
use pyth_solana_receiver_sdk::program::PythSolanaReceiver;

use solana_program::msg;

use crate::controller::token::close_vault;
use crate::error::ErrorCode;
use crate::ids::admin_hot_wallet;
use crate::instructions::constraints::*;
use crate::instructions::optional_accounts::{ load_maps, AccountMaps };
use crate::math::casting::Cast;
use crate::constants::constants::{
	MAX_SQRT_K,
	MAX_UPDATE_K_PRICE_CHANGE,
	PERCENTAGE_PRECISION,
	QUOTE_SPOT_MARKET_INDEX,
	TWENTY_FOUR_HOUR,
	THIRTEEN_DAY,
};
use crate::math::cp_curve::get_update_k_result;
use crate::math::orders::is_multiple_of_step_size;
use crate::math::repeg::get_total_fee_lower_bound;
use crate::math::safe_math::SafeMath;
use crate::math::balance::get_token_amount;
use crate::math::{ amm, bn };
use crate::optional_accounts::get_token_mint;
use crate::state::events::CurveRecord;
use crate::state::oracle::get_sb_on_demand_price;
use crate::state::oracle::{
	get_oracle_price,
	get_pyth_price,
	get_switchboard_price,
	HistoricalIndexData,
	HistoricalOracleData,
	OraclePriceData,
	OracleSource,
};
use crate::state::oracle_map::OracleMap;
use crate::state::oracles::index_fund::IndexFund;
use crate::state::paused_operations::{ Operatio, InsuranceFundOperation };
use crate::state::market::{ AssetType, MarketStatus, Market, PoolBalance };
use crate::state::market_map::get_writable_market_set;
// use crate::state::insurance::{ SyntheticTier};
// use crate::state::market::{
//     SyntheticTier, SpotBalanceType, SpotFulfillmentConfigStatus, SpotMarket,
// };
use crate::state::amm::AMM;
use crate::state::state::{
	ExchangeStatus,
	FeeStructure,
	OracleGuardRails,
	State,
};
use crate::state::traits::Size;
use crate::state::user::{ User, UserStats };
use crate::state::insurance::InsuranceFund;
use crate::validate;
use crate::validation::fee_structure::validate_fee_structure;
use crate::validation::market::validate_market;
use crate::{ controller, QUOTE_PRECISION_I64 };
use crate::{ get_then_update_id, EPOCH_DURATION };
use crate::{ load, FEE_ADJUSTMENT_MAX };
use crate::{ load_mut, PTYH_PRICE_FEED_SEED_PREFIX };
use crate::{ math, safe_decrement, safe_increment };
use crate::{ math_error, SPOT_BALANCE_PRECISION };

#[access_control(
    market_valid(&ctx.accounts.market)
    valid_oracle_for_market(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_update_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
	// allow update to amm's oracle twap iff price gap is reduced and thus more tame funding
	// otherwise if oracle error or funding flip: set oracle twap to mark twap (0 gap)

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("updating amm oracle twap for perp market {}", market.market_index);
	let price_oracle = &ctx.accounts.oracle;
	let oracle_twap = market.amm.get_oracle_twap(price_oracle, clock.slot)?;

	if let Some(oracle_twap) = oracle_twap {
		let oracle_mark_gap_before = market.amm.last_mark_price_twap
			.cast::<i64>()?
			.safe_sub(market.amm.historical_oracle_data.last_oracle_price_twap)?;

		let oracle_mark_gap_after = market.amm.last_mark_price_twap
			.cast::<i64>()?
			.safe_sub(oracle_twap)?;

		if
			(oracle_mark_gap_after > 0 && oracle_mark_gap_before < 0) ||
			(oracle_mark_gap_after < 0 && oracle_mark_gap_before > 0)
		{
			msg!(
				"market.amm.historical_oracle_data.last_oracle_price_twap {} -> {}",
				market.amm.historical_oracle_data.last_oracle_price_twap,
				market.amm.last_mark_price_twap.cast::<i64>()?
			);
			msg!(
				"market.amm.historical_oracle_data.last_oracle_price_twap_ts {} -> {}",
				market.amm.historical_oracle_data.last_oracle_price_twap_ts,
				now
			);
			market.amm.historical_oracle_data.last_oracle_price_twap =
				market.amm.last_mark_price_twap.cast::<i64>()?;
			market.amm.historical_oracle_data.last_oracle_price_twap_ts = now;
		} else if
			oracle_mark_gap_after.unsigned_abs() <=
			oracle_mark_gap_before.unsigned_abs()
		{
			msg!(
				"market.amm.historical_oracle_data.last_oracle_price_twap {} -> {}",
				market.amm.historical_oracle_data.last_oracle_price_twap,
				oracle_twap
			);
			msg!(
				"market.amm.historical_oracle_data.last_oracle_price_twap_ts {} -> {}",
				market.amm.historical_oracle_data.last_oracle_price_twap_ts,
				now
			);
			market.amm.historical_oracle_data.last_oracle_price_twap = oracle_twap;
			market.amm.historical_oracle_data.last_oracle_price_twap_ts = now;
		} else {
			return Err(ErrorCode::PriceBandsBreached.into());
		}
	} else {
		return Err(ErrorCode::InvalidOracle.into());
	}

	Ok(())
}

#[access_control(
    market_valid(&ctx.accounts.market)
    valid_oracle_for_market(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_reset_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
	// admin failsafe to reset amm oracle_twap to the mark_twap

	let market = &mut load_mut!(ctx.accounts.market)?;

	msg!("resetting amm oracle twap for perp market {}", market.market_index);
	msg!(
		"market.amm.historical_oracle_data.last_oracle_price_twap: {:?} -> {:?}",
		market.amm.historical_oracle_data.last_oracle_price_twap,
		market.amm.last_mark_price_twap.cast::<i64>()?
	);

	msg!(
		"market.amm.historical_oracle_data.last_oracle_price_twap_ts: {:?} -> {:?}",
		market.amm.historical_oracle_data.last_oracle_price_twap_ts,
		market.amm.last_mark_price_twap_ts
	);

	market.amm.historical_oracle_data.last_oracle_price_twap =
		market.amm.last_mark_price_twap.cast::<i64>()?;
	market.amm.historical_oracle_data.last_oracle_price_twap_ts =
		market.amm.last_mark_price_twap_ts;

	Ok(())
}



#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_synthetic_tier(
	ctx: Context<AdminUpdateMarket>,
	synthetic_tier: SyntheticTier
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("market {}", market.market_index);

	msg!(
		"market.synthetic_tier: {:?} -> {:?}",
		market.synthetic_tier,
		synthetic_tier
	);

	market.synthetic_tier = synthetic_tier;

	let AccountMaps { market_map, oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&get_writable_market_set(market_index),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	let prev_max_insurance_claim_pct = market.max_insurance_claim_pct;
	controller::insurance::update_market_max_insurance_claim(&market_map);
	let new_max_insurance_claim_pct = market.max_insurance_claim_pct;

	msg!(
		"market.max_insurance_claim_pct: {} -> {}",
		prev_max_insurance_claim_pct,
		new_max_insurance_claim_pct
	);

	Ok(())
}


#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_step_size_and_tick_size(
	ctx: Context<AdminUpdateMarket>,
	step_size: u64,
	tick_size: u64
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("market {}", market.market_index);

	validate!(step_size > 0 && tick_size > 0, ErrorCode::DefaultError)?;
	validate!(step_size <= 2000000000, ErrorCode::DefaultError)?; // below i32 max for lp's remainder_base_asset

	msg!(
		"market.amm.order_step_size: {:?} -> {:?}",
		market.amm.order_step_size,
		step_size
	);

	msg!(
		"market.amm.order_tick_size: {:?} -> {:?}",
		market.amm.order_tick_size,
		tick_size
	);

	market.amm.order_step_size = step_size;
	market.amm.order_tick_size = tick_size;
	Ok(())
}


pub fn handle_transfer_fees_to_treasury<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, TransferFeesToTreasury<'info>>,
	market_index: u16
) -> Result<()> {
	let state = &ctx.accounts.state;
	let market = &mut load_mut!(ctx.accounts.market)?;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(
		market_index == market.market_index,
		ErrorCode::InvalidMarketAccount,
		"invalid market passed"
	)?;

	let market_vault_amount = ctx.accounts.market_vault.amount;
	let insurance_vault_amount = ctx.accounts.insurance_fund_vault.amount;
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	// uses proportion of revenue pool allocated to insurance fund
	let token_amount = controller::insurance::transfer_fees_to_treasury(
		market_vault_amount,
		insurance_vault_amount,
		market,
		insurance_fund,
		now,
		true
	)?;

	controller::token::send_from_program_vault(
		&ctx.accounts.token_program,
		&ctx.accounts.market_fee_pool,
		&ctx.accounts.treasury_vault,
		&ctx.accounts.normal_signer,
		state.signer_nonce,
		token_amount,
		&mint
	)?;

	// reload the market vault balance so it's up-to-date
	ctx.accounts.market_vault.reload()?;

	Ok(())
}

pub fn handle_burn_gov_token_with_fees<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, TransferFeesToTreasury<'info>>,
	market_index: u16
) -> Result<()> {
	// 1) Purchase NORM with fee amount

	// 2) Burn NORM - TODO: this is not complete
	controller::token::burn_tokens(
		&ctx.accounts.governance_token_program,
		&ctx.accounts.market_fee_pool,
		&ctx.accounts.market_vault,
		state.signer_nonce,
		token_amount,
		&mint
	);

	Ok(())
}





#[derive(Accounts)]
pub struct AdminUpdateMarketAmmSummaryStats<'info> {
	#[account(address = admin_hot_wallet::id())]
	pub admin: Signer<'info>,
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub market: AccountLoader<'info, Market>,
	/// CHECK: checked in `admin_update_perp_market_summary_stats` ix constraint
	pub oracle: AccountInfo<'info>,
}


#[derive(Accounts)]
pub struct AdminUpdateState<'info> {
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
}

#[derive(Accounts)]
pub struct AdminDisableBidAskTwapUpdate<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub user_stats: AccountLoader<'info, UserStats>,
}



#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct TransferFeesToTreasury<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"market", market_index.to_le_bytes().as_ref()],
        bump
    )]
	pub market: AccountLoader<'info, Market>,
	#[account(
        mut,
        seeds = [b"market_fee_pool".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub market_fee_pool: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(
        mut,
        seeds = [b"insurance_fund"],
        bump
    )]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        seeds = [b"treasury_vault".as_ref()],
        bump,
    )]
	pub treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct BurnGovTokenWithFees<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"market", market_index.to_le_bytes().as_ref()],
        bump
    )]
	pub market: AccountLoader<'info, Market>,
	#[account(
        mut,
        seeds = [b"market_fee_pool".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub market_fee_pool: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(
        mut,
        seeds = [b"insurance_fund"],
        bump
    )]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        seeds = [b"treasury_vault".as_ref()],
        bump,
    )]
	pub treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}
