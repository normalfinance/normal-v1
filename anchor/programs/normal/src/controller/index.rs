use crate::math;
use crate::state::index_market::IndexMarket;
use crate::state::synth_market::SynthMarket;
use crate::state::events::IndexFundRebalanceRecord;

pub fn rebalance(market: &mut Market, now: i64) -> NormalResult<()> {
	// rebalance...

	let weights = math::index::generate_weights(method);

	emit!(IndexFundRebalanceRecord {
		ts: now,
	});

	Ok(())
}
