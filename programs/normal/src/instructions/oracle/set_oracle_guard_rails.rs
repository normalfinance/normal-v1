pub fn handle_update_oracle_guard_rails(
	ctx: Context<AdminUpdateState>,
	oracle_guard_rails: OracleGuardRails
) -> Result<()> {
	msg!(
		"oracle_guard_rails: {:?} -> {:?}",
		ctx.accounts.state.oracle_guard_rails,
		oracle_guard_rails
	);

	ctx.accounts.state.oracle_guard_rails = oracle_guard_rails;
	Ok(())
}
