use anchor_lang::prelude::*;
use anchor_lang::Discriminator;
use anchor_spl::{
	token::Token,
	token_2022::Token2022,
	token_interface::{ TokenAccount, TokenInterface },
};
use solana_program::program::invoke;
use solana_program::system_instruction::transfer;

use crate::controller::orders::{ cancel_orders, ModifyOrderId };
use crate::controller::position::OrderSide;
use crate::error::ErrorCode;
use crate::instructions::constraints::*;
use crate::instructions::optional_accounts::{
	get_referrer_and_referrer_stats,
	get_whitelist_token,
	load_maps,
	AccountMaps,
};
use crate::instructions::FulfillmentType;
use crate::math::casting::Cast;
use crate::math::safe_math::SafeMath;
use crate::math::balance::get_token_value;
use crate::math_error;
use crate::optional_accounts::{ get_token_interface, get_token_mint };
use crate::print_error;
use crate::safe_decrement;
use crate::safe_increment;
use crate::state::events::{ NewUserRecord, OrderActionExplanation };
use crate::state::fill_mode::FillMode;
use crate::state::oracle::StrictOraclePrice;
use crate::state::order_params::{
	ModifyOrderParams,
	OrderParams,
	PlaceAndTakeOrderSuccessCondition,
	PlaceOrderOptions,
	PostOnlyParam,
};
use crate::state::paused_operations::{ PerpOperation, SpotOperation };
use crate::state::market::MarketStatus;
use crate::state::market_map::{ get_writable_market_set, MarketSet };
use crate::state::market::SpotBalanceType;
use crate::state::market::Market;
use crate::state::market_map::{
	get_writable_market_set,
	get_writable_market_set_from_many,
};
use crate::state::state::State;
use crate::state::traits::Size;
use crate::state::user::{
	MarketType,
	OrderType,
	ReferrerName,
	User,
	UserStats,
};
use crate::state::user_map::{ load_user_maps, UserMap, UserStatsMap };
use crate::validate;
use crate::validation::user::validate_user_deletion;
use crate::validation::whitelist::validate_whitelist_token;
use crate::{ controller, math };
use crate::{ get_then_update_id, QUOTE_SPOT_MARKET_INDEX };
use crate::{ load, THIRTEEN_DAY };
use crate::{ load_mut, ExchangeStatus };
use anchor_lang::solana_program::sysvar::instructions;
use anchor_spl::associated_token::AssociatedToken;
use borsh::{ BorshDeserialize, BorshSerialize };

pub fn handle_initialize_user<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, InitializeUser<'info>>,
	sub_account_id: u16,
	name: [u8; 32]
) -> Result<()> {
	let user_key = ctx.accounts.user.key();
	let mut user = ctx.accounts.user
		.load_init()
		.or(Err(ErrorCode::UnableToLoadAccountLoader))?;
	user.authority = ctx.accounts.authority.key();
	user.sub_account_id = sub_account_id;
	user.name = name;
	user.next_order_id = 1;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();

	let mut user_stats = load_mut!(ctx.accounts.user_stats)?;
	user_stats.number_of_sub_accounts =
		user_stats.number_of_sub_accounts.safe_add(1)?;

	// Only try to add referrer if it is the first user
	if user_stats.number_of_sub_accounts_created == 0 {
		let (referrer, referrer_stats) = get_referrer_and_referrer_stats(
			remaining_accounts_iter
		)?;
		let referrer = if
			let (Some(referrer), Some(referrer_stats)) = (referrer, referrer_stats)
		{
			let referrer = load!(referrer)?;
			let mut referrer_stats = load_mut!(referrer_stats)?;

			validate!(referrer.sub_account_id == 0, ErrorCode::InvalidReferrer)?;

			validate!(
				referrer.authority == referrer_stats.authority,
				ErrorCode::ReferrerAndReferrerStatsAuthorityUnequal
			)?;

			referrer_stats.is_referrer = true;

			referrer.authority
		} else {
			Pubkey::default()
		};

		user_stats.referrer = referrer;
	}

	let whitelist_mint = &ctx.accounts.state.whitelist_mint;
	if !whitelist_mint.eq(&Pubkey::default()) {
		validate_whitelist_token(
			get_whitelist_token(remaining_accounts_iter)?,
			whitelist_mint,
			&ctx.accounts.authority.key()
		)?;
	}

	validate!(
		sub_account_id == user_stats.number_of_sub_accounts_created,
		ErrorCode::InvalidUserSubAccountId,
		"Invalid sub account id {}, must be {}",
		sub_account_id,
		user_stats.number_of_sub_accounts_created
	)?;

	user_stats.number_of_sub_accounts_created =
		user_stats.number_of_sub_accounts_created.safe_add(1)?;

	let state = &mut ctx.accounts.state;
	safe_increment!(state.number_of_sub_accounts, 1);

	let max_number_of_sub_accounts = state.max_number_of_sub_accounts();

	validate!(
		max_number_of_sub_accounts == 0 ||
			state.number_of_sub_accounts <= max_number_of_sub_accounts,
		ErrorCode::MaxNumberOfUsers
	)?;

	let now_ts = Clock::get()?.unix_timestamp;

	emit!(NewUserRecord {
		ts: now_ts,
		user_authority: ctx.accounts.authority.key(),
		user: user_key,
		sub_account_id,
		name,
		referrer: user_stats.referrer,
	});

	drop(user);

	let init_fee = state.get_init_user_fee()?;

	if init_fee > 0 {
		let payer_lamports = ctx.accounts.payer.to_account_info().try_lamports()?;
		if payer_lamports < init_fee {
			msg!("payer lamports {} init fee {}", payer_lamports, init_fee);
			return Err(ErrorCode::CantPayUserInitFee.into());
		}

		invoke(
			&transfer(&ctx.accounts.payer.key(), &ctx.accounts.user.key(), init_fee),
			&[
				ctx.accounts.payer.to_account_info(),
				ctx.accounts.user.to_account_info(),
				ctx.accounts.system_program.to_account_info(),
			]
		)?;
	}

	Ok(())
}

