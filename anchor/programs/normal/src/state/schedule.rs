use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use crate::MarketType;

#[derive(
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Debug,
	Eq,
	Default
)]
pub enum OrderDirection {
	#[default]
	Buy,
	Sell,
}

impl OrderDirection {
	pub fn opposite(&self) -> Self {
		match self {
			OrderDirection::Buy => OrderDirection::Sell,
			OrderDirection::Sell => OrderDirection::Buy,
		}
	}
}

#[zero_copy(unsafe)]
#[derive(Default, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Schedule {
	// pub crank_delegate: Pubkey, // TODO: do we need this?
	pub market_type: MarketType,
	pub amm: Pubkey,
	pub base_asset_amount_per_interval: u64,
	pub direction: OrderDirection,
	pub active: bool,
	pub interval_seconds: u64,
	pub total_orders: u16,
	pub min_price: Option<u16>,
	pub max_price: Option<u16>,
	pub executed_orders: u16,
	pub total_executed: u64,
	pub last_updated_ts: u64,
	pub last_order_ts: u64,
}

impl Schedule {
	// pub fn opposite(&self) -> Self {
	// 	match self {
	// 		OrderDirection::Buy => OrderDirection::Sell,
	// 		OrderDirection::Sell => OrderDirection::Buy,
	// 	}
	// }
}

#[derive(
	AnchorSerialize,
	AnchorDeserialize,
	Clone,
	Default,
	Copy,
	Eq,
	PartialEq,
	Debug
)]
pub struct ScheduleParams {
	pub base_asset_amount_per_interval: u64,
	pub direction: OrderDirection,
	pub base_asset_amount: u64,
	pub active: bool,
	pub interval_seconds: u64,
	pub total_orders: u16,
	pub min_price: Option<u64>,
	pub max_price: Option<u64>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct ModifyScheduleParams {
	pub direction: Option<OrderDirection>,
	pub base_asset_amount: Option<u64>,
	pub active: Option<bool>,
	pub interval_seconds: Option<u64>,
	pub min_price: Option<u64>,
	pub max_price: Option<u64>,
}
