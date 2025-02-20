use std::num::TryFromIntError;

use anchor_lang::prelude::*;

pub type NormalResult<T = ()> = std::result::Result<T, ErrorCode>;

#[error_code]
#[derive(PartialEq, Eq)]
pub enum ErrorCode {
	#[msg("Invalid Spot Market Authority")]
	InvalidSpotMarketAuthority,
	#[msg("Clearing house not insurance fund authority")]
	InvalidInsuranceFundAuthority,
	#[msg("Insufficient deposit")]
	InsufficientDeposit,
	#[msg("Insufficient collateral")]
	InsufficientCollateral,
	#[msg("Sufficient collateral")]
	SufficientCollateral,
	#[msg("Max number of positions taken")]
	MaxNumberOfPositions,
	#[msg("Admin Controls Prices Disabled")]
	AdminControlsPricesDisabled,
	#[msg("Market Delisted")]
	MarketDelisted,
	#[msg("Market Index Already Initialized")]
	MarketIndexAlreadyInitialized,
	#[msg("User Account And User Positions Account Mismatch")]
	UserAccountAndUserPositionsAccountMismatch,
	#[msg("User Has No Position In Market")]
	UserHasNoPositionInMarket,
	#[msg("Invalid Initial Peg")]
	InvalidInitialPeg,
	#[msg("AMM repeg already configured with amt given")]
	InvalidRepegRedundant,
	#[msg("AMM repeg incorrect repeg direction")]
	InvalidRepegDirection,
	#[msg("AMM repeg out of bounds pnl")]
	InvalidRepegProfitability,
	#[msg("Slippage Outside Limit Price")]
	SlippageOutsideLimit,
	#[msg("Order Size Too Small")]
	OrderSizeTooSmall,
	#[msg("Price change too large when updating K")]
	InvalidUpdateK,
	#[msg("Admin tried to withdraw amount larger than fees collected")]
	AdminWithdrawTooLarge,
	#[msg("Math Error")]
	MathError,
	#[msg("Conversion to u128/u64 failed with an overflow or underflow")]
	BnConversionError,
	#[msg("Clock unavailable")]
	ClockUnavailable,
	#[msg("Unable To Load Oracles")]
	UnableToLoadOracle,
	#[msg("Price Bands Breached")]
	PriceBandsBreached,
	#[msg("Exchange is paused")]
	ExchangePaused,
	#[msg("Invalid whitelist token")]
	InvalidWhitelistToken,
	#[msg("Whitelist token not found")]
	WhitelistTokenNotFound,
	#[msg("Invalid discount token")]
	InvalidDiscountToken,
	#[msg("Discount token not found")]
	DiscountTokenNotFound,
	#[msg("Referrer not found")]
	ReferrerNotFound,
	#[msg("ReferrerNotFound")]
	ReferrerStatsNotFound,
	#[msg("ReferrerMustBeWritable")]
	ReferrerMustBeWritable,
	#[msg("ReferrerMustBeWritable")]
	ReferrerStatsMustBeWritable,
	#[msg("ReferrerAndReferrerStatsAuthorityUnequal")]
	ReferrerAndReferrerStatsAuthorityUnequal,
	#[msg("InvalidReferrer")]
	InvalidReferrer,
	#[msg("InvalidOracle")]
	InvalidOracle,
	#[msg("OracleNotFound")]
	OracleNotFound,
	#[msg("Liquidations Blocked By Oracle")]
	LiquidationsBlockedByOracle,
	#[msg("Can not deposit more than max deposit")]
	MaxDeposit,
	#[msg("Can not delete user that still has collateral")]
	CantDeleteUserWithCollateral,
	#[msg("AMM funding out of bounds pnl")]
	InvalidFundingProfitability,
	#[msg("Casting Failure")]
	CastingFailure,
	#[msg("InvalidOrder")]
	InvalidOrder,
	#[msg("InvalidOrderMaxTs")]
	InvalidOrderMaxTs,
	#[msg("InvalidOrderMarketType")]
	InvalidOrderMarketType,
	#[msg("InvalidOrderForInitialMarginReq")]
	InvalidOrderForInitialMarginReq,
	#[msg("InvalidOrderNotRiskReducing")]
	InvalidOrderNotRiskReducing,
	#[msg("InvalidOrderSizeTooSmall")]
	InvalidOrderSizeTooSmall,
	#[msg("InvalidOrderNotStepSizeMultiple")]
	InvalidOrderNotStepSizeMultiple,
	#[msg("InvalidOrderBaseQuoteAsset")]
	InvalidOrderBaseQuoteAsset,
	#[msg("InvalidOrderIOC")]
	InvalidOrderIOC,
	#[msg("InvalidOrderPostOnly")]
	InvalidOrderPostOnly,
	#[msg("InvalidOrderIOCPostOnly")]
	InvalidOrderIOCPostOnly,
	#[msg("InvalidOrderTrigger")]
	InvalidOrderTrigger,
	#[msg("InvalidOrderAuction")]
	InvalidOrderAuction,
	#[msg("InvalidOrderOracleOffset")]
	InvalidOrderOracleOffset,
	#[msg("InvalidOrderMinOrderSize")]
	InvalidOrderMinOrderSize,
	#[msg("Failed to Place Post-Only Limit Order")]
	PlacePostOnlyLimitFailure,
	#[msg("User has no order")]
	UserHasNoOrder,
	#[msg("Order Amount Too Small")]
	OrderAmountTooSmall,
	#[msg("Max number of orders taken")]
	MaxNumberOfOrders,
	#[msg("Order does not exist")]
	OrderDoesNotExist,
	#[msg("Order not open")]
	OrderNotOpen,
	#[msg("FillOrderDidNotUpdateState")]
	FillOrderDidNotUpdateState,
	#[msg("Reduce only order increased risk")]
	ReduceOnlyOrderIncreasedRisk,
	#[msg("Unable to load AccountLoader")]
	UnableToLoadAccountLoader,
	#[msg("Trade Size Too Large")]
	TradeSizeTooLarge,
	#[msg("User cant refer themselves")]
	UserCantReferThemselves,
	#[msg("Did not receive expected referrer")]
	DidNotReceiveExpectedReferrer,
	#[msg("Could not deserialize referrer")]
	CouldNotDeserializeReferrer,
	#[msg("Could not deserialize referrer stats")]
	CouldNotDeserializeReferrerStats,
	#[msg("User Order Id Already In Use")]
	UserOrderIdAlreadyInUse,
	#[msg("No positions liquidatable")]
	NoPositionsLiquidatable,
	#[msg("Invalid Margin Ratio")]
	InvalidMarginRatio,
	#[msg("Cant Cancel Post Only Order")]
	CantCancelPostOnlyOrder,
	#[msg("InvalidOracleOffset")]
	InvalidOracleOffset,
	#[msg("CantExpireOrders")]
	CantExpireOrders,
	#[msg("CouldNotLoadMarketData")]
	CouldNotLoadMarketData,
	#[msg("PerpMarketNotFound")]
	PerpMarketNotFound,
	#[msg("InvalidMarketAccount")]
	InvalidMarketAccount,
	#[msg("UnableToLoadMarketAccount")]
	UnableToLoadPerpMarketAccount,
	#[msg("MarketWrongMutability")]
	MarketWrongMutability,
	#[msg("UnableToCastUnixTime")]
	UnableToCastUnixTime,
	#[msg("CouldNotFindSpotPosition")]
	CouldNotFindSpotPosition,
	#[msg("NoSpotPositionAvailable")]
	NoSpotPositionAvailable,
	#[msg("InvalidSpotMarketInitialization")]
	InvalidSpotMarketInitialization,
	#[msg("CouldNotLoadSpotMarketData")]
	CouldNotLoadSpotMarketData,
	#[msg("SpotMarketNotFound")]
	SpotMarketNotFound,
	#[msg("InvalidSpotMarketAccount")]
	InvalidSpotMarketAccount,
	#[msg("UnableToLoadSpotMarketAccount")]
	UnableToLoadSpotMarketAccount,
	#[msg("SpotMarketWrongMutability")]
	SpotMarketWrongMutability,
	#[msg("SpotInterestNotUpToDate")]
	SpotMarketInterestNotUpToDate,
	#[msg("SpotMarketInsufficientDeposits")]
	SpotMarketInsufficientDeposits,
	#[msg("UserMustSettleTheirOwnPositiveUnsettledPNL")]
	UserMustSettleTheirOwnPositiveUnsettledPNL,
	#[msg("CantUpdatePoolBalanceType")]
	CantUpdatePoolBalanceType,
	#[msg("InsufficientCollateralForSettlingPNL")]
	InsufficientCollateralForSettlingPNL,
	#[msg("AMMNotUpdatedInSameSlot")]
	AMMNotUpdatedInSameSlot,
	#[msg("AuctionNotComplete")]
	AuctionNotComplete,
	#[msg("MakerNotFound")]
	MakerNotFound,
	#[msg("MakerNotFound")]
	MakerStatsNotFound,
	#[msg("MakerMustBeWritable")]
	MakerMustBeWritable,
	#[msg("MakerMustBeWritable")]
	MakerStatsMustBeWritable,
	#[msg("MakerOrderNotFound")]
	MakerOrderNotFound,
	#[msg("CouldNotDeserializeMaker")]
	CouldNotDeserializeMaker,
	#[msg("CouldNotDeserializeMaker")]
	CouldNotDeserializeMakerStats,
	#[msg("AuctionPriceDoesNotSatisfyMaker")]
	AuctionPriceDoesNotSatisfyMaker,
	#[msg("MakerCantFulfillOwnOrder")]
	MakerCantFulfillOwnOrder,
	#[msg("MakerOrderMustBePostOnly")]
	MakerOrderMustBePostOnly,
	#[msg("CantMatchTwoPostOnlys")]
	CantMatchTwoPostOnlys,
	#[msg("OrderBreachesOraclePriceLimits")]
	OrderBreachesOraclePriceLimits,
	#[msg("OrderMustBeTriggeredFirst")]
	OrderMustBeTriggeredFirst,
	#[msg("OrderNotTriggerable")]
	OrderNotTriggerable,
	#[msg("OrderDidNotSatisfyTriggerCondition")]
	OrderDidNotSatisfyTriggerCondition,
	#[msg("PositionAlreadyBeingLiquidated")]
	PositionAlreadyBeingLiquidated,
	#[msg("PositionDoesntHaveOpenPositionOrOrders")]
	PositionDoesntHaveOpenPositionOrOrders,
	#[msg("AllOrdersAreAlreadyLiquidations")]
	AllOrdersAreAlreadyLiquidations,
	#[msg("CantCancelLiquidationOrder")]
	CantCancelLiquidationOrder,
	#[msg("UserIsBeingLiquidated")]
	UserIsBeingLiquidated,
	#[msg("LiquidationsOngoing")]
	LiquidationsOngoing,
	#[msg("WrongSpotBalanceType")]
	WrongSpotBalanceType,
	#[msg("UserCantLiquidateThemself")]
	UserCantLiquidateThemself,
	#[msg("InvalidPerpPositionToLiquidate")]
	InvalidPerpPositionToLiquidate,
	#[msg("InvalidBaseAssetAmountForLiquidatePerp")]
	InvalidBaseAssetAmountForLiquidatePerp,

