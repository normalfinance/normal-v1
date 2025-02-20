use anchor_lang::prelude::*;

use crate::errors::ErrorCode;

// Max & min tick index based on sqrt(1.0001) & max.min price of 2^64
pub const MAX_TICK_INDEX: i32 = 443636;
pub const MIN_TICK_INDEX: i32 = -443636;

// We have two consts because most of our code uses it as a i32. However,
// for us to use it in tick array declarations, anchor requires it to be a usize.
pub const TICK_ARRAY_SIZE: i32 = 88;
pub const TICK_ARRAY_SIZE_USIZE: usize = 88;

#[zero_copy(unsafe)]
#[repr(C, packed)]
#[derive(Default, Debug, PartialEq)]
pub struct Tick {
	// Total 137 bytes
	pub initialized: bool, // 1
	pub liquidity_net: i128, // 16
	pub liquidity_gross: u128, // 16

	// Q64.64
	pub fee_growth_outside_a: u128, // 16
	// Q64.64
	pub fee_growth_outside_b: u128, // 16

	// Array of Q64.64
	pub reward_growths_outside: [u128; NUM_REWARDS], // 48 = 16 * 3
}

impl Tick {
	pub const LEN: usize = 113;

	/// Apply an update for this tick
	///
	/// # Parameters
	/// - `update` - An update object to update the values in this tick
	pub fn update(&mut self, update: &TickUpdate) {
		self.initialized = update.initialized;
		self.liquidity_net = update.liquidity_net;
		self.liquidity_gross = update.liquidity_gross;
		self.fee_growth_outside_a = update.fee_growth_outside_a;
		self.fee_growth_outside_b = update.fee_growth_outside_b;
		self.reward_growths_outside = update.reward_growths_outside;
	}

	/// Check that the tick index is within the supported range of this contract
	///
	/// # Parameters
	/// - `tick_index` - A i32 integer representing the tick index
	///
	/// # Returns
	/// - `true`: The tick index is not within the range supported by this contract
	/// - `false`: The tick index is within the range supported by this contract
	pub fn check_is_out_of_bounds(tick_index: i32) -> bool {
		!(MIN_TICK_INDEX..=MAX_TICK_INDEX).contains(&tick_index)
	}

	/// Check that the tick index is a valid start tick for a tick array in this amm
	/// A valid start-tick-index is a multiple of tick_spacing & number of ticks in a tick-array.
	///
	/// # Parameters
	/// - `tick_index` - A i32 integer representing the tick index
	/// - `tick_spacing` - A u8 integer of the tick spacing for this amm
	///
	/// # Returns
	/// - `true`: The tick index is a valid start-tick-index for this amm
	/// - `false`: The tick index is not a valid start-tick-index for this amm
	///            or the tick index not within the range supported by this contract
	pub fn check_is_valid_start_tick(tick_index: i32, tick_spacing: u16) -> bool {
		let ticks_in_array = TICK_ARRAY_SIZE * (tick_spacing as i32);

		if Tick::check_is_out_of_bounds(tick_index) {
			// Left-edge tick-array can have a start-tick-index smaller than the min tick index
			if tick_index > MIN_TICK_INDEX {
				return false;
			}

			let min_array_start_index =
				MIN_TICK_INDEX - ((MIN_TICK_INDEX % ticks_in_array) + ticks_in_array);
			return tick_index == min_array_start_index;
		}
		tick_index % ticks_in_array == 0
	}

	/// Check that the tick index is within bounds and is a usable tick index for the given tick spacing.
	///
	/// # Parameters
	/// - `tick_index` - A i32 integer representing the tick index
	/// - `tick_spacing` - A u8 integer of the tick spacing for this amm
	///
	/// # Returns
	/// - `true`: The tick index is within max/min index bounds for this protocol and is a usable tick-index given the tick-spacing
	/// - `false`: The tick index is out of bounds or is not a usable tick for this tick-spacing
	pub fn check_is_usable_tick(tick_index: i32, tick_spacing: u16) -> bool {
		if Tick::check_is_out_of_bounds(tick_index) {
			return false;
		}

		tick_index % (tick_spacing as i32) == 0
	}

	pub fn full_range_indexes(tick_spacing: u16) -> (i32, i32) {
		let lower_index =
			(MIN_TICK_INDEX / (tick_spacing as i32)) * (tick_spacing as i32);
		let upper_index =
			(MAX_TICK_INDEX / (tick_spacing as i32)) * (tick_spacing as i32);
		(lower_index, upper_index)
	}

	/// Bound a tick-index value to the max & min index value for this protocol
	///
	/// # Parameters
	/// - `tick_index` - A i32 integer representing the tick index
	///
	/// # Returns
	/// - `i32` The input tick index value but bounded by the max/min value of this protocol.
	pub fn bound_tick_index(tick_index: i32) -> i32 {
		tick_index.clamp(MIN_TICK_INDEX, MAX_TICK_INDEX)
	}
}

