use crate::state::*;
use amm::AMM;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };

#[derive(Accounts)]
#[instruction(tick_spacing: u16)]
pub struct InitializeAMM<'info> {
	pub token_mint_a: Account<'info, Mint>,
	pub token_mint_b: Account<'info, Mint>,

	#[account(mut)]
	pub funder: Signer<'info>,

	#[account(
		init,
		seeds = [
			b"amm".as_ref(),
			token_mint_a.key().as_ref(),
			token_mint_b.key().as_ref(),
			tick_spacing.to_le_bytes().as_ref(),
		],
		bump,
		payer = funder,
		space = Whirlpool::LEN
	)]
	pub whirlpool: Box<Account<'info, Whirlpool>>,

	#[account(
		init,
		payer = funder,
		token::mint = token_mint_a,
		token::authority = whirlpool
	)]
	pub token_vault_a: Box<Account<'info, TokenAccount>>,

	#[account(
		init,
		payer = funder,
		token::mint = token_mint_b,
		token::authority = whirlpool
	)]
	pub token_vault_b: Box<Account<'info, TokenAccount>>,

	#[account(
		has_one = whirlpools_config,
		constraint = fee_tier.tick_spacing == tick_spacing
	)]
	pub fee_tier: Account<'info, FeeTier>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
	pub system_program: Program<'info, System>,
	pub rent: Sysvar<'info, Rent>,
}

pub fn handle_initialize_amm(
	ctx: Context<InitializeAMM>,
	tick_spacing: u16,
	initial_sqrt_price: u128,
	oracle_source: OracleSource,
	fee_rate: u16,
	protocol_fee_rate: u16,
    max_price_variance: u16,
) -> Result<()> {
	let token_mint_a = ctx.accounts.token_mint_a.key();
	let token_mint_b = ctx.accounts.token_mint_b.key();

	let whirlpool = &mut ctx.accounts.whirlpool;
	let whirlpools_config = &ctx.accounts.whirlpools_config;

	// AMM validations
	if token_mint_synthetic.ge(&token_mint_quote) {
		return Err(ErrorCode::InvalidTokenMintOrder.into());
	}

	if !(MIN_SQRT_PRICE_X64..=MAX_SQRT_PRICE_X64).contains(&sqrt_price) {
		return Err(ErrorCode::SqrtPriceOutOfBounds.into());
	}

	if fee_rate > MAX_FEE_RATE {
		return Err(ErrorCode::FeeRateMaxExceeded.into());
	}
	if protocol_fee_rate > MAX_PROTOCOL_FEE_RATE {
		return Err(ErrorCode::ProtocolFeeRateMaxExceeded.into());
	}

	**amm = AMM {
		token_mint_synthetic: ctx.accounts.token_mint_synthetic.key(),
		token_vault_synthetic: ctx.accounts.token_vault_synthetic.key(),
		token_mint_quote: ctx.accounts.token_mint_quote.key(),
		token_vault_quote: ctx.accounts.token_vault_quote.key(),

        // Peg
        max_price_variance,
        liquidity_to_volume_multiplier: 0,

		// Oracle
		oracle: *ctx.accounts.oracle.key,
		oracle_source,
		historical_oracle_data: HistoricalOracleData::default(),
		last_oracle_conf_pct: 0,
		last_oracle_valid: false,
		last_oracle_normalised_price: 0,
		last_oracle_reserve_price_spread_pct: 0,
		oracle_std: 0,

		// Liquidity
		sqrt_price: initial_sqrt_price,
		liquidity: 0,
		tick_spacing,
		tick_spacing_seed: tick_spacing.to_le_bytes(),
		tick_current_index: math::amm::tick_index_from_sqrt_price(
			&initial_sqrt_price
		),

		// Fees
		fee_rate,
		protocol_fee_rate,
		protocol_fee_owed_synthetic: 0,
		protocol_fee_owed_quote: 0,
		fee_growth_global_synthetic: 0,
		fee_growth_global_quote: 0,

		// Rewards
		reward_infos: [
			AMMRewardInfo::new(state.reward_emissions_super_authority);
			NUM_REWARDS
		],
	};

	Ok(())
}
