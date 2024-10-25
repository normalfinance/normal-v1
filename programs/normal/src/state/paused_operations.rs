use solana_program::msg;

// #[cfg(test)]
// mod tests;

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum Operation {
	AmmFill = 0b00000001,
	Fill = 0b00000010,
}

const ALL_OPERATIONS: [Operation; 2] = [Operation::AmmFill, Operation::Fill];

impl Operation {
	pub fn is_operation_paused(current: u8, operation: Operation) -> bool {
		(current & (operation as u8)) != 0
	}

	pub fn log_all_operations_paused(current: u8) {
		for operation in ALL_OPERATIONS.iter() {
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
