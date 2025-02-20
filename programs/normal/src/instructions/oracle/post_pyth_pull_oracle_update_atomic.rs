use crate::errors::ErrorCode;
use crate::ids::{ normal_oracle_receiver_program, wormhole_program };
use crate::{math, validate};
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

#[derive(Accounts)]
#[instruction(feed_id : [u8; 32])]
pub struct PostPythPullOracleUpdateAtomic<'info> {
	#[account(mut)]
	pub keeper: Signer<'info>,
	pub pyth_solana_receiver: Program<'info, PythSolanaReceiver>,
	/// CHECK: We can't use AccountVariant::<GuardianSet> here because its owner is hardcoded as the "official" Wormhole program
	#[account(
        owner = wormhole_program::id() @ ErrorCode::OracleWrongGuardianSetOwner)]
	pub guardian_set: AccountInfo<'info>,

	/// CHECK: This account's seeds are checked
	#[account(mut, owner = normal_oracle_receiver_program::id(), seeds = [PTYH_PRICE_FEED_SEED_PREFIX, &feed_id], bump)]
	pub price_feed: AccountInfo<'info>,
}

pub fn handle_post_pyth_pull_oracle_update_atomic(
	ctx: Context<PostPythPullOracleUpdateAtomic>,
	feed_id: [u8; 32],
	params: Vec<u8>
) -> Result<()> {
	let cpi_program = ctx.accounts.pyth_solana_receiver.to_account_info();
	let cpi_accounts = PostUpdateAtomic {
		payer: ctx.accounts.keeper.to_account_info(),
		guardian_set: ctx.accounts.guardian_set.to_account_info(),
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

	let params = PostUpdateAtomicParams::deserialize(&mut &params[..]).unwrap();

	// Get the timestamp of the price currently stored in the price feed account.
	let current_timestamp = math::oracle::get_timestamp_from_price_feed_account(
		&ctx.accounts.price_feed
	)?;
	let next_timestamp = math::oracle::get_timestamp_from_price_update_message(
		&params.merkle_price_update.message
	)?;

	if next_timestamp > current_timestamp {
		pyth_solana_receiver_sdk::cpi::post_update_atomic(cpi_context, params)?;

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

		msg!(
			"Posting new update. current ts {} < next ts {}",
			current_timestamp,
			next_timestamp
		);
	} else {
		msg!(
			"Skipping new update. current ts {} >= next ts {}",
			current_timestamp,
			next_timestamp
		);
	}

	Ok(())
}
