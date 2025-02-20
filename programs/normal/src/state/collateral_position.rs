use anchor_lang::prelude::*;

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct CollateralPosition {
	/// The market index of the corresponding market
	pub market_index: u16,
	/// Whether the user is active, being liquidated or bankrupt
    pub status: MarketPositionStatus,

    /// The balance of the position
    /// precision: SPOT_BALANCE_PRECISION
    pub collateral_balance: u128,
    /// The balance of minted synthetic tokens
    pub debt_balance: u128,
    /// The balance of collateral provided as liquidity
    pub collateral_lp_balance: u128,
    /// The balance of collateral lent to money markets
    pub collateral_loan_balance: u128,

    /// The number of lp (liquidity provider) shares the user has in this perp market
    /// LP shares allow users to provide liquidity via the AMM
    /// precision: BASE_PRECISION
    pub lp_shares: u64,

  
    /// The total values of mints the user has made
    /// precision: QUOTE_PRECISION
    pub total_mints: u128,
    /// The total values of burns the user has made
    /// precision: QUOTE_PRECISION
    pub total_burns: u128,

    /// The amount of margin freed during liquidation. Used to force the liquidation to occur over a period of time
    /// Defaults to zero when not being liquidated
    /// precision: QUOTE_PRECISION
    pub liquidation_margin_freed: u128,
    

 

    /// Whether the position is deposit or borrow
    pub balance_type: BalanceType,
}

impl SpotBalance for CollateralPosition {
	fn market_index(&self) -> u16 {
		self.market_index
	}

	fn balance(&self) -> u128 {
		self.scaled_balance as u128
	}

	fn increase_balance(&mut self, delta: u128) -> NormalResult {
		self.scaled_balance = self.scaled_balance.safe_add(delta.cast()?)?;
		Ok(())
	}

	fn decrease_balance(&mut self, delta: u128) -> NormalResult {
		self.scaled_balance = self.scaled_balance.safe_sub(delta.cast()?)?;
		Ok(())
	}
}

impl CollateralPosition {
	pub fn is_available(&self) -> bool {
		self.scaled_balance == 0
	}

	pub fn get_token_amount(
		&self,
		spot_market: &SpotMarket
	) -> NormalResult<u128> {
		get_token_amount(
			self.scaled_balance.cast()?,
			spot_market,
			&self.balance_type
		)
	}

	pub fn get_signed_token_amount(
		&self,
		spot_market: &SpotMarket
	) -> NormalResult<i128> {
		get_signed_token_amount(
			get_token_amount(
				self.scaled_balance.cast()?,
				spot_market,
				&self.balance_type
			)?,
			&self.balance_type
		)
	}

	pub fn is_borrow(&self) -> bool {
		self.scaled_balance > 0 && self.balance_type == SpotBalanceType::Borrow
	}
}
