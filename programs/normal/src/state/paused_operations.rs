use solana_program::msg;

// #[cfg(test)]
// mod tests;

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum Operation {
    // UpdateFunding = 0b00000001,
    AmmFill = 0b00000010,
    Fill = 0b00000100,
    // SettlePnl = 0b00001000,
    // SettlePnlWithPosition = 0b00010000,
    // Liquidation = 0b00100000,
}

const ALL_OPERATIONS: [Operation; 4] = [
    // Operation::UpdateFunding,
    Operation::AmmFill,
    Operation::Fill,
    // Operation::SettlePnl,
    // Operation::SettlePnlWithPosition,
    // Operation::Liquidation,
];

impl Operation {
    pub fn is_operation_paused(current: u8, operation: Operation) -> bool {
        current & operation as u8 != 0
    }

    pub fn log_all_operations_paused(current: u8) {
        for operation in ALL_OPERATIONS.iter() {
            if Self::is_operation_paused(current, *operation) {
                msg!("{:?} is paused", operation);
            }
        }
    }
}