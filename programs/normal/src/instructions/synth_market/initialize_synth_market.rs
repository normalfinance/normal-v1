use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };

use synth_market::{ SynthMarket };
use oracle_map::OracleMap;

use crate::{
	state::{ synth_market::{ Collateral, Synthetic }, * },
	validation::margin::validate_margin,
	State,
};

#[derive(Accounts)]
pub struct AdminUpdateSynthMarket<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub synth_market: AccountLoader<'info, SynthMarket>,
}

#[derive(Accounts)]
pub struct InitializeSynthMarket<'info> {
	#[account(mut)]
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
	#[account(
		init,
		seeds = [b"synth_market", state.number_of_markets.to_le_bytes().as_ref()],
		space = Market::SIZE,
		bump,
		payer = admin
	)]
	pub synth_market: AccountLoader<'info, SynthMarket>,

	pub token_mint_collateral: Account<'info, Mint>,
	pub token_mint_synthetic: Account<'info, Mint>,

	#[account(
		init,
		payer = admin,
		token::mint = token_mint_collateral,
		token::authority = market
	)]
	pub token_vault_collateral: Box<Account<'info, TokenAccount>>,

	#[account(
		init,
		payer = admin,
		token::mint = token_mint_synthetic,
		token::authority = market
	)]
	pub token_vault_synthetic: Box<Account<'info, TokenAccount>>,

	/// CHECK: checked in `initialize_market`
	pub oracle: AccountInfo<'info>,
	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

