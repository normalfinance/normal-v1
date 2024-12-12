use solana_program::msg;

// #[cfg(test)]
// mod tests;

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum VaultOperation {
	Create = 0b00000001,
	Deposit = 0b00000010,
	Withdraw = 0b00000100,
	Lend = 0b00001000,
	Transfer = 0b00010000,
	Delete = 0b00100000,
	Liquidation = 0b01000000,
}

const ALL_VAULT_OPERATIONS: [VaultOperation; 7] = [
	VaultOperation::Create,
	VaultOperation::Delete,
	VaultOperation::Withdraw,
	VaultOperation::Lend,
	VaultOperation::Transfer,
	VaultOperation::Delete,
	VaultOperation::Liquidation,
];

impl VaultOperation {
	pub fn is_operation_paused(current: u8, operation: VaultOperation) -> bool {
		(current & (operation as u8)) != 0
	}

	pub fn log_all_operations_paused(current: u8) {
		for operation in ALL_VAULT_OPERATIONS.iter() {
			if Self::is_operation_paused(current, *operation) {
				msg!("{:?} is paused", operation);
			}
		}
	}
}

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum SynthOperation {
	Swap = 0b00000001,
}

const ALL_SYNTH_OPERATIONS: [SynthOperation; 1] = [SynthOperation::Swap];

impl SynthOperation {
	pub fn is_operation_paused(current: u8, operation: SynthOperation) -> bool {
		(current & (operation as u8)) != 0
	}

	pub fn log_all_operations_paused(current: u8) {
		for operation in ALL_SYNTH_OPERATIONS.iter() {
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
