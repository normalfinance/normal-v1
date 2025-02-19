use solana_program::msg;

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum MarketOperation {
	Create = 0b00000001,
	Deposit = 0b00000010,
	Withdraw = 0b00000100,
	Lend = 0b00001000,
	Transfer = 0b00010000,
	Delete = 0b00100000,
	Liquidation = 0b01000000,
	// Swap = 0,
}

const ALL_MARKET_OPERATIONS: [MarketOperation; 7] = [
	MarketOperation::Create,
	MarketOperation::Delete,
	MarketOperation::Withdraw,
	MarketOperation::Lend,
	MarketOperation::Transfer,
	MarketOperation::Delete,
	MarketOperation::Liquidation,
];

impl MarketOperation {
	pub fn is_operation_paused(current: u8, operation: MarketOperation) -> bool {
		(current & (operation as u8)) != 0
	}

	pub fn log_all_operations_paused(current: u8) {
		for operation in ALL_MARKET_OPERATIONS.iter() {
			if Self::is_operation_paused(current, *operation) {
				msg!("{:?} is paused", operation);
			}
		}
	}
}

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum InsuranceFundOperation {
	Init = 0b00000001,
	Add = 0b00000010,
	RequestRemove = 0b00000100,
	Remove = 0b00001000,
}

const ALL_IF_OPERATIONS: [InsuranceFundOperation; 4] = [
	InsuranceFundOperation::Init,
	InsuranceFundOperation::Add,
	InsuranceFundOperation::RequestRemove,
	InsuranceFundOperation::Remove,
];

impl InsuranceFundOperation {
	pub fn is_operation_paused(
		current: u8,
		operation: InsuranceFundOperation
	) -> bool {
		(current & (operation as u8)) != 0
	}

	pub fn log_all_operations_paused(current: u8) {
		for operation in ALL_IF_OPERATIONS.iter() {
			if Self::is_operation_paused(current, *operation) {
				msg!("{:?} is paused", operation);
			}
		}
	}
}