pub fn handle_initialize_user_stats<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, InitializeUserStats>
) -> Result<()> {
	let clock = Clock::get()?;

	let mut user_stats = ctx.accounts.user_stats
		.load_init()
		.or(Err(ErrorCode::UnableToLoadAccountLoader))?;

	*user_stats = UserStats {
		authority: ctx.accounts.authority.key(),
		number_of_sub_accounts: 0,
		last_taker_volume_30d_ts: clock.unix_timestamp,
		last_maker_volume_30d_ts: clock.unix_timestamp,
		last_filler_volume_30d_ts: clock.unix_timestamp,
		..UserStats::default()
	};

	let state = &mut ctx.accounts.state;
	safe_increment!(state.number_of_authorities, 1);

	let max_number_of_sub_accounts = state.max_number_of_sub_accounts();

	validate!(
		max_number_of_sub_accounts == 0 ||
			state.number_of_authorities <= max_number_of_sub_accounts,
		ErrorCode::MaxNumberOfUsers
	)?;

	Ok(())
}

pub fn handle_initialize_referrer_name(
	ctx: Context<InitializeReferrerName>,
	name: [u8; 32]
) -> Result<()> {
	let authority_key = ctx.accounts.authority.key();
	let user_stats_key = ctx.accounts.user_stats.key();
	let user_key = ctx.accounts.user.key();
	let mut referrer_name = ctx.accounts.referrer_name
		.load_init()
		.or(Err(ErrorCode::UnableToLoadAccountLoader))?;

	let user = load!(ctx.accounts.user)?;
	validate!(
		user.sub_account_id == 0,
		ErrorCode::InvalidReferrer,
		"must be subaccount 0"
	)?;

	referrer_name.authority = authority_key;
	referrer_name.user = user_key;
	referrer_name.user_stats = user_stats_key;
	referrer_name.name = name;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_cancel_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, CancelOrder>,
	order_id: Option<u32>
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	let order_id = match order_id {
		Some(order_id) => order_id,
		None => load!(ctx.accounts.user)?.get_last_order_id(),
	};

	controller::orders::cancel_order_by_order_id(
		order_id,
		&ctx.accounts.user,
		&market_map,
		&mut oracle_map,
		clock
	)?;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_cancel_order_by_user_id<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, CancelOrder>,
	user_order_id: u8
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	controller::orders::cancel_order_by_user_order_id(
		user_order_id,
		&ctx.accounts.user,
		&market_map,
		&mut oracle_map,
		clock
	)?;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_cancel_orders_by_ids<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, CancelOrder>,
	order_ids: Vec<u32>
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	for order_id in order_ids {
		controller::orders::cancel_order_by_order_id(
			order_id,
			&ctx.accounts.user,
			&market_map,
			&mut oracle_map,
			clock
		)?;
	}

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_cancel_orders<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, CancelOrder<'info>>,
	market_type: Option<MarketType>,
	market_index: Option<u16>,
	side: Option<OrderSide>
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	let user_key = ctx.accounts.user.key();
	let mut user = load_mut!(ctx.accounts.user)?;

	cancel_orders(
		&mut user,
		&user_key,
		None,
		&market_map,
		&mut oracle_map,
		clock.unix_timestamp,
		clock.slot,
		OrderActionExplanation::None,
		market_type,
		market_index,
		side
	)?;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_modify_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, CancelOrder<'info>>,
	order_id: Option<u32>,
	modify_order_params: ModifyOrderParams
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	let order_id = match order_id {
		Some(order_id) => order_id,
		None => load!(ctx.accounts.user)?.get_last_order_id(),
	};

	controller::orders::modify_order(
		ModifyOrderId::OrderId(order_id),
		modify_order_params,
		&ctx.accounts.user,
		state,
		&market_map,
		&mut oracle_map,
		clock
	)?;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_modify_order_by_user_order_id<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, CancelOrder<'info>>,
	user_order_id: u8,
	modify_order_params: ModifyOrderParams
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	controller::orders::modify_order(
		ModifyOrderId::UserOrderId(user_order_id),
		modify_order_params,
		&ctx.accounts.user,
		state,
		&market_map,
		&mut oracle_map,
		clock
	)?;

	Ok(())
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_place_orders<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, PlaceOrder>,
	params: Vec<OrderParams>
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	validate!(
		params.len() <= 32,
		ErrorCode::DefaultError,
		"max 32 order params"
	)?;

	let user_key = ctx.accounts.user.key();
	let mut user = load_mut!(ctx.accounts.user)?;

	let num_orders = params.len();
	for (i, params) in params.iter().enumerate() {
		validate!(
			!params.immediate_or_cancel,
			ErrorCode::InvalidOrderIOC,
			"immediate_or_cancel order must be in place_and_make or place_and_take"
		)?;

		// only enforce margin on last order and only try to expire on first order
		let options = PlaceOrderOptions {
			swift_taker_order_slot: None,
			try_expire_orders: i == 0,
			risk_increasing: false,
			explanation: OrderActionExplanation::None,
		};

		controller::orders::place_order(
			&ctx.accounts.state,
			&mut user,
			user_key,
			&market_map,
			&mut oracle_map,
			clock,
			*params,
			options
		)?;
	}

	Ok(())
}

