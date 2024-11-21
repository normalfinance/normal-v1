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

#[derive(Accounts)]
pub struct PostPythPullMultiOracleUpdatesAtomic<'info> {
	#[account(mut)]
	pub keeper: Signer<'info>,
	pub pyth_solana_receiver: Program<'info, PythSolanaReceiver>,
	/// CHECK: We can't use AccountVariant::<GuardianSet> here because its owner is hardcoded as the "official" Wormhole program
	#[account(
        owner = wormhole_program::id() @ ErrorCode::OracleWrongGuardianSetOwner)]
	pub guardian_set: AccountInfo<'info>,
}

pub fn handle_post_multi_pyth_pull_oracle_updates_atomic<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, PostPythPullMultiOracleUpdatesAtomic<'info>>,
	params: Vec<u8>
) -> Result<()> {
	let remaining_accounts = ctx.remaining_accounts;
	validate!(
		remaining_accounts.len() <= 2,
		ErrorCode::OracleTooManyPriceAccountUpdates
	)?;
	let update_param = PostMultiUpdatesAtomicParams::deserialize(
		&mut &params[..]
	).unwrap();
	let vaa = update_param.vaa;
	let merkle_price_updates = update_param.merkle_price_updates;

	validate!(
		remaining_accounts.len() == merkle_price_updates.len(),
		ErrorCode::OracleMismatchedVaaAndPriceUpdates
	)?;

	for (account, merkle_price_update) in remaining_accounts
		.iter()
		.zip(merkle_price_updates.iter()) {
		let cpi_program = ctx.accounts.pyth_solana_receiver.to_account_info();
		let cpi_accounts = PostUpdateAtomic {
			payer: ctx.accounts.keeper.to_account_info(),
			guardian_set: ctx.accounts.guardian_set.to_account_info(),
			price_update_account: account.clone(),
			write_authority: account.clone(),
		};

		let price_feed_account_data = account.try_borrow_data()?;
		let price_feed_account = PriceUpdateV2::try_deserialize(
			&mut &price_feed_account_data[..]
		)?;
		let feed_id = price_feed_account.price_message.feed_id;

		// Verify the pda
		let (pda, bump) = Pubkey::find_program_address(
			&[PTYH_PRICE_FEED_SEED_PREFIX, feed_id.as_ref()],
			&crate::ID
		);
		require_keys_eq!(
			*account.key,
			pda,
			ErrorCode::OracleBadRemainingAccountPublicKey
		);

		let seeds = &[PTYH_PRICE_FEED_SEED_PREFIX, feed_id.as_ref(), &[bump]];

		let signer_seeds = &[&seeds[..]];
		let cpi_context = CpiContext::new_with_signer(
			cpi_program,
			cpi_accounts,
			signer_seeds
		);

		// Get the timestamp of the price currently stored in the price feed account.
		let current_timestamp =
			math::oracle::get_timestamp_from_price_feed_account(account)?;
		let next_timestamp = math::oracle::get_timestamp_from_price_update_message(
			&merkle_price_update.message
		)?;

		drop(price_feed_account_data);

		if next_timestamp > current_timestamp {
			pyth_solana_receiver_sdk::cpi::post_update_atomic(
				cpi_context,
				PostUpdateAtomicParams {
					merkle_price_update: merkle_price_update.clone(),
					vaa: vaa.clone(),
				}
			)?;

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
	}

	Ok(())
}
