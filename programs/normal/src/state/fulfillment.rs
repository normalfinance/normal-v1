use solana_program::pubkey::Pubkey;

#[derive(Debug, PartialEq, Eq)]
pub enum FulfillmentMethod {
    AMM(Option<u64>),
    Match(Pubkey, u16),
}
