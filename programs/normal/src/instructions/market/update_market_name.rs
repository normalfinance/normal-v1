use anchor_lang::prelude::*;

use super::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_name(
	ctx: Context<AdminUpdateMarket>,
	name: [u8; 32]
) -> Result<()> {
	let mut market = load_mut!(ctx.accounts.market)?;
	msg!("market.name: {:?} -> {:?}", market.name, name);
	market.name = name;
	Ok(())
}