#[access_control(fill_not_paused(&ctx.accounts.state))]
pub fn handle_place_and_take_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, PlaceAndTake<'info>>,
	params: OrderParams,
	success_condition: Option<u32> // u32 for backwards compatibility
) -> Result<()> {
	let clock = Clock::get()?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { market_map, mut oracle_map } = load_maps(
		remaining_accounts_iter,
		&get_writable_market_set(params.market_index),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	if params.post_only != PostOnlyParam::None {
		msg!("post_only cant be used in place_and_take");
		return Err(print_error!(ErrorCode::InvalidOrderPostOnly)().into());
	}

	let (makers_and_referrer, makers_and_referrer_stats) = load_user_maps(
		remaining_accounts_iter,
		true
	)?;

	let is_immediate_or_cancel = params.immediate_or_cancel;

	controller::repeg::update_amm(
		params.market_index,
		&market_map,
		&mut oracle_map,
		&ctx.accounts.state,
		&Clock::get()?
	)?;

	let user_key = ctx.accounts.user.key();
	let mut user = load_mut!(ctx.accounts.user)?;
	let clock = Clock::get()?;

	controller::orders::place_order(
		&ctx.accounts.state,
		&mut user,
		user_key,
		&market_map,
		&mut oracle_map,
		&clock,
		params,
		PlaceOrderOptions::default()
	)?;

	drop(user);

	let user = &mut ctx.accounts.user;
	let order_id = load!(user)?.get_last_order_id();

	let (base_asset_amount_filled, _) = controller::orders::fill_order(
		order_id,
		&ctx.accounts.state,
		user,
		&ctx.accounts.user_stats,
		&market_map,
		&mut oracle_map,
		&user.clone(),
		&ctx.accounts.user_stats.clone(),
		&makers_and_referrer,
		&makers_and_referrer_stats,
		None,
		&Clock::get()?,
		FillMode::PlaceAndTake
	)?;

	let order_exists = load!(ctx.accounts.user)?
		.orders.iter()
		.any(|order| order.order_id == order_id);

	if is_immediate_or_cancel && order_exists {
		controller::orders::cancel_order_by_order_id(
			order_id,
			&ctx.accounts.user,
			&market_map,
			&mut oracle_map,
			&Clock::get()?
		)?;
	}

	if let Some(success_condition) = success_condition {
		if
			success_condition ==
			(PlaceAndTakeOrderSuccessCondition::PartialFill as u32)
		{
			validate!(
				base_asset_amount_filled > 0,
				ErrorCode::PlaceAndTakeOrderSuccessConditionFailed,
				"no partial fill"
			)?;
		} else if
			success_condition == (PlaceAndTakeOrderSuccessCondition::FullFill as u32)
		{
			validate!(
				base_asset_amount_filled > 0 && !order_exists,
				ErrorCode::PlaceAndTakeOrderSuccessConditionFailed,
				"no full fill"
			)?;
		}
	}

	Ok(())
}

#[access_control(fill_not_paused(&ctx.accounts.state))]
pub fn handle_place_and_make_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, PlaceAndMake<'info>>,
	params: OrderParams,
	taker_order_id: u32
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { market_map, mut oracle_map } = load_maps(
		remaining_accounts_iter,
		&get_writable_market_set(params.market_index),
		Clock::get()?.slot,
		Some(state.oracle_guard_rails)
	)?;

	if
		!params.immediate_or_cancel ||
		params.post_only == PostOnlyParam::None ||
		params.order_type != OrderType::Limit
	{
		msg!("place_and_make must use IOC post only limit order");
		return Err(print_error!(ErrorCode::InvalidOrderIOCPostOnly)().into());
	}

	controller::repeg::update_amm(
		params.market_index,
		&market_map,
		&mut oracle_map,
		state,
		clock
	)?;

	let user_key = ctx.accounts.user.key();
	let mut user = load_mut!(ctx.accounts.user)?;

	controller::orders::place_order(
		state,
		&mut user,
		user_key,
		&market_map,
		&mut oracle_map,
		clock,
		params,
		PlaceOrderOptions::default()
	)?;

	let (order_id, authority) = (user.get_last_order_id(), user.authority);

	drop(user);

	let (mut makers_and_referrer, mut makers_and_referrer_stats) = load_user_maps(
		remaining_accounts_iter,
		true
	)?;
	makers_and_referrer.insert(
		ctx.accounts.user.key(),
		ctx.accounts.user.clone()
	)?;
	makers_and_referrer_stats.insert(authority, ctx.accounts.user_stats.clone())?;

	controller::orders::fill_order(
		taker_order_id,
		state,
		&ctx.accounts.taker,
		&ctx.accounts.taker_stats,
		&market_map,
		&mut oracle_map,
		&ctx.accounts.user.clone(),
		&ctx.accounts.user_stats.clone(),
		&makers_and_referrer,
		&makers_and_referrer_stats,
		Some(order_id),
		clock,
		FillMode::PlaceAndMake
	)?;

	let order_exists = load!(ctx.accounts.user)?
		.orders.iter()
		.any(|order| order.order_id == order_id);

	if order_exists {
		controller::orders::cancel_order_by_order_id(
			order_id,
			&ctx.accounts.user,
			&market_map,
			&mut oracle_map,
			clock
		)?;
	}

	Ok(())
}