	#[msg("InvalidPositionDelta")]
	InvalidPositionDelta,
	#[msg("UserBankrupt")]
	UserBankrupt,
	#[msg("UserNotBankrupt")]
	UserNotBankrupt,
	#[msg("UserHasInvalidBorrow")]
	UserHasInvalidBorrow,
	#[msg("DailyWithdrawLimit")]
	DailyWithdrawLimit,
	#[msg("DefaultError")]
	DefaultError,
	#[msg("Insufficient LP tokens")]
	InsufficientLPTokens,
	#[msg("Cant LP with a market position")]
	CantLPWithPerpPosition,
	#[msg("Unable to burn LP tokens")]
	UnableToBurnLPTokens,
	#[msg("Trying to remove liqudity too fast after adding it")]
	TryingToRemoveLiquidityTooFast,
	#[msg("Invalid Spot Market Vault")]
	InvalidSpotMarketVault,
	#[msg("Invalid Spot Market State")]
	InvalidSpotMarketState,
	#[msg("InvalidSerumProgram")]
	InvalidSerumProgram,
	#[msg("InvalidSerumMarket")]
	InvalidSerumMarket,
	#[msg("InvalidSerumBids")]
	InvalidSerumBids,
	#[msg("InvalidSerumAsks")]
	InvalidSerumAsks,
	#[msg("InvalidSerumOpenOrders")]
	InvalidSerumOpenOrders,
	#[msg("FailedSerumCPI")]
	FailedSerumCPI,
	#[msg("FailedToFillOnExternalMarket")]
	FailedToFillOnExternalMarket,
	#[msg("InvalidFulfillmentConfig")]
	InvalidFulfillmentConfig,
	#[msg("InvalidFeeStructure")]
	InvalidFeeStructure,
	#[msg("Insufficient IF shares")]
	InsufficientIFShares,
	#[msg("the Market has paused this action")]
	MarketActionPaused,
	#[msg("the Market status doesnt allow placing orders")]
	MarketPlaceOrderPaused,
	#[msg("the Market status doesnt allow filling orders")]
	MarketFillOrderPaused,
	#[msg("the Market status doesnt allow withdraws")]
	MarketWithdrawPaused,
	#[msg("Action violates the Protected Asset Tier rules")]
	ProtectedAssetTierViolation,
	#[msg("Action violates the Isolated Asset Tier rules")]
	IsolatedAssetTierViolation,
	#[msg("User Cant Be Deleted")]
	UserCantBeDeleted,
	#[msg("Reduce Only Withdraw Increased Risk")]
	ReduceOnlyWithdrawIncreasedRisk,
	#[msg("Max Open Interest")]
	MaxOpenInterest,
	#[msg("Cant Resolve Perp Bankruptcy")]
	CantResolvePerpBankruptcy,
	#[msg("Liquidation Doesnt Satisfy Limit Price")]
	LiquidationDoesntSatisfyLimitPrice,
	#[msg("Margin Trading Disabled")]
	MarginTradingDisabled,
	#[msg("Invalid Market Status to Settle Perp Pnl")]
	InvalidMarketStatusToSettlePnl,
	#[msg("PerpMarketNotInSettlement")]
	PerpMarketNotInSettlement,
	#[msg("PerpMarketNotInReduceOnly")]
	PerpMarketNotInReduceOnly,
	#[msg("PerpMarketSettlementBufferNotReached")]
	PerpMarketSettlementBufferNotReached,
	#[msg("PerpMarketSettlementUserHasOpenOrders")]
	PerpMarketSettlementUserHasOpenOrders,
	#[msg("PerpMarketSettlementUserHasActiveLP")]
	PerpMarketSettlementUserHasActiveLP,
	#[msg("UnableToSettleExpiredUserPosition")]
	UnableToSettleExpiredUserPosition,
	#[msg("UnequalMarketIndexForSpotTransfer")]
	UnequalMarketIndexForSpotTransfer,
	#[msg("InvalidPerpPositionDetected")]
	InvalidPerpPositionDetected,
	#[msg("InvalidSpotPositionDetected")]
	InvalidSpotPositionDetected,
	#[msg("InvalidAmmDetected")]
	InvalidAmmDetected,
	#[msg("InvalidAmmForFillDetected")]
	InvalidAmmForFillDetected,
	#[msg("InvalidAmmLimitPriceOverride")]
	InvalidAmmLimitPriceOverride,
	#[msg("InvalidOrderFillPrice")]
	InvalidOrderFillPrice,
	#[msg("SpotMarketBalanceInvariantViolated")]
	SpotMarketBalanceInvariantViolated,
	#[msg("SpotMarketVaultInvariantViolated")]
	SpotMarketVaultInvariantViolated,
	#[msg("InvalidPDA")]
	InvalidPDA,
	#[msg("InvalidPDASigner")]
	InvalidPDASigner,
	#[msg("RevenueSettingsCannotSettleToIF")]
	RevenueSettingsCannotSettleToIF,
	#[msg("NoRevenueToSettleToIF")]
	NoRevenueToSettleToIF,
	#[msg("NoAmmPerpPnlDeficit")]
	NoAmmPerpPnlDeficit,
	#[msg("SufficientPerpPnlPool")]
	SufficientPerpPnlPool,
	#[msg("InsufficientPerpPnlPool")]
	InsufficientPerpPnlPool,
	#[msg("PerpPnlDeficitBelowThreshold")]
	PerpPnlDeficitBelowThreshold,
	#[msg("MaxRevenueWithdrawPerPeriodReached")]
	MaxRevenueWithdrawPerPeriodReached,
	#[msg("InvalidSpotPositionDetected")]
	MaxIFWithdrawReached,
	#[msg("NoIFWithdrawAvailable")]
	NoIFWithdrawAvailable,
	#[msg("InvalidIFUnstake")]
	InvalidIFUnstake,
	#[msg("InvalidIFUnstakeSize")]
	InvalidIFUnstakeSize,
	#[msg("InvalidIFUnstakeCancel")]
	InvalidIFUnstakeCancel,
	#[msg("InvalidIFForNewStakes")]
	InvalidIFForNewStakes,
	#[msg("InvalidIFRebase")]
	InvalidIFRebase,
	#[msg("InvalidInsuranceUnstakeSize")]
	InvalidInsuranceUnstakeSize,
	#[msg("InvalidOrderLimitPrice")]
	InvalidOrderLimitPrice,
	#[msg("InvalidIFDetected")]
	InvalidIFDetected,
	#[msg("InvalidAmmMaxSpreadDetected")]
	InvalidAmmMaxSpreadDetected,
	#[msg("InvalidConcentrationCoef")]
	InvalidConcentrationCoef,
	#[msg("InvalidSrmVault")]
	InvalidSrmVault,
	#[msg("InvalidVaultOwner")]
	InvalidVaultOwner,
	#[msg("InvalidMarketStatusForFills")]
	InvalidMarketStatusForFills,
	#[msg("IFWithdrawRequestInProgress")]
	IFWithdrawRequestInProgress,
	#[msg("NoIFWithdrawRequestInProgress")]
	NoIFWithdrawRequestInProgress,
	#[msg("IFWithdrawRequestTooSmall")]
	IFWithdrawRequestTooSmall,
	#[msg("IncorrectSpotMarketAccountPassed")]
	IncorrectSpotMarketAccountPassed,
	#[msg("BlockchainClockInconsistency")]
	BlockchainClockInconsistency,
	#[msg("InvalidIFSharesDetected")]
	InvalidIFSharesDetected,
	#[msg("NewLPSizeTooSmall")]
	NewLPSizeTooSmall,
	#[msg("MarketStatusInvalidForNewLP")]
	MarketStatusInvalidForNewLP,
	#[msg("InvalidMarkTwapUpdateDetected")]
	InvalidMarkTwapUpdateDetected,
	#[msg("MarketSettlementAttemptOnActiveMarket")]
	MarketSettlementAttemptOnActiveMarket,
	#[msg("MarketSettlementRequiresSettledLP")]
	MarketSettlementRequiresSettledLP,
	#[msg("MarketSettlementAttemptTooEarly")]
	MarketSettlementAttemptTooEarly,
	#[msg("MarketSettlementTargetPriceInvalid")]
	MarketSettlementTargetPriceInvalid,
	#[msg("UnsupportedSpotMarket")]
	UnsupportedSpotMarket,
	#[msg("SpotOrdersDisabled")]
	SpotOrdersDisabled,
	#[msg("Market Being Initialized")]
	MarketBeingInitialized,
	#[msg("Invalid Sub Account Id")]
	InvalidUserSubAccountId,
	#[msg("Invalid Trigger Order Condition")]
	InvalidTriggerOrderCondition,
	#[msg("Invalid Spot Position")]
	InvalidSpotPosition,
	#[msg("Cant transfer between same user account")]
	CantTransferBetweenSameUserAccount,
	#[msg("Invalid Perp Position")]
	InvalidPerpPosition,
	#[msg("Unable To Get Limit Price")]
	UnableToGetLimitPrice,
	#[msg("Invalid Liquidation")]
	InvalidLiquidation,
	#[msg("Spot Fulfillment Config Disabled")]
	SpotFulfillmentConfigDisabled,
	#[msg("Invalid Maker")]
	InvalidMaker,
	#[msg("Failed Unwrap")]
	FailedUnwrap,
	#[msg("Max Number Of Users")]
	MaxNumberOfUsers,
	#[msg("InvalidOracleForSettlePnl")]
	InvalidOracleForSettlePnl,
	#[msg("MarginOrdersOpen")]
	MarginOrdersOpen,
	#[msg("TierViolationLiquidatingPerpPnl")]
	TierViolationLiquidatingPerpPnl,
	#[msg("CouldNotLoadUserData")]
	CouldNotLoadUserData,
	#[msg("UserWrongMutability")]
	UserWrongMutability,
	#[msg("InvalidUserAccount")]
	InvalidUserAccount,
	#[msg("CouldNotLoadUserData")]
	CouldNotLoadUserStatsData,
	#[msg("UserWrongMutability")]
	UserStatsWrongMutability,
	#[msg("InvalidUserAccount")]
	InvalidUserStatsAccount,
	#[msg("UserNotFound")]
	UserNotFound,
	#[msg("UnableToLoadUserAccount")]
	UnableToLoadUserAccount,
	#[msg("UserStatsNotFound")]
	UserStatsNotFound,
	#[msg("UnableToLoadUserStatsAccount")]
	UnableToLoadUserStatsAccount,
	#[msg("User Not Inactive")]
	UserNotInactive,
	#[msg("RevertFill")]
	RevertFill,
	#[msg("Invalid MarketAccount for Deletion")]
	InvalidMarketAccountforDeletion,
	#[msg("Invalid Spot Fulfillment Params")]
	InvalidSpotFulfillmentParams,
	#[msg("Failed to Get Mint")]
	FailedToGetMint,
	#[msg("FailedPhoenixCPI")]
	FailedPhoenixCPI,
	#[msg("FailedToDeserializePhoenixMarket")]
	FailedToDeserializePhoenixMarket,
	#[msg("InvalidPricePrecision")]
	InvalidPricePrecision,
	#[msg("InvalidPhoenixProgram")]
	InvalidPhoenixProgram,
	#[msg("InvalidPhoenixMarket")]
	InvalidPhoenixMarket,
	#[msg("InvalidSwap")]
	InvalidSwap,
	#[msg("SwapLimitPriceBreached")]
	SwapLimitPriceBreached,
	#[msg("SpotMarketReduceOnly")]
	SpotMarketReduceOnly,