#[derive(Default, Debug, PartialEq)]
pub struct TickUpdate {
	pub initialized: bool,
	pub liquidity_net: i128,
	pub liquidity_gross: u128,
	pub fee_growth_outside_a: u128,
	pub fee_growth_outside_b: u128,
	pub reward_growths_outside: [u128; NUM_REWARDS],
}

impl TickUpdate {
	pub fn from(tick: &Tick) -> TickUpdate {
		TickUpdate {
			initialized: tick.initialized,
			liquidity_net: tick.liquidity_net,
			liquidity_gross: tick.liquidity_gross,
			fee_growth_outside_a: tick.fee_growth_outside_a,
			fee_growth_outside_b: tick.fee_growth_outside_b,
			reward_growths_outside: tick.reward_growths_outside,
		}
	}
}

pub trait TickArrayType {
	fn start_tick_index(&self) -> i32;

	fn get_next_init_tick_index(
		&self,
		tick_index: i32,
		tick_spacing: u16,
		a_to_b: bool
	) -> Result<Option<i32>>;

	fn get_tick(&self, tick_index: i32, tick_spacing: u16) -> Result<&Tick>;

	fn update_tick(
		&mut self,
		tick_index: i32,
		tick_spacing: u16,
		update: &TickUpdate
	) -> Result<()>;

	/// Checks that this array holds the next tick index for the current tick index, given the pool's tick spacing & search direction.
	///
	/// unshifted checks on [start, start + TICK_ARRAY_SIZE * tick_spacing)
	/// shifted checks on [start - tick_spacing, start + (TICK_ARRAY_SIZE - 1) * tick_spacing) (adjusting range by -tick_spacing)
	///
	/// shifted == !a_to_b
	///
	/// For a_to_b swaps, price moves left. All searchable ticks in this tick-array's range will end up in this tick's usable ticks.
	/// The search range is therefore the range of the tick-array.
	///
	/// For b_to_a swaps, this tick-array's left-most ticks can be the 'next' usable tick-index of the previous tick-array.
	/// The right-most ticks also points towards the next tick-array. The search range is therefore shifted by 1 tick-spacing.
	fn in_search_range(
		&self,
		tick_index: i32,
		tick_spacing: u16,
		shifted: bool
	) -> bool {
		let mut lower = self.start_tick_index();
		let mut upper =
			self.start_tick_index() + TICK_ARRAY_SIZE * (tick_spacing as i32);
		if shifted {
			lower -= tick_spacing as i32;
			upper -= tick_spacing as i32;
		}
		tick_index >= lower && tick_index < upper
	}

	fn check_in_array_bounds(&self, tick_index: i32, tick_spacing: u16) -> bool {
		self.in_search_range(tick_index, tick_spacing, false)
	}

	fn is_min_tick_array(&self) -> bool {
		self.start_tick_index() <= MIN_TICK_INDEX
	}

	fn is_max_tick_array(&self, tick_spacing: u16) -> bool {
		self.start_tick_index() + TICK_ARRAY_SIZE * (tick_spacing as i32) >
			MAX_TICK_INDEX
	}

	fn tick_offset(&self, tick_index: i32, tick_spacing: u16) -> Result<isize> {
		if tick_spacing == 0 {
			return Err(ErrorCode::InvalidTickSpacing.into());
		}

		Ok(get_offset(tick_index, self.start_tick_index(), tick_spacing))
	}
}

fn get_offset(
	tick_index: i32,
	start_tick_index: i32,
	tick_spacing: u16
) -> isize {
	// TODO: replace with i32.div_floor once not experimental
	let lhs = tick_index - start_tick_index;
	let rhs = tick_spacing as i32;
	let d = lhs / rhs;
	let r = lhs % rhs;
	let o = if (r > 0 && rhs < 0) || (r < 0 && rhs > 0) { d - 1 } else { d };
	o as isize
}

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
pub struct TickArray {
	pub start_tick_index: i32,
	pub ticks: [Tick; TICK_ARRAY_SIZE_USIZE],
	pub market: Pubkey,
}

impl Default for TickArray {
	#[inline]
	fn default() -> TickArray {
		TickArray {
			market: Pubkey::default(),
			ticks: [Tick::default(); TICK_ARRAY_SIZE_USIZE],
			start_tick_index: 0,
		}
	}
}

impl TickArray {
	pub const LEN: usize = 8 + 36 + Tick::LEN * TICK_ARRAY_SIZE_USIZE;
}

impl TickArrayType for TickArray {
	fn start_tick_index(&self) -> i32 {
		self.start_tick_index
	}

