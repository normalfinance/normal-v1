use std::cell::RefMut;

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{ TokenAccount, TokenInterface };
use solana_program::instruction::Instruction;
use solana_program::sysvar::instructions::{
	load_current_index_checked,
	load_instruction_at_checked,
	ID as IX_ID,
};

use crate::controller::position::PositionDirection;
use crate::error::ErrorCode;
use crate::ids::swift_server;
use crate::instructions::constraints::*;
use crate::instructions::optional_accounts::{ load_maps, AccountMaps };
use crate::math::casting::Cast;
use crate::math::constants::QUOTE_SPOT_MARKET_INDEX;
use crate::math::orders::{
	estimate_price_from_side,
	find_bids_and_asks_from_users,
};
use crate::optional_accounts::{ get_token_mint };
use crate::state::fill_mode::FillMode;
use crate::state::spot_fulfillment_params::normal::MatchFulfillmentParams;
use crate::state::oracle_map::OracleMap;
use crate::state::order_params::{
	OrderParams,
	PlaceOrderOptions,
	SwiftOrderParamsMessage,
	SwiftServerMessage,
};
use crate::state::paused_operations::PerpOperation;
use crate::state::market::{ AssetType, MarketStatus, Market };
use crate::state::market_map::{
	get_market_set_for_user_positions,
	get_market_set_from_list,
	get_writable_market_set,
	get_writable_market_set_from_vec,
	MarketSet,
	MarketMap,
};
use crate::state::spot_fulfillment_params::FulfillmentParams;

use crate::state::state::State;
use crate::state::user::{
	MarketType,
	OrderStatus,
	OrderTriggerCondition,
	OrderType,
	User,
	UserStats,
};
use crate::state::user_map::{
	load_user_map,
	load_user_maps,
	UserMap,
	UserStatsMap,
};
use crate::validation::sig_verification::verify_ed25519_ix;
use crate::validation::user::validate_user_is_idle;
use crate::{ controller, load, math, print_error, OracleSource };
use crate::{ load_mut, QUOTE_PRECISION_U64 };
use crate::{ validate, QUOTE_PRECISION_I128 };

#[access_control(fill_not_paused(&ctx.accounts.state))]
pub fn handle_fill_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, FillOrder<'info>>,
	order_id: Option<u32>
) -> Result<()> {
	let (order_id, market_index) = {
		let user = &load!(ctx.accounts.user)?;
		// if there is no order id, use the users last order id
		let order_id = order_id.unwrap_or_else(|| user.get_last_order_id());
		let market_index = match user.get_order(order_id) {
			Some(order) => order.market_index,
			None => {
				msg!("Order does not exist {}", order_id);
				return Ok(());
			}
		};
		(order_id, market_index)
	};

	let user_key = &ctx.accounts.user.key();
	fill_order(ctx, order_id, market_index).map_err(|e| {
		msg!(
			"Err filling order id {} for user {} for market index {}",
			order_id,
			user_key,
			market_index
		);
		e
	})?;

	Ok(())
}

fn fill_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, FillOrder<'info>>,
	order_id: u32,
	market_index: u16
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { market_map, mut oracle_map } = load_maps(
		remaining_accounts_iter,
		&get_writable_market_set(market_index),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	let (makers_and_referrer, makers_and_referrer_stats) = load_user_maps(
		remaining_accounts_iter,
		true
	)?;

	controller::repeg::update_amm(
		market_index,
		&market_map,
		&mut oracle_map,
		&ctx.accounts.state,
		clock
	)?;

	controller::orders::fill_order(
		order_id,
		&ctx.accounts.state,
		&ctx.accounts.user,
		&ctx.accounts.user_stats,
		&market_map,
		&mut oracle_map,
		&ctx.accounts.filler,
		&ctx.accounts.filler_stats,
		&makers_and_referrer,
		&makers_and_referrer_stats,
		None,
		clock,
		FillMode::Fill
	)?;

	Ok(())
}