pub fn handle_initialize_synth_market(
	ctx: Context<InitializeSynthMarket>,
	// market_index: u16,
	// name: [u8; 32],
	// active_status: bool,
	// synth_tier: SyntheticTier,

	// // Oracle
	// oracle_source: OracleSource,

	// // Margin
	// margin_ratio_initial: u32,
	// margin_ratio_maintenance: u32,
	// imf_factor: u32,

	// // Liquidation
	// liquidation_penalty: u32,
	// liquidator_fee: u32,
	// insurance_fund_liquidation_fee: u32,
	// debt_ceiling: u128,
	// debt_floor: u32,

	// // Insurance
	// max_revenue_withdraw_per_period: u64,
	// quote_max_insurance: u64
) -> Result<()> {
	msg!("market {}", market_index);
	let market_pubkey = ctx.accounts.market.to_account_info().key;
	let market = &mut ctx.accounts.market.load_init()?;
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let clock_slot = clock.slot;

	OracleMap::validate_oracle_account_info(&ctx.accounts.oracle)?;

	// Verify oracle is readable
	let (oracle_price, oracle_delay, last_oracle_price_twap) = match
		oracle_source
	{
		OracleSource::Pyth => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_pyth_price(&ctx.accounts.oracle, clock_slot, 1, false)?;
			let last_oracle_price_twap = amm.get_pyth_twap(
				&ctx.accounts.oracle,
				1,
				false
			)?;
			(oracle_price, oracle_delay, last_oracle_price_twap)
		}
		OracleSource::Pyth1K => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_pyth_price(&ctx.accounts.oracle, clock_slot, 1000, false)?;
			let last_oracle_price_twap = amm.get_pyth_twap(
				&ctx.accounts.oracle,
				1000,
				false
			)?;
			(oracle_price, oracle_delay, last_oracle_price_twap)
		}
		OracleSource::Pyth1M => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_pyth_price(&ctx.accounts.oracle, clock_slot, 1000000, false)?;
			let last_oracle_price_twap = amm.get_pyth_twap(
				&ctx.accounts.oracle,
				1000000,
				false
			)?;
			(oracle_price, oracle_delay, last_oracle_price_twap)
		}
		OracleSource::PythStableCoin => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_pyth_price(&ctx.accounts.oracle, clock_slot, 1, false)?;
			(oracle_price, oracle_delay, QUOTE_PRECISION_I64)
		}
		OracleSource::QuoteAsset => {
			msg!("Quote asset oracle cant be used for perp market");
			return Err(ErrorCode::InvalidOracle.into());
		}
		OracleSource::PythPull => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_pyth_price(&ctx.accounts.oracle, clock_slot, 1, true)?;
			let last_oracle_price_twap = amm.get_pyth_twap(
				&ctx.accounts.oracle,
				1,
				true
			)?;
			(oracle_price, oracle_delay, last_oracle_price_twap)
		}
		OracleSource::Pyth1KPull => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_pyth_price(&ctx.accounts.oracle, clock_slot, 1000, true)?;
			let last_oracle_price_twap = amm.get_pyth_twap(
				&ctx.accounts.oracle,
				1000,
				true
			)?;
			(oracle_price, oracle_delay, last_oracle_price_twap)
		}
		OracleSource::Pyth1MPull => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_pyth_price(&ctx.accounts.oracle, clock_slot, 1000000, true)?;
			let last_oracle_price_twap = amm.get_pyth_twap(
				&ctx.accounts.oracle,
				1000000,
				true
			)?;
			(oracle_price, oracle_delay, last_oracle_price_twap)
		}
		OracleSource::PythStableCoinPull => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_pyth_price(&ctx.accounts.oracle, clock_slot, 1, true)?;
			(oracle_price, oracle_delay, QUOTE_PRECISION_I64)
		}
	};

	validate_margin(
		margin_ratio_initial,
		margin_ratio_maintenance,
		liquidator_fee
	)?;

	let state = &mut ctx.accounts.state;
	validate!(
		market_index == state.number_of_markets,
		ErrorCode::MarketIndexAlreadyInitialized,
		"market_index={} != state.number_of_markets={}",
		market_index,
		state.number_of_markets
	)?;

	// **market = SynthMarket {
	// 	pubkey: *market_pubkey,
	// 	market_index,
	// 	name,
	// 	status: if active_status {
	// 		MarketStatus::Active
	// 	} else {
	// 		MarketStatus::Initialized
	// 	},
	// 	synth_tier,
	// 	paused_operations: 0,
	// 	number_of_users: 0,

	// 	synthetic: Synthetic {},
	// 	collateral: Collateral {},

	// 	// Collateral
	// 	token_mint_collateral: ctx.accounts.token_mint_collateral.key(),
	// 	token_vault_synthetic: ctx.accounts.token_vault_synthetic.key(),
	// 	token_vault_collateral: ctx.accounts.token_vault_collateral.key(),

	// 	// Liquidation
	// 	liquidation_penalty,
	// 	liquidator_fee,
	// 	if_liquidation_fee,
	// 	margin_ratio_initial, // unit is 20% (+2 decimal places)
	// 	margin_ratio_maintenance,
	// 	imf_factor,
	// 	debt_ceiling,
	// 	debt_floor,
	// 	collateral_lending_utilization: 0,

	// 	// Auction
	// 	collateral_action_config: AuctionConfig {
	// 		auction_location: collateral_auction_location,
	// 		..AuctionConfig::default()
	// 	},

	// 	// AMM
	// 	amm: AMM {
	// 		oracle: 0,
	// 		oracle_source,
	// 		historical_oracle_data: HistoricalOracleData::default(),
	// 		last_oracle_conf_pct: 0,
	// 		last_oracle_valid: false,
	// 		last_oracle_normalised_price: 0,
	// 		last_oracle_reserve_price_spread_pct: 0,
	// 		oracle_std: 0,

	// 		tick_spacing,
	// 		tick_spacing_seed: tick_spacing.to_le_bytes(),

	// 		liquidity: 0,
	// 		sqrt_price,
	// 		tick_current_index: tick_index_from_sqrt_price(&sqrt_price),

	// 		fee_rate: 0,
	// 		protocol_fee_rate: 0,

	// 		protocol_fee_owed_synthetic: 0,
	// 		protocol_fee_owed_quote: 0,

	// 		token_mint_synthetic,
	// 		token_vault_synthetic,
	// 		fee_growth_global_synthetic: 0,

	// 		token_mint_quote,
	// 		token_vault_quote,
	// 		fee_growth_global_quote: 0,

	// 		reward_authority: Pubkey::default(),
	// 		reward_infos: [],
	// 	},

	// 	// Insurance
	// 	insurance_claim: InsuranceClaim {
	// 		max_revenue_withdraw_per_period,
	// 		quote_max_insurance,
	// 		..InsuranceClaim::default()
	// 	},

	// 	// Metrics
	// 	outstanding_debt: 0,
	// 	protocol_debt: 0,

	// 	// Market settlement
	// 	expiry_price: 0,
	// 	expiry_ts: 0,

	// 	total_gov_token_inflation: 0,

	// 	padding: [0; 43],
	// };

	safe_increment!(state.number_of_markets, 1);

	Ok(())
}
