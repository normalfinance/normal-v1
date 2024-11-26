


pub fn handle_update_user_name(
    ctx: Context<UpdateUser>,
    _sub_account_id: u16,
    name: [u8; 32],
) -> Result<()> {
    let mut user = load_mut!(ctx.accounts.user)?;
    user.name = name;
    Ok(())
}


pub fn handle_update_user_delegate(
    ctx: Context<UpdateUser>,
    _sub_account_id: u16,
    delegate: Pubkey,
) -> Result<()> {
    let mut user = load_mut!(ctx.accounts.user)?;
    user.delegate = delegate;
    Ok(())
}


pub fn handle_update_user_reduce_only(
    ctx: Context<UpdateUser>,
    _sub_account_id: u16,
    reduce_only: bool,
) -> Result<()> {
    let mut user = load_mut!(ctx.accounts.user)?;

    validate!(!user.is_being_liquidated(), ErrorCode::LiquidationsOngoing)?;

    user.update_reduce_only_status(reduce_only)?;
    Ok(())
}

pub fn handle_update_user_advanced_lp(
    ctx: Context<UpdateUser>,
    _sub_account_id: u16,
    advanced_lp: bool,
) -> Result<()> {
    let mut user = load_mut!(ctx.accounts.user)?;

    validate!(!user.is_being_liquidated(), ErrorCode::LiquidationsOngoing)?;

    user.update_advanced_lp_status(advanced_lp)?;
    Ok(())
}

pub fn handle_delete_user(ctx: Context<DeleteUser>) -> Result<()> {
    let user = &load!(ctx.accounts.user)?;
    let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;

    validate_user_deletion(
        user,
        user_stats,
        &ctx.accounts.state,
        Clock::get()?.unix_timestamp,
    )?;

    safe_decrement!(user_stats.number_of_sub_accounts, 1);

    let state = &mut ctx.accounts.state;
    safe_decrement!(state.number_of_sub_accounts, 1);

    Ok(())
}

pub fn handle_reclaim_rent(ctx: Context<ReclaimRent>) -> Result<()> {
    let user_size = ctx.accounts.user.to_account_info().data_len();
    let minimum_lamports = ctx.accounts.rent.minimum_balance(user_size);
    let current_lamports = ctx.accounts.user.to_account_info().try_lamports()?;
    let reclaim_amount = current_lamports.saturating_sub(minimum_lamports);

    validate!(
        reclaim_amount > 0,
        ErrorCode::CantReclaimRent,
        "user account has no excess lamports to reclaim"
    )?;

    **ctx
        .accounts
        .user
        .to_account_info()
        .try_borrow_mut_lamports()? = minimum_lamports;

    **ctx
        .accounts
        .authority
        .to_account_info()
        .try_borrow_mut_lamports()? += reclaim_amount;

    let user_stats = &mut load!(ctx.accounts.user_stats)?;

    // Skip age check if is no max sub accounts
    let max_sub_accounts = ctx.accounts.state.max_number_of_sub_accounts();
    let estimated_user_stats_age = user_stats.get_age_ts(Clock::get()?.unix_timestamp);
    validate!(
        max_sub_accounts == 0 || estimated_user_stats_age >= THIRTEEN_DAY,
        ErrorCode::CantReclaimRent,
        "user stats too young to reclaim rent. age ={} minimum = {}",
        estimated_user_stats_age,
        THIRTEEN_DAY
    )?;

    Ok(())
}