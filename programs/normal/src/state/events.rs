use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use crate::controller::position::OrderSide;
use crate::error::{ NormalResult, ErrorCode::InvalidOrder };
use crate::math::casting::Cast;
use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::traits::Size;
use crate::state::user::{ MarketType, Order };
use anchor_lang::Discriminator;
use std::io::Write;

#[event]
pub struct NewUserRecord {
	/// unix_timestamp of action
	pub ts: i64,
	pub user_authority: Pubkey,
	pub user: Pubkey,
	pub sub_account_id: u16,
	pub name: [u8; 32],
	pub referrer: Pubkey,
}

#[event]
pub struct CurveRecord {
	pub ts: i64,
	pub record_id: u64,
	pub peg_multiplier_before: u128,
	pub base_asset_reserve_before: u128,
	pub quote_asset_reserve_before: u128,
	pub sqrt_k_before: u128,
	pub peg_multiplier_after: u128,
	pub base_asset_reserve_after: u128,
	pub quote_asset_reserve_after: u128,
	pub sqrt_k_after: u128,
	/// precision: BASE_PRECISION
	pub base_asset_amount_long: u128,
	/// precision: BASE_PRECISION
	pub base_asset_amount_with_amm: i128,
	/// precision: QUOTE_PRECISION
	pub total_fee: i128,
	/// precision: QUOTE_PRECISION
	pub total_fee_minus_distributions: i128,
	/// precision: QUOTE_PRECISION
	pub adjustment_cost: i128,
	/// precision: PRICE_PRECISION
	pub oracle_price: i64,
	pub fill_record: u128,
	pub number_of_users: u32,
	pub market_index: u16,
}

#[event]
pub struct OrderRecord {
	pub ts: i64,
	pub user: Pubkey,
	pub order: Order,
}

impl Size for OrderRecord {
	const SIZE: usize = 192;
}

#[event]
pub struct OrderActionRecord {
	pub ts: i64,
	pub action: OrderAction,
	pub action_explanation: OrderActionExplanation,
	pub market_index: u16,
	pub market_type: MarketType,

	pub filler: Option<Pubkey>,
	/// precision: QUOTE_PRECISION
	pub filler_reward: Option<u64>,
	pub fill_record_id: Option<u64>,

	/// precision: BASE_PRECISION (perp) or MINT_PRECISION (spot)
	pub base_asset_amount_filled: Option<u64>,
	/// precision: QUOTE_PRECISION
	pub quote_asset_amount_filled: Option<u64>,
	/// precision: QUOTE_PRECISION
	pub taker_fee: Option<u64>,
	/// precision: QUOTE_PRECISION
	pub maker_fee: Option<i64>,
	/// precision: QUOTE_PRECISION
	pub referrer_reward: Option<u32>,
	/// precision: QUOTE_PRECISION
	pub quote_asset_amount_surplus: Option<i64>,
	/// precision: QUOTE_PRECISION
	pub spot_fulfillment_method_fee: Option<u64>,

	pub taker: Option<Pubkey>,
	pub taker_order_id: Option<u32>,
	pub taker_order_side: Option<OrderSide>,
	/// precision: BASE_PRECISION (perp) or MINT_PRECISION (spot)
	pub taker_order_base_asset_amount: Option<u64>,
	/// precision: BASE_PRECISION (perp) or MINT_PRECISION (spot)
	pub taker_order_cumulative_base_asset_amount_filled: Option<u64>,
	/// precision: QUOTE_PRECISION
	pub taker_order_cumulative_quote_asset_amount_filled: Option<u64>,

	pub maker: Option<Pubkey>,
	pub maker_order_id: Option<u32>,
	pub maker_order_side: Option<OrderSide>,
	/// precision: BASE_PRECISION (perp) or MINT_PRECISION (spot)
	pub maker_order_base_asset_amount: Option<u64>,
	/// precision: BASE_PRECISION (perp) or MINT_PRECISION (spot)
	pub maker_order_cumulative_base_asset_amount_filled: Option<u64>,
	/// precision: QUOTE_PRECISION
	pub maker_order_cumulative_quote_asset_amount_filled: Option<u64>,

	/// precision: PRICE_PRECISION
	pub oracle_price: i64,
}

impl Size for OrderActionRecord {
	const SIZE: usize = 384;
}