	#[msg("ImpossibleFill")]
	ImpossibleFill,
	#[msg("CantUpdatePerpBidAskTwap")]
	CantUpdatePerpBidAskTwap,
	#[msg("UserReduceOnly")]
	UserReduceOnly,
	#[msg("InvalidMarginCalculation")]
	InvalidMarginCalculation,
	#[msg("CantPayUserInitFee")]
	CantPayUserInitFee,
	#[msg("CantReclaimRent")]
	CantReclaimRent,
	#[msg("InsuranceFundOperationPaused")]
	InsuranceFundOperationPaused,
	#[msg("NoUnsettledPnl")]
	NoUnsettledPnl,
	#[msg("PnlPoolCantSettleUser")]
	PnlPoolCantSettleUser,
	#[msg("OracleInvalid")]
	OracleNonPositive,
	#[msg("OracleTooVolatile")]
	OracleTooVolatile,
	#[msg("OracleTooUncertain")]
	OracleTooUncertain,
	#[msg("OracleStaleForMargin")]
	OracleStaleForMargin,
	#[msg("OracleInsufficientDataPoints")]
	OracleInsufficientDataPoints,
	#[msg("OracleStaleForAMM")]
	OracleStaleForAMM,
	#[msg("Unable to parse pull oracle message")]
	UnableToParsePullOracleMessage,
	#[msg("Can not borow more than max borrows")]
	MaxBorrows,
	#[msg("Updates must be monotonically increasing")]
	OracleUpdatesNotMonotonic,
	#[msg("Trying to update price feed with the wrong feed id")]
	OraclePriceFeedMessageMismatch,
	#[msg("The message in the update must be a PriceFeedMessage")]
	OracleUnsupportedMessageType,
	#[msg("Could not deserialize the message in the update")]
	OracleDeserializeMessageFailed,
	#[msg("Wrong guardian set owner in update price atomic")]
	OracleWrongGuardianSetOwner,
	#[msg("Oracle post update atomic price feed account must be normal program")]
	OracleWrongWriteAuthority,
	#[msg("Oracle vaa owner must be wormhole program")]
	OracleWrongVaaOwner,
	#[msg(
		"Multi updates must have 2 or fewer accounts passed in remaining accounts"
	)]
	OracleTooManyPriceAccountUpdates,
	#[msg(
		"Don't have the same remaining accounts number and merkle price updates left"
	)]
	OracleMismatchedVaaAndPriceUpdates,
	#[msg("Remaining account passed is not a valid pda")]
	OracleBadRemainingAccountPublicKey,
	#[msg("FailedOpenbookV2CPI")]
	FailedOpenbookV2CPI,
	#[msg("InvalidOpenbookV2Program")]
	InvalidOpenbookV2Program,
	#[msg("InvalidOpenbookV2Market")]
	InvalidOpenbookV2Market,
	#[msg("Non zero transfer fee")]
	NonZeroTransferFee,
	#[msg("Liquidation order failed to fill")]
	LiquidationOrderFailedToFill,

	#[msg("Market index mismatched b/w taker and maker swift order params")]
	MismatchedSwiftOrderParamsMarketIndex,
	#[msg("Swift only available for market/oracle perp orders")]
	InvalidSwiftOrderParam,
	#[msg("Place and take order success condition failed")]
	PlaceAndTakeOrderSuccessConditionFailed,

	// Orca
	#[msg("Enum value could not be converted")]
	InvalidEnum, // 0x1770 (6000)
	#[msg("Invalid start tick index provided.")]
	InvalidStartTick, // 0x1771 (6001)
	#[msg("Tick-array already exists in this amm")]
	TickArrayExistInPool, // 0x1772 (6002)
	#[msg("Attempt to search for a tick-array failed")]
	TickArrayIndexOutofBounds, // 0x1773 (6003)
	#[msg("Tick-spacing is not supported")]
	InvalidTickSpacing, // 0x1774 (6004)
	#[msg("Position is not empty It cannot be closed")]
	ClosePositionNotEmpty, // 0x1775 (6005)

	#[msg("Unable to divide by zero")]
	DivideByZero, // 0x1776 (6006)
	#[msg("Unable to cast number into BigInt")]
	NumberCastError, //  0x1777 (6007)
	#[msg("Unable to down cast number")]
	NumberDownCastError, //  0x1778 (6008)

	#[msg("Tick not found within tick array")]
	TickNotFound, // 0x1779 (6009)
	#[msg("Provided tick index is either out of bounds or uninitializable")]
	InvalidTickIndex, // 0x177a (6010)
	#[msg("Provided sqrt price out of bounds")]
	SqrtPriceOutOfBounds, // 0x177b (6011)

	#[msg("Liquidity amount must be greater than zero")]
	LiquidityZero, // 0x177c (6012)
	#[msg("Liquidity amount must be less than i64::MAX")]
	LiquidityTooHigh, // 0x177d (6013)
	#[msg("Liquidity overflow")]
	LiquidityOverflow, // 0x177e (6014)
	#[msg("Liquidity underflow")]
	LiquidityUnderflow, // 0x177f (6015)
	#[msg("Tick liquidity net underflowed or overflowed")]
	LiquidityNetError, // 0x1780 (6016)

	#[msg("Exceeded token max")]
	TokenMaxExceeded, // 0x1781 (6017)
	#[msg("Did not meet token min")]
	TokenMinSubceeded, // 0x1782 (6018)

	#[msg("Position token account has a missing or invalid delegate")]
	MissingOrInvalidDelegate, // 0x1783 (6019)
	#[msg("Position token amount must be 1")]
	InvalidPositionTokenAmount, // 0x1784 (6020)

	#[msg("Timestamp should be convertible from i64 to u64")]
	InvalidTimestampConversion, // 0x1785 (6021)
	#[msg("Timestamp should be greater than the last updated timestamp")]
	InvalidTimestamp, // 0x1786 (6022)

	#[msg("Invalid tick array sequence provided for instruction.")]
	InvalidTickArraySequence, // 0x1787 (6023)
	#[msg("Token Mint in wrong order")]
	InvalidTokenMintOrder, // 0x1788 (6024)

	#[msg("Reward not initialized")]
	RewardNotInitialized, // 0x1789 (6025)
	#[msg("Invalid reward index")]
	InvalidRewardIndex, // 0x178a (6026)

	#[msg(
		"Reward vault requires amount to support emissions for at least one day"
	)]
	RewardVaultAmountInsufficient, // 0x178b (6027)
	#[msg("Exceeded max fee rate")]
	FeeRateMaxExceeded, // 0x178c (6028)
	#[msg("Exceeded max protocol fee rate")]
	ProtocolFeeRateMaxExceeded, // 0x178d (6029)

	#[msg("Multiplication with shift right overflow")]
	MultiplicationShiftRightOverflow, // 0x178e (6030)
	#[msg("Muldiv overflow")]
	MulDivOverflow, // 0x178f (6031)
	#[msg("Invalid div_u256 input")]
	MulDivInvalidInput, // 0x1790 (6032)
	#[msg("Multiplication overflow")]
	MultiplicationOverflow, // 0x1791 (6033)

	#[msg("Provided SqrtPriceLimit not in the same direction as the swap.")]
	InvalidSqrtPriceLimitDirection, // 0x1792 (6034)
	#[msg("There are no tradable amount to swap.")]
	ZeroTradableAmount, // 0x1793 (6035)

	#[msg("Amount out below minimum threshold")]
	AmountOutBelowMinimum, // 0x1794 (6036)
	#[msg("Amount in above maximum threshold")]
	AmountInAboveMaximum, // 0x1795 (6037)

	#[msg("Invalid index for tick array sequence")]
	TickArraySequenceInvalidIndex, // 0x1796 (6038)

	#[msg("Amount calculated overflows")]
	AmountCalcOverflow, // 0x1797 (6039)
	#[msg("Amount remaining overflows")]
	AmountRemainingOverflow, // 0x1798 (6040)

	#[msg("Invalid intermediary mint")]
	InvalidIntermediaryMint, // 0x1799 (6041)
	#[msg("Duplicate two hop pool")]
	DuplicateTwoHopPool, // 0x179a (6042)

	#[msg("Token mint has unsupported attributes")]
	UnsupportedTokenMint, // 0x179f (6047)

	#[msg("Invalid remaining accounts")]
	RemainingAccountsInvalidSlice, // 0x17a0 (6048)
	#[msg("Insufficient remaining accounts")]
	RemainingAccountsInsufficient, // 0x17a1 (6049)

	#[msg("Unable to call transfer hook without extra accounts")]
	NoExtraAccountsForTransferHook, // 0x17a2 (6050)

	#[msg("Output and input amount mismatch")]
	IntermediateTokenAmountMismatch, // 0x17a3 (6051)

	#[msg("Transfer fee calculation failed")]
	TransferFeeCalculationError, // 0x17a4 (6052)

	#[msg("Same accounts type is provided more than once")]
	RemainingAccountsDuplicatedAccountsType, // 0x17a5 (6053)

	#[msg("This amm only supports full-range positions")]
	FullRangeOnlyPool, // 0x17a6 (6054)

	#[msg("Too many supplemental tick arrays provided")]
	TooManySupplementalTickArrays, // 0x17a7 (6055)
	#[msg("TickArray account for different amm provided")]
	DifferentAMMTickArrayAccount, // 0x17a8 (6056)

	#[msg("Trade resulted in partial fill")]
	PartialFillError, // 0x17a9 (6057)
}

// Orca
impl From<TryFromIntError> for ErrorCode {
	fn from(_: TryFromIntError) -> Self {
		ErrorCode::NumberCastError
	}
}

#[macro_export]
macro_rules! print_error {
	($err:expr) => {
		{
        || {
            let error_code: ErrorCode = $err;
            msg!("{:?} thrown at {}:{}", error_code, file!(), line!());
            $err
        }
		}
	};
}

#[macro_export]
macro_rules! math_error {
	() => {
		{
        || {
            let error_code = $crate::errors::ErrorCode::MathError;
            msg!("Error {} thrown at {}:{}", error_code, file!(), line!());
            error_code
        }
		}
	};
}
