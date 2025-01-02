use anchor_lang::prelude::*;

use crate::{ load_mut, state::index_market::{ IndexMarket, UpdateWeight } };

use super::UpdateIndexMarket;

pub fn handle_rebalance_index_market(
	ctx: Context<UpdateIndexMarket>,
	updates: Vec<UpdateWeight>
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let index_market = &mut load_mut!(ctx.accounts.index_market)?;

	for operation in operations.iter() {
		if let Some(new_weight) = operation.new_weight {
			// Update the weight
			if
				let Some(asset) = index_fund.assets
					.iter_mut()
					.find(|a| a.market_index == operation.market_index)
			{
				asset.weight = new_weight;
				asset.last_updated_ts = Clock::get()?.unix_timestamp;
			} else {
				return Err(error!(CustomError::AssetNotFound));
			}
		} else {
			// Remove the asset
			index_fund.assets.retain(
				|asset| asset.market_index != operation.market_index
			);
		}
	}

	index_fund.last_updated_ts = now;

	Ok(())
}