#[access_control(fill_not_paused(&ctx.accounts.state))]
pub fn handle_revert_fill<'info>(ctx: Context<RevertFill>) -> Result<()> {
	let filler = load_mut!(ctx.accounts.filler)?;
	let clock = Clock::get()?;

	validate!(
		filler.last_active_slot == clock.slot,
		ErrorCode::RevertFill,
		"filler last active slot ({}) != current slot ({})",
		filler.last_active_slot,
		clock.slot
	)?;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_trigger_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, TriggerOrder<'info>>,
	order_id: u32
) -> Result<()> {
	let (market_type, market_index) = match
		load!(ctx.accounts.user)?.get_order(order_id)
	{
		Some(order) => (order.market_type, order.market_index),
		None => {
			msg!("order_id not found {}", order_id);
			return Ok(());
		}
	};

	let writeable_markets = match market_type {
		MarketType::Synthetic => MarketSet::new(),
	};

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&writeable_markets,
		Clock::get()?.slot,
		None
	)?;

	controller::orders::trigger_order(
		order_id,
		&ctx.accounts.state,
		&ctx.accounts.user,
		&market_map,
		&mut oracle_map,
		&ctx.accounts.filler,
		&Clock::get()?
	)?;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_force_cancel_orders<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, ForceCancelOrder>
) -> Result<()> {
	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		Clock::get()?.slot,
		None
	)?;

	controller::orders::force_cancel_orders(
		&ctx.accounts.state,
		&ctx.accounts.user,
		&market_map,
		&mut oracle_map,
		&ctx.accounts.filler,
		&Clock::get()?
	)?;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_update_user_idle<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, UpdateUserIdle<'info>>
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;
	let clock = Clock::get()?;

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		Clock::get()?.slot,
		None
	)?;

	let (equity, _) = calculate_user_equity(&user, &market_map, &mut oracle_map)?;

	// user flipped to idle faster if equity is less than 1000
	let accelerated = equity < QUOTE_PRECISION_I128 * 1000;

	validate_user_is_idle(&user, clock.slot, accelerated)?;

	user.idle = true;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_update_user_open_orders_count<'info>(
	ctx: Context<UpdateUserIdle>
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;

	let mut open_orders = 0_u8;
	let mut open_auctions = 0_u8;

	for order in user.orders.iter() {
		if order.status == OrderStatus::Open {
			open_orders += 1;
		}

		if order.has_auction() {
			open_auctions += 1;
		}
	}

	user.open_orders = open_orders;
	user.has_open_order = open_orders > 0;
	user.open_auctions = open_auctions;
	user.has_open_auction = open_auctions > 0;

	Ok(())
}

pub fn handle_place_swift_taker_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, PlaceSwiftTakerOrder<'info>>,
	swift_message_bytes: Vec<u8>,
	swift_order_params_message_bytes: Vec<u8>,
	sig: [u8; 64]
) -> Result<()> {
	let swift_message: SwiftServerMessage = SwiftServerMessage::deserialize(
		&mut &swift_message_bytes[..]
	).unwrap();
	let taker_order_params_message: SwiftOrderParamsMessage =
		SwiftOrderParamsMessage::deserialize(
			&mut &swift_order_params_message_bytes[..]
		).unwrap();

	let state = &ctx.accounts.state;

	// TODO: generalize to support multiple market types
	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		Clock::get()?.slot,
		Some(state.oracle_guard_rails)
	)?;

	let taker_key = ctx.accounts.user.key();
	let mut taker = load_mut!(ctx.accounts.user)?;

	place_swift_taker_order(
		taker_key,
		&mut taker,
		swift_message,
		taker_order_params_message,
		&ctx.accounts.ix_sysvar.to_account_info(),
		sig,
		&market_map,
		&mut oracle_map,
		state
	)?;
	Ok(())
}

