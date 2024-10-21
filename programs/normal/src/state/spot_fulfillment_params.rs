use crate::error::NormalResult;
use crate::state::events::OrderActionExplanation;
use crate::state::market::{ SpotBalanceType, Market };
use crate::PositionDirection;
use std::cell::Ref;

pub trait FulfillmentParams {
    /// Returns the markets best bid and ask price, in PRICE_PRECISION
    fn get_best_bid_and_ask(&self) -> NormalResult<(Option<u64>, Option<u64>)>;

    /// Fulfills the taker order
    ///
    /// # Arguments
    ///
    /// *`taker_direction` - The direction of the taker order
    /// *`taker_price` - The price of the taker order, in PRICE_PRECISION
    /// *`taker_base_asset_amount` - The base amount for taker order, precision is 10^base_mint_decimals
    /// *`taker_max_quote_asset_amount` - The max quote amount for taker order, precision is QUOTE_PRECISION (1e6)
    /// *`now` - The current unix timestamp
    fn fulfill_order(
        &mut self,
        taker_direction: PositionDirection,
        taker_price: u64,
        taker_base_asset_amount: u64,
        taker_max_quote_asset_amount: u64
    ) -> NormalResult<ExternalSpotFill>;

    /// Gets the order action explanation to be logged in the OrderActionRecord
    fn get_order_action_explanation(&self) -> NormalResult<OrderActionExplanation>;

    /// Called at the end of instructions calling fill_spot_order, validates that the token amount in each market's vault
    /// equals the markets deposits - borrows
    fn validate_vault_amounts(
        &self,
        base_market: &Ref<Market>,
        quote_market: &Ref<Market>
    ) -> NormalResult<()>;

    fn validate_markets(&self, base_market: &Market, quote_market: &Market) -> NormalResult<()>;
}

#[cfg(test)]
use crate::error::ErrorCode;
#[cfg(test)]
pub struct TestFulfillmentParams {}

#[cfg(test)]
impl SpotFulfillmentParams for TestFulfillmentParams {
    fn get_best_bid_and_ask(&self) -> NormalResult<(Option<u64>, Option<u64>)> {
        Err(ErrorCode::InvalidSpotFulfillmentParams)
    }

    fn fulfill_order(
        &mut self,
        _taker_direction: PositionDirection,
        _taker_price: u64,
        _taker_base_asset_amount: u64,
        _taker_max_quote_asset_amount: u64
    ) -> NormalResult<ExternalSpotFill> {
        Err(ErrorCode::InvalidSpotFulfillmentParams)
    }

    fn get_order_action_explanation(&self) -> NormalResult<OrderActionExplanation> {
        Err(ErrorCode::InvalidSpotFulfillmentParams)
    }

    fn validate_vault_amounts(
        &self,
        _base_market: &Ref<Market>,
        _quote_market: &Ref<Market>
    ) -> NormalResult<()> {
        Err(ErrorCode::InvalidSpotFulfillmentParams)
    }

    fn validate_markets(&self, base_market: &Market, quote_market: &Market) -> NormalResult<()> {
        Ok(())
    }
}