pub fn handle_place_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, PlaceOrder>,
	params: OrderParams
) -> Result<()> {
	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		Clock::get()?.slot,
		None
	)?;

	if params.immediate_or_cancel {
		msg!(
			"immediate_or_cancel order must be in place_and_make or place_and_take"
		);
		return Err(print_error!(ErrorCode::InvalidOrderIOC)().into());
	}

	let user_key = ctx.accounts.user.key();
	let mut user = load_mut!(ctx.accounts.user)?;

	controller::orders::place_order(
		&ctx.accounts.state,
		&mut user,
		user_key,
		&market_map,
		&mut oracle_map,
		&Clock::get()?,
		params,
		PlaceOrderOptions::default()
	)?;

	Ok(())
}

#[access_control(fill_not_paused(&ctx.accounts.state))]
pub fn handle_place_and_take_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, PlaceAndTake<'info>>,
	params: OrderParams,
	_maker_order_id: Option<u32>
) -> Result<()> {
	let clock = Clock::get()?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { market_map, mut oracle_map } = load_maps(
		remaining_accounts_iter,
		&get_writable_market_set(params.market_index),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	if params.post_only != PostOnlyParam::None {
		msg!("post_only cant be used in place_and_take");
		return Err(print_error!(ErrorCode::InvalidOrderPostOnly)().into());
	}

	let (makers_and_referrer, makers_and_referrer_stats) = load_user_maps(
		remaining_accounts_iter,
		true
	)?;

	let is_immediate_or_cancel = params.immediate_or_cancel;

	controller::repeg::update_amm(
		params.market_index,
		&market_map,
		&mut oracle_map,
		&ctx.accounts.state,
		&Clock::get()?
	)?;

	let user_key = ctx.accounts.user.key();
	let mut user = load_mut!(ctx.accounts.user)?;
	let clock = Clock::get()?;

	controller::orders::place_order(
		&ctx.accounts.state,
		&mut user,
		user_key,
		&market_map,
		&mut oracle_map,
		&clock,
		params,
		PlaceOrderOptions::default()
	)?;

	drop(user);

	let user = &mut ctx.accounts.user;
	let order_id = load!(user)?.get_last_order_id();

	let (base_asset_amount_filled, _) = controller::orders::fill_order(
		order_id,
		&ctx.accounts.state,
		user,
		&ctx.accounts.user_stats,
		&market_map,
		&mut oracle_map,
		&user.clone(),
		&ctx.accounts.user_stats.clone(),
		&makers_and_referrer,
		&makers_and_referrer_stats,
		None,
		&Clock::get()?,
		FillMode::PlaceAndTake
	)?;

	let order_exists = load!(ctx.accounts.user)?
		.orders.iter()
		.any(|order| order.order_id == order_id);

	if is_immediate_or_cancel && order_exists {
		controller::orders::cancel_order_by_order_id(
			order_id,
			&ctx.accounts.user,
			&market_map,
			&mut oracle_map,
			&Clock::get()?
		)?;
	}

	if let Some(success_condition) = success_condition {
		if
			success_condition ==
			(PlaceAndTakeOrderSuccessCondition::PartialFill as u32)
		{
			validate!(
				base_asset_amount_filled > 0,
				ErrorCode::PlaceAndTakeOrderSuccessConditionFailed,
				"no partial fill"
			)?;
		} else if
			success_condition == (PlaceAndTakeOrderSuccessCondition::FullFill as u32)
		{
			validate!(
				base_asset_amount_filled > 0 && !order_exists,
				ErrorCode::PlaceAndTakeOrderSuccessConditionFailed,
				"no full fill"
			)?;
		}
	}

	Ok(())
}

#[access_control(fill_not_paused(&ctx.accounts.state))]
pub fn handle_place_and_make_order<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, PlaceAndMake<'info>>,
	params: OrderParams,
	taker_order_id: u32
) -> Result<()> {
	let clock = &Clock::get()?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let AccountMaps { market_map, mut oracle_map } = load_maps(
		remaining_accounts_iter,
		&get_writable_market_set(params.market_index),
		Clock::get()?.slot,
		Some(state.oracle_guard_rails)
	)?;

	if
		!params.immediate_or_cancel ||
		params.post_only == PostOnlyParam::None ||
		params.order_type != OrderType::Limit
	{
		msg!("place_and_make must use IOC post only limit order");
		return Err(print_error!(ErrorCode::InvalidOrderIOCPostOnly)().into());
	}

	controller::repeg::update_amm(
		params.market_index,
		&market_map,
		&mut oracle_map,
		state,
		clock
	)?;

	let user_key = ctx.accounts.user.key();
	let mut user = load_mut!(ctx.accounts.user)?;

	controller::orders::place_order(
		state,
		&mut user,
		user_key,
		&market_map,
		&mut oracle_map,
		clock,
		params,
		PlaceOrderOptions::default()
	)?;

	let (order_id, authority) = (user.get_last_order_id(), user.authority);

	drop(user);

	let (mut makers_and_referrer, mut makers_and_referrer_stats) = load_user_maps(
		remaining_accounts_iter,
		true
	)?;
	makers_and_referrer.insert(
		ctx.accounts.user.key(),
		ctx.accounts.user.clone()
	)?;
	makers_and_referrer_stats.insert(authority, ctx.accounts.user_stats.clone())?;

	controller::orders::fill_order(
		taker_order_id,
		state,
		&ctx.accounts.taker,
		&ctx.accounts.taker_stats,
		&market_map,
		&mut oracle_map,
		&ctx.accounts.user.clone(),
		&ctx.accounts.user_stats.clone(),
		&makers_and_referrer,
		&makers_and_referrer_stats,
		Some(order_id),
		clock,
		FillMode::PlaceAndMake
	)?;

	let order_exists = load!(ctx.accounts.user)?
		.orders.iter()
		.any(|order| order.order_id == order_id);

	if order_exists {
		controller::orders::cancel_order_by_order_id(
			order_id,
			&ctx.accounts.user,
			&market_map,
			&mut oracle_map,
			clock
		)?;
	}

	Ok(())
}

