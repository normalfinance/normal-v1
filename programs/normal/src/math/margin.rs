use crate::error::NormalResult;
use crate::error::ErrorCode;
use crate::math::constants::{
	MARGIN_PRECISION_U128,
	MAX_POSITIVE_UPNL_FOR_INITIAL_MARGIN,
	PRICE_PRECISION,
	SPOT_IMF_PRECISION_U128,
	SPOT_WEIGHT_PRECISION,
	SPOT_WEIGHT_PRECISION_U128,
};

use crate::{ validate, PRICE_PRECISION_I128 };
use crate::{ validation, PRICE_PRECISION_I64 };

use crate::math::casting::Cast;
use crate::math::oracle::{ is_oracle_valid_for_action, NormalAction };

use crate::math::synth_balance::{ get_strict_token_value, get_token_value };

use crate::math::safe_math::SafeMath;
use crate::state::margin_calculation::{
	MarginCalculation,
	MarginContext,
	MarketIdentifier,
};
use crate::state::oracle::{ OraclePriceData, StrictOraclePrice };
use crate::state::oracle_map::OracleMap;
use crate::state::market::{ Tier, MarketStatus, Market };
use crate::state::market_map::MarketMap;
use crate::state::user::{  User };
use num_integer::Roots;
use solana_program::msg;
use std::cmp::{ max, min, Ordering };

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum MarginRequirementType {
	Initial,
	// Fill,
	Maintenance,
}

pub fn calculate_size_premium_liability_weight(
	size: u128, // AMM_RESERVE_PRECISION
	imf_factor: u32,
	liability_weight: u32,
	precision: u128
) -> NormalResult<u32> {
	if imf_factor == 0 {
		return Ok(liability_weight);
	}

	let size_sqrt = (size * 10 + 1).nth_root(2); //1e9 -> 1e10 -> 1e5

	let imf_factor_u128 = imf_factor.cast::<u128>()?;
	let liability_weight_u128 = liability_weight.cast::<u128>()?;
	let liability_weight_numerator = liability_weight_u128.safe_sub(
		liability_weight_u128.safe_div(5)?
	)?;

	// increases
	let size_premium_liability_weight = liability_weight_numerator
		.safe_add(
			size_sqrt // 1e5
				.safe_mul(imf_factor_u128)?
				.safe_div((100_000 * SPOT_IMF_PRECISION_U128) / precision)? // 1e5 * 1e2
		)?
		.cast::<u32>()?;

	let max_liability_weight = max(
		liability_weight,
		size_premium_liability_weight
	);
	Ok(max_liability_weight)
}

pub fn calculate_margin_requirement_and_total_collateral_and_liability_info(
	user: &User,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	context: MarginContext
) -> NormalResult<MarginCalculation> {
	let mut calculation = MarginCalculation::new(context);

	let user_custom_margin_ratio = if
		context.margin_type == MarginRequirementType::Initial
	{
		user.max_margin_ratio
	} else {
		0_u32
	};

	for vault_position in user.vault_positions.iter() {
		if vault_position.is_available() {
			continue;
		}

		let market = market_map.get_ref(&vault_position.market_index)?;

		let (oracle_price_data, oracle_validity) =
			oracle_map.get_price_data_and_validity(
				market.market_index,
				&market.oracle,
				market.historical_oracle_data.last_oracle_price_twap,
				market.get_max_confidence_interval_multiplier()?
			)?;

		calculation.update_all_oracles_valid(
			is_oracle_valid_for_action(
				oracle_validity,
				Some(NormalAction::MarginCalc)
			)?
		);

		// TODO: ...
	}

	calculation.validate_num_spot_liabilities()?;

	Ok(calculation)
}

pub fn meets_initial_margin_requirement(
	user: &User,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap
) -> NormalResult<bool> {
	calculate_margin_requirement_and_total_collateral_and_liability_info(
		user,
		market_map,
		oracle_map,
		MarginContext::standard(MarginRequirementType::Initial)
	).map(|calc| calc.meets_margin_requirement())
}