pub fn place_swift_taker_order<'c: 'info, 'info>(
	taker_key: Pubkey,
	taker: &mut RefMut<User>,
	swift_message: SwiftServerMessage,
	taker_order_params_message: SwiftOrderParamsMessage,
	ix_sysvar: &AccountInfo<'info>,
	sig: [u8; 64],
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	state: &State
) -> Result<()> {
	#[cfg(all(feature = "mainnet-beta", not(feature = "anchor-test")))]
	{
		panic!("Swift orders are disabled on mainnet-beta");
	}

	// Authenticate the swift param message
	let ix_idx = load_current_index_checked(ix_sysvar)?;
	validate!(
		ix_idx > 1,
		ErrorCode::InvalidVerificationIxIndex,
		"instruction index must be greater than 1 for two sig verifies"
	)?;
	let ix: Instruction = load_instruction_at_checked(
		(ix_idx as usize) - 2,
		ix_sysvar
	)?;
	verify_ed25519_ix(
		&ix,
		&swift_server::id().to_bytes(),
		&swift_message.clone().try_to_vec()?,
		&sig
	)?;

	let ix: Instruction = load_instruction_at_checked(
		(ix_idx as usize) - 1,
		ix_sysvar
	)?;
	verify_ed25519_ix(
		&ix,
		&taker.authority.to_bytes(),
		&taker_order_params_message.clone().try_to_vec()?,
		&swift_message.swift_order_signature
	)?;

	let clock = &Clock::get()?;

	// First order must be a taker order
	let matching_taker_order_params =
		&taker_order_params_message.swift_order_params;
	if
		matching_taker_order_params.order_type != OrderType::Market ||
		matching_taker_order_params.market_type != MarketType::Synthetic
	{
		msg!("First order must be a market synthetic taker order");
		return Err(print_error!(ErrorCode::InvalidSwiftOrderParam)().into());
	}

	let market_index = matching_taker_order_params.market_index;
	let expected_order_id = taker_order_params_message.expected_order_id;
	let taker_next_order_id = taker.next_order_id;
	let order_slot = swift_message.slot;
	if expected_order_id.cast::<u32>()? != taker_next_order_id {
		msg!(
			"Orders not placed due to taker order id mismatch: taker account next order id {}, order params expected next order id {:?}",
			taker.next_order_id,
			expected_order_id
		);
		return Ok(());
	}
	controller::orders::place_order(
		state,
		taker,
		taker_key,
		market_map,
		oracle_map,
		clock,
		*matching_taker_order_params,
		PlaceOrderOptions {
			swift_taker_order_slot: Some(order_slot),
			..PlaceOrderOptions::default()
		}
	)?;

	if
		let Some(stop_loss_order_params) =
			taker_order_params_message.stop_loss_order_params
	{
		let stop_loss_order = OrderParams {
			order_type: OrderType::TriggerMarket,
			direction: matching_taker_order_params.direction.opposite(),
			trigger_price: Some(stop_loss_order_params.trigger_price),
			base_asset_amount: stop_loss_order_params.base_asset_amount,
			trigger_condition: if
				matching_taker_order_params.direction == PositionDirection::Long
			{
				OrderTriggerCondition::Below
			} else {
				OrderTriggerCondition::Above
			},
			market_index,
			market_type: MarketType::Synthetic,
			reduce_only: true,
			..OrderParams::default()
		};

		controller::orders::place_order(
			state,
			taker,
			taker_key,
			market_map,
			oracle_map,
			clock,
			stop_loss_order,
			PlaceOrderOptions {
				..PlaceOrderOptions::default()
			}
		)?;
	}

	if
		let Some(take_profit_order_params) =
			taker_order_params_message.take_profit_order_params
	{
		let take_profit_order = OrderParams {
			order_type: OrderType::TriggerMarket,
			direction: matching_taker_order_params.direction.opposite(),
			trigger_price: Some(take_profit_order_params.trigger_price),
			base_asset_amount: take_profit_order_params.base_asset_amount,
			trigger_condition: if
				matching_taker_order_params.direction == PositionDirection::Long
			{
				OrderTriggerCondition::Above
			} else {
				OrderTriggerCondition::Below
			},
			market_index,
			market_type: MarketType::Synthetic,
			reduce_only: true,
			..OrderParams::default()
		};

		controller::orders::place_order(
			state,
			taker,
			taker_key,
			market_map,
			oracle_map,
			clock,
			take_profit_order,
			PlaceOrderOptions {
				swift_taker_order_slot: Some(order_slot),
				..PlaceOrderOptions::default()
			}
		)?;
	}

	Ok(())
}

#[access_control(amm_not_paused(&ctx.accounts.state))]
pub fn handle_settle_lp<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, SettleLP>,
	market_index: u16
) -> Result<()> {
	let user_key = ctx.accounts.user.key();
	let user = &mut load_mut!(ctx.accounts.user)?;

	let state = &ctx.accounts.state;
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let AccountMaps { market_map, .. } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&get_writable_market_set(market_index),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	let market = &mut market_map.get_ref_mut(&market_index)?;
	controller::lp::settle_lp(user, &user_key, market, now)?;
	user.update_last_active_slot(clock.slot);

	Ok(())
}

