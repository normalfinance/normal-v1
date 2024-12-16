use amm::AMM;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };

use index_market::{ IndexAsset, IndexMarket, IndexVisibility };
use synth_market::{ AuctionConfig, AuctionPreference, Market };
use oracle_map::OracleMap;

use crate::{ state::*, validation::margin::validate_margin, State };

#[derive(Accounts)]
pub struct InitializeIndexMarket<'info> {
	#[account(mut)]
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub authority: Signer<'info>,
	#[account(
		init,
		seeds = [b"index_market", state.number_of_markets.to_le_bytes().as_ref()],
		space = IndexMarket::SIZE,
		bump,
		payer = authority
	)]
	pub index_market: AccountLoader<'info, IndexMarket>,

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

pub fn handle_initialize_index_market(
	ctx: Context<InitializeIndexMarket>,
	market_index: u16,
	name: [u8; 32],
	active_status: bool,
	synthetic_tier: SyntheticTier,

	// Oracle
	oracle_source: OracleSource,

	// Index
	visibility: IndexVisibility,
	assets: Vec<IndexAsset>,
	manager_fee: u16,

	// Insurance
	max_revenue_withdraw_per_period: u64,
	quote_max_insurance: u64
) -> Result<()> {
	msg!("index_market {}", index_market_index);
	let index_market_pubkey = ctx.accounts.index_market.to_account_info().key;
	let index_market = &mut ctx.accounts.index_market.load_init()?;
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
			let last_oracle_price_twap = synth_amm.get_pyth_twap(
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
			let last_oracle_price_twap = synth_amm.get_pyth_twap(
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
			let last_oracle_price_twap = synth_amm.get_pyth_twap(
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
		OracleSource::Switchboard => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_switchboard_price(&ctx.accounts.oracle, clock_slot)?;

			(oracle_price, oracle_delay, oracle_price)
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
			let last_oracle_price_twap = synth_amm.get_pyth_twap(
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
			let last_oracle_price_twap = synth_amm.get_pyth_twap(
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
			let last_oracle_price_twap = synth_amm.get_pyth_twap(
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
		OracleSource::SwitchboardOnDemand => {
			let OraclePriceData {
				price: oracle_price,
				delay: oracle_delay,
				..
			} = get_sb_on_demand_price(&ctx.accounts.oracle, clock_slot)?;

			(oracle_price, oracle_delay, oracle_price)
		}
	};

	let state = &mut ctx.accounts.state;
	validate!(
		market_index == state.number_of_markets,
		ErrorCode::MarketIndexAlreadyInitialized,
		"market_index={} != state.number_of_markets={}",
		market_index,
		state.number_of_markets
	)?;

	**index_market = IndexMarket {
		pubkey: *index_market_pubkey,
		market_index,
		name,
		status: if active_status {
			MarketStatus::Active
		} else {
			MarketStatus::Initialized
		},
		paused_operations: 0,
		number_of_users: 0,

		// Oracle
		oracle: ctx.accounts.oracle.key(),
		oracle_source,

		// Accounts
		vault: ctx.accounts.vault.key(),
		token_mint: ctx.accounts.token_mint.key(),

		// Index
		visibility,
		assets,
		manager_fee,

		// Metrics

		// Market settlement
		expiry_price: 0,
		expiry_ts: 0,

		padding: [0; 43],
	};

	safe_increment!(state.number_of_index_markets, 1);

	Ok(())
}
