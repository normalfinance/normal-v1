use anchor_lang::prelude::*;

pub mod amm_nft_update_auth {
	use super::*;
	declare_id!("3axbTs2z5GBy6usVbNVoqEgZMng3vZvMnAoX29BFfwhr");
}

// Based on Metaplex TokenMetadata
//
// METADATA_NAME   : max  32 bytes
// METADATA_SYMBOL : max  10 bytes
// METADATA_URI    : max 200 bytes
pub const WP_METADATA_NAME: &str = "Normal AMM Position";
pub const WP_METADATA_SYMBOL: &str = "NAP";
pub const WP_METADATA_URI: &str =
	"https://arweave.net/P_OoP15YyiZZu6LlxRFEWdbFTOjPsT23oHbbUZNXIJM";

// Based on Token-2022 TokenMetadata extension
//
// There is no clear upper limit on the length of name, symbol, and uri,
// but it is safe for wallet apps to limit the uri to 128 bytes.
//
// see also: TokenMetadata struct
// https://github.com/solana-labs/solana-program-library/blob/cd6ce4b7709d2420bca60b4656bbd3d15d2e1485/token-metadata/interface/src/state.rs#L25
pub const WP_2022_METADATA_NAME_PREFIX: &str = "NAP";
pub const WP_2022_METADATA_SYMBOL: &str = "NAP";
pub const WP_2022_METADATA_URI_BASE: &str =
	"https://position-nft.normalfinance.io/meta";