	/// Search for the next initialized tick in this array.
	///
	/// # Parameters
	/// - `tick_index` - A i32 integer representing the tick index to start searching for
	/// - `tick_spacing` - A u8 integer of the tick spacing for this amm
	/// - `a_to_b` - If the trade is from a_to_b, the search will move to the left and the starting search tick is inclusive.
	///              If the trade is from b_to_a, the search will move to the right and the starting search tick is not inclusive.
	///
	/// # Returns
	/// - `Some(i32)`: The next initialized tick index of this array
	/// - `None`: An initialized tick index was not found in this array
	/// - `InvalidTickArraySequence` - error if `tick_index` is not a valid search tick for the array
	/// - `InvalidTickSpacing` - error if the provided tick spacing is 0
	fn get_next_init_tick_index(
		&self,
		tick_index: i32,
		tick_spacing: u16,
		a_to_b: bool
	) -> Result<Option<i32>> {
		if !self.in_search_range(tick_index, tick_spacing, !a_to_b) {
			return Err(ErrorCode::InvalidTickArraySequence.into());
		}

		let mut curr_offset = match self.tick_offset(tick_index, tick_spacing) {
			Ok(value) => value as i32,
			Err(e) => {
				return Err(e);
			}
		};

		// For a_to_b searches, the search moves to the left. The next possible init-tick can be the 1st tick in the current offset
		// For b_to_a searches, the search moves to the right. The next possible init-tick cannot be within the current offset
		if !a_to_b {
			curr_offset += 1;
		}

		while (0..TICK_ARRAY_SIZE).contains(&curr_offset) {
			let curr_tick = self.ticks[curr_offset as usize];
			if curr_tick.initialized {
				return Ok(
					Some(curr_offset * (tick_spacing as i32) + self.start_tick_index)
				);
			}

			curr_offset = if a_to_b {
				curr_offset - 1
			} else {
				curr_offset + 1
			};
		}

		Ok(None)
	}

	/// Get the Tick object at the given tick-index & tick-spacing
	///
	/// # Parameters
	/// - `tick_index` - the tick index the desired Tick object is stored in
	/// - `tick_spacing` - A u8 integer of the tick spacing for this amm
	///
	/// # Returns
	/// - `&Tick`: A reference to the desired Tick object
	/// - `TickNotFound`: - The provided tick-index is not an initializable tick index in this amm w/ this tick-spacing.
	fn get_tick(&self, tick_index: i32, tick_spacing: u16) -> Result<&Tick> {
		if
			!self.check_in_array_bounds(tick_index, tick_spacing) ||
			!Tick::check_is_usable_tick(tick_index, tick_spacing)
		{
			return Err(ErrorCode::TickNotFound.into());
		}
		let offset = self.tick_offset(tick_index, tick_spacing)?;
		if offset < 0 {
			return Err(ErrorCode::TickNotFound.into());
		}
		Ok(&self.ticks[offset as usize])
	}

	/// Updates the Tick object at the given tick-index & tick-spacing
	///
	/// # Parameters
	/// - `tick_index` - the tick index the desired Tick object is stored in
	/// - `tick_spacing` - A u8 integer of the tick spacing for this amm
	/// - `update` - A reference to a TickUpdate object to update the Tick object at the given index
	///
	/// # Errors
	/// - `TickNotFound`: - The provided tick-index is not an initializable tick index in this amm w/ this tick-spacing.
	fn update_tick(
		&mut self,
		tick_index: i32,
		tick_spacing: u16,
		update: &TickUpdate
	) -> Result<()> {
		if
			!self.check_in_array_bounds(tick_index, tick_spacing) ||
			!Tick::check_is_usable_tick(tick_index, tick_spacing)
		{
			return Err(ErrorCode::TickNotFound.into());
		}
		let offset = self.tick_offset(tick_index, tick_spacing)?;
		if offset < 0 {
			return Err(ErrorCode::TickNotFound.into());
		}
		self.ticks
			.get_mut(offset as usize)
			.unwrap()
			.update(update);
		Ok(())
	}
}

pub(crate) struct ZeroedTickArray {
	pub start_tick_index: i32,
	zeroed_tick: Tick,
}

impl ZeroedTickArray {
	pub fn new(start_tick_index: i32) -> Self {
		ZeroedTickArray {
			start_tick_index,
			zeroed_tick: Tick::default(),
		}
	}
}

impl TickArrayType for ZeroedTickArray {
	fn start_tick_index(&self) -> i32 {
		self.start_tick_index
	}

	fn get_next_init_tick_index(
		&self,
		tick_index: i32,
		tick_spacing: u16,
		a_to_b: bool
	) -> Result<Option<i32>> {
		if !self.in_search_range(tick_index, tick_spacing, !a_to_b) {
			return Err(ErrorCode::InvalidTickArraySequence.into());
		}

		self.tick_offset(tick_index, tick_spacing)?;

		// no initialized tick
		Ok(None)
	}

	fn get_tick(&self, tick_index: i32, tick_spacing: u16) -> Result<&Tick> {
		if
			!self.check_in_array_bounds(tick_index, tick_spacing) ||
			!Tick::check_is_usable_tick(tick_index, tick_spacing)
		{
			return Err(ErrorCode::TickNotFound.into());
		}
		let offset = self.tick_offset(tick_index, tick_spacing)?;
		if offset < 0 {
			return Err(ErrorCode::TickNotFound.into());
		}

		// always return the zeroed tick
		Ok(&self.zeroed_tick)
	}

	fn update_tick(
		&mut self,
		_tick_index: i32,
		_tick_spacing: u16,
		_update: &TickUpdate
	) -> Result<()> {
		panic!("ZeroedTickArray must not be updated");
	}
}