#[access_control(
    market_valid(&ctx.accounts.market)
    funding_not_paused(&ctx.accounts.state)
    valid_oracle_for_market(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_update_bid_ask_twap<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, UpdateBidAskTwap<'info>>
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let slot = clock.slot;
	let state = &ctx.accounts.state;
	let mut oracle_map = OracleMap::load_one(
		&ctx.accounts.oracle,
		slot,
		Some(state.oracle_guard_rails)
	)?;

	let keeper_stats = load!(ctx.accounts.keeper_stats)?;
	validate!(
		!keeper_stats.disable_update_bid_ask_twap,
		ErrorCode::CantUpdatePerpBidAskTwap,
		"Keeper stats disable_update_bid_ask_twap is true"
	)?;

	let min_if_stake = 1000 * QUOTE_PRECISION_U64;
	validate!(
		keeper_stats.if_staked_quote_asset_amount >= min_if_stake,
		ErrorCode::CantUpdatePerpBidAskTwap,
		"Keeper doesnt have min if stake. stake = {} min if stake = {}",
		keeper_stats.if_staked_quote_asset_amount,
		min_if_stake
	)?;

	let oracle_price_data = oracle_map.get_price_data(&market.amm.oracle)?;
	controller::repeg::_update_amm(market, oracle_price_data, state, now, slot)?;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let makers = load_user_map(remaining_accounts_iter, false)?;

	let depth = market.get_market_depth_for_funding_rate()?;

	let (bids, asks) = find_bids_and_asks_from_users(
		market,
		oracle_price_data,
		&makers,
		slot,
		now
	)?;
	let estimated_bid = estimate_price_from_side(&bids, depth)?;
	let estimated_ask = estimate_price_from_side(&asks, depth)?;

	msg!(
		"estimated_bid = {:?} estimated_ask = {:?}",
		estimated_bid,
		estimated_ask
	);

	msg!(
		"before amm bid twap = {} ask twap = {} ts = {}",
		market.amm.last_bid_price_twap,
		market.amm.last_ask_price_twap,
		market.amm.last_mark_price_twap_ts
	);

	let sanitize_clamp_denominator = market.get_sanitize_clamp_denominator()?;
	math::amm::update_mark_twap_crank(
		&mut market.amm,
		now,
		oracle_price_data,
		estimated_bid,
		estimated_ask,
		sanitize_clamp_denominator
	)?;

	msg!(
		"after amm bid twap = {} ask twap = {} ts = {}",
		market.amm.last_bid_price_twap,
		market.amm.last_ask_price_twap,
		market.amm.last_mark_price_twap_ts
	);

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_update_amms<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, UpdateAMM<'info>>,
	market_indexes: [u16; 5]
) -> Result<()> {
	// up to ~60k compute units (per amm) worst case

	let clock = Clock::get()?;

	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let oracle_map = &mut OracleMap::load(
		remaining_accounts_iter,
		clock.slot,
		None
	)?;
	let market_map = &mut MarketMap::load(
		&get_market_set_from_list(market_indexes),
		remaining_accounts_iter
	)?;

	controller::repeg::update_amms(market_map, oracle_map, state, &clock)?;

	Ok(())
}

#[derive(Accounts)]
pub struct FillOrder<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_user(&filler, &authority)?
    )]
	pub filler: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&filler, &filler_stats)?
    )]
	pub filler_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
}

#[derive(Accounts)]
pub struct RevertFill<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_user(&filler, &authority)?
    )]
	pub filler: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&filler, &filler_stats)?
    )]
	pub filler_stats: AccountLoader<'info, UserStats>,
}

#[derive(Accounts)]
pub struct TriggerOrder<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_user(&filler, &authority)?
    )]
	pub filler: AccountLoader<'info, User>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
}

#[derive(Accounts)]
pub struct ForceCancelOrder<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_user(&filler, &authority)?
    )]
	pub filler: AccountLoader<'info, User>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
}

#[derive(Accounts)]
pub struct UpdateUserIdle<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_user(&filler, &authority)?
    )]
	pub filler: AccountLoader<'info, User>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
}

#[derive(Accounts)]
pub struct PlaceSwiftTakerOrder<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	/// CHECK: The address check is needed because otherwise
	/// the supplied Sysvar could be anything else.
	/// The Instruction Sysvar has not been implemented
	/// in the Anchor framework yet, so this is the safe approach.
	#[account(address = IX_ID)]
	pub ix_sysvar: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct SettleLP<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
}

#[derive(Accounts)]
pub struct UpdateAMM<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdateBidAskTwap<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub market: AccountLoader<'info, Market>,
	/// CHECK: checked in `update_funding_rate` ix constraint
	pub oracle: AccountInfo<'info>,
	pub keeper_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
}
