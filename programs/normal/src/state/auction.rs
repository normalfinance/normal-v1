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
pub enum AuctionType {
	#[default]
	/// selling collateral from a Vault liquidation
	Collateral,
	/// selling newly minted NORM to cover Protocol Debt (the deficit from Collateral Auctions)
	Debt,
	/// selling excess synthetic token proceeds over the Insurance Fund max limit for NORM to be burned
	Surplus,
}

#[derive(Copy, AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct AuctionConfig {
	/// where collateral auctions should take place (3rd party AMM vs private)
	pub auction_location: AuctionPreference,
	/// Maximum time allowed for the auction to complete.
	pub auction_duration: u16,
	/// Determines how quickly the starting price decreases during the auction if there are no bids.
	pub auction_bid_decrease_rate: u16,
	/// May be capped to prevent overly large auctions that could affect the market price.
	pub max_auction_lot_size: u64,
}

#[derive(
	Clone,
	Copy,
	AnchorSerialize,
	AnchorDeserialize,
	PartialEq,
	Debug,
	Eq,
	Default
)]
pub enum AuctionPreference {
	#[default]
	/// a local secondary market
	Private,
	/// a DEX like Orca, Serum, Jupiter, etc.
	External,
}