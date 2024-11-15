use crate::error::ErrorCode;
use crate::ids::{ normal_oracle_receiver_program, wormhole_program };
use crate::{ math, validate };
use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::{
	cpi::accounts::{ PostUpdate, PostUpdateAtomic },
	price_update::PriceUpdateV2,
	program::PythSolanaReceiver,
	PostMultiUpdatesAtomicParams,
	PostUpdateAtomicParams,
	PostUpdateParams,
};
use pythnet_sdk::{ messages::Message, wire::{ from_slice, PrefixedVec } };

pub const PTYH_PRICE_FEED_SEED_PREFIX: &[u8] = b"pyth_pull";

#[derive(Accounts)]
#[instruction(feed_id : [u8; 32])]
pub struct UpdatePythPullOraclePriceFeed<'info> {
	#[account(mut)]
	pub keeper: Signer<'info>,
	pub pyth_solana_receiver: Program<'info, PythSolanaReceiver>,
	/// CHECK: Checked by CPI into the Pyth Solana Receiver
	#[account(owner = wormhole_program::id() @ ErrorCode::OracleWrongVaaOwner)]
	pub encoded_vaa: AccountInfo<'info>,
	/// CHECK: This account's seeds are checked
	#[account(mut, seeds = [PTYH_PRICE_FEED_SEED_PREFIX, &feed_id], bump, owner = normal_oracle_receiver_program::id())]
	pub price_feed: AccountInfo<'info>,
}

pub fn handle_update_pyth_pull_oracle(
	ctx: Context<UpdatePythPullOraclePriceFeed>,
	feed_id: [u8; 32],
	params: Vec<u8>
) -> Result<()> {
	let cpi_program = ctx.accounts.pyth_solana_receiver.to_account_info();
	let cpi_accounts = PostUpdate {
		payer: ctx.accounts.keeper.to_account_info(),
		encoded_vaa: ctx.accounts.encoded_vaa.to_account_info(),
		price_update_account: ctx.accounts.price_feed.to_account_info(),
		write_authority: ctx.accounts.price_feed.to_account_info(),
	};

	let seeds = &[
		PTYH_PRICE_FEED_SEED_PREFIX,
		feed_id.as_ref(),
		&[ctx.bumps.price_feed],
	];
	let signer_seeds = &[&seeds[..]];
	let cpi_context = CpiContext::new_with_signer(
		cpi_program,
		cpi_accounts,
		signer_seeds
	);

	let params = PostUpdateParams::deserialize(&mut &params[..]).unwrap();

	// Get the timestamp of the price currently stored in the price feed account.
	let current_timestamp = math::oracle::get_timestamp_from_price_feed_account(
		&ctx.accounts.price_feed
	)?;
	let next_timestamp = math::oracle::get_timestamp_from_price_update_message(
		&params.merkle_price_update.message
	)?;

	// Only update the price feed if the message contains a newer price. Pushing a stale price
	// suceeds without changing the on-chain state.
	if next_timestamp > current_timestamp {
		pyth_solana_receiver_sdk::cpi::post_update(cpi_context, params)?;
		{
			let price_feed_account_data = ctx.accounts.price_feed.try_borrow_data()?;
			let price_feed_account = PriceUpdateV2::try_deserialize(
				&mut &price_feed_account_data[..]
			)?;

			validate!(
				price_feed_account.price_message.feed_id == feed_id,
				ErrorCode::OraclePriceFeedMessageMismatch
			)?;
		}
	}
	Ok(())
}
