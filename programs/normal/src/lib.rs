#![allow(clippy::too_many_arguments)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::comparison_chain)]

use anchor_lang::prelude::*;

use instructions::*;
#[cfg(test)]
use math::amm;
use math::{ bn, constants::* };
use state::oracle::OracleSource;

// use crate::controller::position::PositionDirection;
use crate::state::oracle::PrelaunchOracleParams;
// use crate::state::order_params::{ ModifyOrderParams, OrderParams };
// use crate::state::perp_market::{ ContractTier, MarketStatus };
// use crate::state::settle_pnl_mode::SettlePnlMode;
// use crate::state::state::FeeStructure;
use crate::state::state::*;
// use crate::state::user::MarketType;

// pub mod controller;
pub mod error;
pub mod ids;
// pub mod instructions;
// pub mod macros;
pub mod math;
// mod signer;
pub mod state;
#[cfg(test)]
mod test_utils;
mod validation;

#[cfg(feature = "mainnet-beta")]
declare_id!("dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH");
#[cfg(not(feature = "mainnet-beta"))]
declare_id!("dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH");

#[program]
pub mod normal {
    use super::*;

    // User Instructions

    pub fn initialize_user<'c: 'info, 'info>(
        ctx: Context<'_, '_, 'c, 'info, InitializeUser<'info>>,
        sub_account_id: u16,
        name: [u8; 32]
    ) -> Result<()> {
        handle_initialize_user(ctx, sub_account_id, name)
    }
}

#[cfg(not(feature = "no-entrypoint"))]
use solana_security_txt::security_txt;
#[cfg(not(feature = "no-entrypoint"))]
security_txt! {
    name: "Normal v1",
    project_url: "https://normalfinance.io",
    contacts: "link:https://docs.normalfinance.io/security/bug-bounty",
    policy: "https://github.com/normalfinance/normal-v1/blob/main/SECURITY.md",
    preferred_languages: "en",
    source_code: "https://github.com/normalfinance/normal-v1"
}
