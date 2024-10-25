use anchor_lang::prelude::*;

pub type NormalResult<T = ()> = std::result::Result<T, ErrorCode>;

#[error_code]
#[derive(PartialEq, Eq)]
pub enum ErrorCode {
	// Custom
	#[msg("Invalid Governance Proposal")]
	InvalidGovernanceProposial,

	// Insurance
	#[msg("NoFeesToDepotiToInsuranceFund")]
	NoFeesToDepotiToInsuranceFund,

	#[msg("Invalid Market Authority")]
	InvalidMarketAuthority,
	#[msg("Clearing house not insurance fund authority")]
	InvalidInsuranceFundAuthority,

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

	#[msg("Casting Failure")]
	CastingFailure,
	#[msg("InvalidOrder")]
	InvalidOrder,
	#[msg("InvalidOrderMaxTs")]
	InvalidOrderMaxTs,
	#[msg("InvalidOrderMarketType")]
	InvalidOrderMarketType,
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
	#[msg("Cant Cancel Post Only Order")]
	CantCancelPostOnlyOrder,

	#[msg("CantExpireOrders")]
	CantExpireOrders,
	#[msg("CouldNotLoadMarketData")]
	CouldNotLoadMarketData,
	#[msg("MarketNotFound")]
	MarketNotFound,
	#[msg("InvalidMarketAccount")]
	InvalidMarketAccount,
	#[msg("UnableToLoadMarketAccount")]
	UnableToLoadMarketAccount,
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

	#[msg("CantUpdatePoolBalanceType")]
	CantUpdatePoolBalanceType,

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
	#[msg("PositionDoesntHaveOpenPositionOrOrders")]
	PositionDoesntHaveOpenPositionOrOrders,
	#[msg("WrongSpotBalanceType")]
	WrongSpotBalanceType,

	#[msg("InvalidPositionDelta")]
	InvalidPositionDelta,

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

	#[msg("InvalidFulfillmentConfig")]
	InvalidFulfillmentConfig,
	#[msg("InvalidFeeStructure")]
	InvalidFeeStructure,
	#[msg("the Market has paused this action")]
	MarketActionPaused,
	#[msg("the Market status doesnt allow placing orders")]
	MarketPlaceOrderPaused,
	#[msg("the Market status doesnt allow filling orders")]
	MarketFillOrderPaused,

	#[msg("Action violates the Protected Synthetic Tier rules")]
	ProtectedSyntheticTierViolation,
	#[msg("Action violates the Isolated Synthetic Tier rules")]
	IsolatedSyntheticTierViolation,
	#[msg("User Cant Be Deleted")]
	UserCantBeDeleted,

	#[msg("Max Open Interest")]
	MaxOpenInterest,

	#[msg("PerpMarketNotInSettlement")]
	PerpMarketNotInSettlement,
	#[msg("PerpMarketNotInReduceOnly")]
	PerpMarketNotInReduceOnly,
	#[msg("PerpMarketSettlementUserHasActiveLP")]
	PerpMarketSettlementUserHasActiveLP,
	#[msg("UnequalMarketIndexForSpotTransfer")]
	UnequalMarketIndexForSpotTransfer,

	#[msg("InvalidPositionDetected")]
	InvalidPositionDetected,

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
	#[msg("NoAmmPerpPnlDeficit")]
	NoAmmPerpPnlDeficit,
	#[msg("SufficientPerpPnlPool")]
	SufficientPerpPnlPool,
	#[msg("InsufficientPerpPnlPool")]
	InsufficientPerpPnlPool,
	#[msg("PerpPnlDeficitBelowThreshold")]
	PerpPnlDeficitBelowThreshold,
	#[msg("InvalidInsuranceUnstakeSize")]
	InvalidInsuranceUnstakeSize,
	#[msg("InvalidOrderLimitPrice")]
	InvalidOrderLimitPrice,
	#[msg("InvalidAmmMaxSpreadDetected")]
	InvalidAmmMaxSpreadDetected,
	#[msg("InvalidConcentrationCoef")]
	InvalidConcentrationCoef,

	#[msg("InvalidVaultOwner")]
	InvalidVaultOwner,
	#[msg("InvalidMarketStatusForFills")]
	InvalidMarketStatusForFills,
	#[msg("IncorrectSpotMarketAccountPassed")]
	IncorrectSpotMarketAccountPassed,
	#[msg("BlockchainClockInconsistency")]
	BlockchainClockInconsistency,
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

	#[msg("Spot Fulfillment Config Disabled")]
	SpotFulfillmentConfigDisabled,
	#[msg("Invalid Maker")]
	InvalidMaker,
	#[msg("Failed Unwrap")]
	FailedUnwrap,
	#[msg("Max Number Of Users")]
	MaxNumberOfUsers,

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

	#[msg("Failed to Get Mint")]
	FailedToGetMint,

	#[msg("InvalidPricePrecision")]
	InvalidPricePrecision,

	#[msg("SpotMarketReduceOnly")]
	SpotMarketReduceOnly,

	#[msg("ImpossibleFill")]
	ImpossibleFill,
	#[msg("CantUpdatePerpBidAskTwap")]
	CantUpdatePerpBidAskTwap,
	#[msg("UserReduceOnly")]
	UserReduceOnly,

	#[msg("CantPayUserInitFee")]
	CantPayUserInitFee,
	#[msg("CantReclaimRent")]
	CantReclaimRent,
	#[msg("InsuranceFundOperationPaused")]
	InsuranceFundOperationPaused,

	#[msg("OracleInvalid")]
	OracleNonPositive,
	#[msg("OracleTooVolatile")]
	OracleTooVolatile,
	#[msg("OracleTooUncertain")]
	OracleTooUncertain,

	#[msg("OracleInsufficientDataPoints")]
	OracleInsufficientDataPoints,
	#[msg("OracleStaleForAMM")]
	OracleStaleForAMM,
	#[msg("Unable to parse pull oracle message")]
	UnableToParsePullOracleMessage,

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

	#[msg("Non zero transfer fee")]
	NonZeroTransferFee,

	#[msg("Ed25519 Ix must be before place and make swift order ix")]
	InvalidVerificationIxIndex,
	#[msg("Swift message verificaiton failed")]
	SigVerificationFailed,
	#[msg("Market index mismatched b/w taker and maker swift order params")]
	MismatchedSwiftOrderParamsMarketIndex,
	#[msg("Swift only available for market/oracle perp orders")]
	InvalidSwiftOrderParam,
	#[msg("Place and take order success condition failed")]
	PlaceAndTakeOrderSuccessConditionFailed,
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
            let error_code = $crate::error::ErrorCode::MathError;
            msg!("Error {} thrown at {}:{}", error_code, file!(), line!());
            error_code
        }
        }
	};
}