pub fn handle_update_user_name(
	ctx: Context<UpdateUser>,
	_sub_account_id: u16,
	name: [u8; 32]
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;
	user.name = name;
	Ok(())
}

pub fn handle_update_user_delegate(
	ctx: Context<UpdateUser>,
	_sub_account_id: u16,
	delegate: Pubkey
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;
	user.delegate = delegate;
	Ok(())
}

pub fn handle_update_user_reduce_only(
	ctx: Context<UpdateUser>,
	_sub_account_id: u16,
	reduce_only: bool
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;

	user.update_reduce_only_status(reduce_only)?;
	Ok(())
}

pub fn handle_delete_user(ctx: Context<DeleteUser>) -> Result<()> {
	let user = &load!(ctx.accounts.user)?;
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;

	validate_user_deletion(
		user,
		user_stats,
		&ctx.accounts.state,
		Clock::get()?.unix_timestamp
	)?;

	safe_decrement!(user_stats.number_of_sub_accounts, 1);

	let state = &mut ctx.accounts.state;
	safe_decrement!(state.number_of_sub_accounts, 1);

	Ok(())
}

pub fn handle_reclaim_rent(ctx: Context<ReclaimRent>) -> Result<()> {
	let user_size = ctx.accounts.user.to_account_info().data_len();
	let minimum_lamports = ctx.accounts.rent.minimum_balance(user_size);
	let current_lamports = ctx.accounts.user.to_account_info().try_lamports()?;
	let reclaim_amount = current_lamports.saturating_sub(minimum_lamports);

	validate!(
		reclaim_amount > 0,
		ErrorCode::CantReclaimRent,
		"user account has no excess lamports to reclaim"
	)?;

	**ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? =
		minimum_lamports;

	**ctx.accounts.authority.to_account_info().try_borrow_mut_lamports()? +=
		reclaim_amount;

	let user_stats = &mut load!(ctx.accounts.user_stats)?;

	// Skip age check if is no max sub accounts
	let max_sub_accounts = ctx.accounts.state.max_number_of_sub_accounts();
	let estimated_user_stats_age = user_stats.get_age_ts(
		Clock::get()?.unix_timestamp
	);
	validate!(
		max_sub_accounts == 0 || estimated_user_stats_age >= THIRTEEN_DAY,
		ErrorCode::CantReclaimRent,
		"user stats too young to reclaim rent. age ={} minimum = {}",
		estimated_user_stats_age,
		THIRTEEN_DAY
	)?;

	Ok(())
}