pub fn meets_maintenance_margin_requirement(
	user: &User,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap
) -> NormalResult<bool> {
	calculate_margin_requirement_and_total_collateral_and_liability_info(
		user,
		market_map,
		oracle_map,
		MarginContext::standard(MarginRequirementType::Maintenance)
	).map(|calc| calc.meets_margin_requirement())
}

pub fn calculate_max_withdrawable_amount(
	market_index: u16,
	user: &User,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap
) -> NormalResult<u64> {
	let calculation =
		calculate_margin_requirement_and_total_collateral_and_liability_info(
			user,
			market_map,
			oracle_map,
			MarginContext::standard(MarginRequirementType::Initial)
		)?;

	let market = &mut market_map.get_ref(&market_index)?;

	let token_amount = user
		.get_spot_position(market_index)?
		.get_token_amount(market)?;

	let oracle_price = oracle_map.get_price_data(&market.oracle)?.price;

	let asset_weight = market.get_asset_weight(
		token_amount,
		oracle_price,
		&MarginRequirementType::Initial
	)?;

	if asset_weight == 0 {
		return Ok(u64::MAX);
	}

	if calculation.get_num_of_liabilities()? == 0 {
		// user has small dust deposit and no liabilities
		// so return early with user tokens amount
		return token_amount.cast();
	}

	let free_collateral = calculation.get_free_collateral()?;

	let precision_increase = (10u128).pow(spot_market.decimals - 6);

	free_collateral
		.safe_mul(MARGIN_PRECISION_U128)?
		.safe_div(asset_weight.cast()?)?
		.safe_mul(PRICE_PRECISION)?
		.safe_div(oracle_price.cast()?)?
		.safe_mul(precision_increase)?
		.cast()
}

pub fn calculate_user_equity(
	user: &User,
	market_map: &MarketMap,
	vault_map: &VaultMap,
	oracle_map: &mut OracleMap
) -> NormalResult<(i128, bool)> {
	let mut net_usd_value: i128 = 0;
	let mut all_oracles_valid = true;

	for spot_position in user.spot_positions.iter() {
		if spot_position.is_available() {
			continue;
		}

		let market = &market_map.get_ref(&market_position.market_index)?;

		// Calculate collateral value
		let (oracle_price_data, oracle_validity) =
			oracle_map.get_price_data_and_validity(
				spot_market.market_index,
				&spot_market.oracle,
				spot_market.historical_oracle_data.last_oracle_price_twap,
				spot_market.get_max_confidence_interval_multiplier()?
			)?;

		all_oracles_valid &= is_oracle_valid_for_action(
			oracle_validity,
			Some(NormalAction::MarginCalc)
		)?;

		let token_amount = spot_position.get_signed_token_amount(&spot_market)?;
		let oracle_price = oracle_price_data.price;
		let token_value = get_token_value(
			token_amount,
			spot_market.decimals,
			oracle_price
		)?;

		net_usd_value = net_usd_value.safe_add(token_value)?;

		// Calculate synthetic value
		let valuation_price = if market.status == MarketStatus::Settlement {
			market.expiry_price
		} else {
			oracle_price_data.price
		};

		let market_position = market_position.simulate_settled_lp_position(
			market,
			valuation_price
		)?;

		let (_, unrealized_pnl) =
			calculate_base_asset_value_and_pnl_with_oracle_price(
				&market_position,
				valuation_price
			)?;

		let pnl = unrealized_pnl?;

		let pnl_value = pnl
			.safe_mul(quote_oracle_price.cast()?)?
			.safe_div(PRICE_PRECISION_I128)?;

		net_usd_value = net_usd_value.safe_add(pnl_value)?;
	}

	Ok((net_usd_value, all_oracles_valid))
}