pub fn get_order_action_record(
	ts: i64,
	action: OrderAction,
	action_explanation: OrderActionExplanation,
	market_index: u16,
	filler: Option<Pubkey>,
	fill_record_id: Option<u64>,
	filler_reward: Option<u64>,
	base_asset_amount_filled: Option<u64>,
	quote_asset_amount_filled: Option<u64>,
	taker_fee: Option<u64>,
	maker_rebate: Option<u64>,
	referrer_reward: Option<u64>,
	quote_asset_amount_surplus: Option<i64>,
	spot_fulfillment_method_fee: Option<u64>,
	taker: Option<Pubkey>,
	taker_order: Option<Order>,
	maker: Option<Pubkey>,
	maker_order: Option<Order>,
	oracle_price: i64
) -> NormalResult<OrderActionRecord> {
	Ok(OrderActionRecord {
		ts,
		action,
		action_explanation,
		market_index,
		market_type: if let Some(taker_order) = taker_order {
			taker_order.market_type
		} else if let Some(maker_order) = maker_order {
			maker_order.market_type
		} else {
			return Err(InvalidOrder);
		},
		filler,
		filler_reward,
		fill_record_id,
		base_asset_amount_filled,
		quote_asset_amount_filled,
		taker_fee,
		maker_fee: match maker_rebate {
			Some(maker_rebate) => Some(-maker_rebate.cast()?),
			None => None,
		},
		referrer_reward: match referrer_reward {
			Some(referrer_reward) if referrer_reward > 0 =>
				Some(referrer_reward.cast()?),
			_ => None,
		},
		quote_asset_amount_surplus,
		spot_fulfillment_method_fee,
		taker,
		taker_order_id: taker_order.map(|order| order.order_id),
		taker_order_side: taker_order.map(|order| order.side),
		taker_order_base_asset_amount: taker_order.map(
			|order| order.base_asset_amount
		),
		taker_order_cumulative_base_asset_amount_filled: taker_order.map(
			|order| order.base_asset_amount_filled
		),
		taker_order_cumulative_quote_asset_amount_filled: taker_order
			.as_ref()
			.map(|order| order.quote_asset_amount_filled),
		maker,
		maker_order_id: maker_order.map(|order| order.order_id),
		maker_order_side: maker_order.map(|order| order.side),
		maker_order_base_asset_amount: maker_order.map(
			|order| order.base_asset_amount
		),
		maker_order_cumulative_base_asset_amount_filled: maker_order.map(
			|order| order.base_asset_amount_filled
		),
		maker_order_cumulative_quote_asset_amount_filled: maker_order.map(
			|order| order.quote_asset_amount_filled
		),
		oracle_price,
	})
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Eq, Default)]
pub enum OrderAction {
	#[default]
	Place,
	Cancel,
	Fill,
	Trigger,
	Expire,
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub enum OrderActionExplanation {
	None,
	InsufficientFunds,
	OraclePriceBreachedLimitPrice,
	MarketOrderFilledToLimitPrice,
	OrderExpired,
	OrderFilledWithAMM,
	OrderFilledWithAMMJit,
	OrderFilledWithMatch,
	OrderFilledWithMatchJit,
	MarketExpired,
	RiskingIncreasingOrder,
	ReduceOnlyOrderIncreasedPosition,
	OrderFilledWithAMMJitLPSplit,
	OrderFilledWithLPJit,
	DeriskLp,
}

#[event]
#[derive(Default)]
pub struct LPRecord {
	pub ts: i64,
	pub user: Pubkey,
	pub action: LPAction,
	/// precision: AMM_RESERVE_PRECISION
	pub n_shares: u64,
	pub market_index: u16,
	/// precision: BASE_PRECISION
	pub delta_base_asset_amount: i64,
	/// precision: QUOTE_PRECISION
	pub delta_quote_asset_amount: i64,
	/// realized pnl of the position settlement
	/// precision: QUOTE_PRECISION
	pub pnl: i64,
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Eq, Default)]
pub enum LPAction {
	#[default]
	AddLiquidity,
	RemoveLiquidity,
	SettleLiquidity,
	RemoveLiquidityDerisk,
}

impl Size for LPRecord {
	const SIZE: usize = 112;
}

#[event]
#[derive(Default)]
pub struct InsuranceFundRecord {
	pub ts: i64,
	pub market_index: u16,
	/// precision: PERCENTAGE_PRECISION
	pub user_if_factor: u32,
	/// precision: PERCENTAGE_PRECISION
	pub total_if_factor: u32,
	/// precision: token mint precision
	pub vault_amount_before: u64,
	/// precision: token mint precision
	pub insurance_vault_amount_before: u64,
	pub total_if_shares_before: u128,
	pub total_if_shares_after: u128,
	/// precision: token mint precision
	pub amount: i64,
}

#[event]
#[derive(Default)]
pub struct InsuranceFundStakeRecord {
	pub ts: i64,
	pub user_authority: Pubkey,
	pub action: StakeAction,
	/// precision: token mint precision
	pub amount: u64,

	/// precision: token mint precision
	pub insurance_vault_amount_before: u64,
	pub if_shares_before: u128,
	pub user_if_shares_before: u128,
	pub total_if_shares_before: u128,
	pub if_shares_after: u128,
	pub user_if_shares_after: u128,
	pub total_if_shares_after: u128,
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Eq, Default)]
pub enum StakeAction {
	#[default]
	Stake,
	UnstakeRequest,
	UnstakeCancelRequest,
	Unstake,
	UnstakeTransfer,
	StakeTransfer,
}

pub fn emit_stack<T: AnchorSerialize + Discriminator, const N: usize>(
	event: T
) -> NormalResult {
	let mut data_buf = [0u8; N];
	let mut out_buf = [0u8; N];

	emit_buffers(event, &mut data_buf[..], &mut out_buf[..])
}

pub fn emit_buffers<T: AnchorSerialize + Discriminator>(
	event: T,
	data_buf: &mut [u8],
	out_buf: &mut [u8]
) -> NormalResult {
	let mut data_writer = std::io::Cursor::new(data_buf);
	data_writer.write_all(&<T as Discriminator>::discriminator()).safe_unwrap()?;
	borsh::to_writer(&mut data_writer, &event).safe_unwrap()?;
	let data_len = data_writer.position() as usize;

	let out_len = base64::encode_config_slice(
		&data_writer.into_inner()[0..data_len],
		base64::STANDARD,
		out_buf
	);

	let msg_bytes = &out_buf[0..out_len];
	let msg_str = unsafe { std::str::from_utf8_unchecked(msg_bytes) };

	msg!(msg_str);

	Ok(())
}