#[derive(Accounts)]
#[instruction(
    sub_account_id: u16,
)]
pub struct InitializeUser<'info> {
	#[account(
		init,
		seeds = [
			b"user",
			authority.key.as_ref(),
			sub_account_id.to_le_bytes().as_ref(),
		],
		space = User::SIZE,
		bump,
		payer = payer
	)]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        has_one = authority
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(mut)]
	pub payer: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitializeUserStats<'info> {
	#[account(
		init,
		seeds = [b"user_stats", authority.key.as_ref()],
		space = UserStats::SIZE,
		bump,
		payer = payer
	)]
	pub user_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(mut)]
	pub payer: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(
    name: [u8; 32],
)]
pub struct InitializeReferrerName<'info> {
	#[account(
		init,
		seeds = [b"referrer_name", name.as_ref()],
		space = ReferrerName::SIZE,
		bump,
		payer = payer
	)]
	pub referrer_name: AccountLoader<'info, ReferrerName>,
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	#[account(mut)]
	pub payer: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PlaceOrder<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct CancelOrder<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct PlaceAndTake<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct PlaceAndMake<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub taker: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&taker, &taker_stats)?
    )]
	pub taker_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(
    sub_account_id: u16,
)]
pub struct UpdateUser<'info> {
	#[account(
        mut,
        seeds = [b"user", authority.key.as_ref(), sub_account_id.to_le_bytes().as_ref()],
        bump,
    )]
	pub user: AccountLoader<'info, User>,
	pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct DeleteUser<'info> {
	#[account(
        mut,
        has_one = authority,
        close = authority
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        has_one = authority
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct ReclaimRent<'info> {
	#[account(
        mut,
        has_one = authority,
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        has_one = authority
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
}
