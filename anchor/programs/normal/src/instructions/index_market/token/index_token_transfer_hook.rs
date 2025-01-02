use anchor_lang::prelude::*;

// Order of accounts matters for this struct.
// The first 4 accounts are the accounts required for token transfer (source, mint, destination, owner)
// Remaining accounts are the extra accounts required from the ExtraAccountMetaList account
// These accounts are provided via CPI to this program from the token2022 program
#[derive(Accounts)]
pub struct IndexTokenTransferHook<'info> {
	#[account(token::mint = mint, token::authority = owner)]
	pub source_token: InterfaceAccount<'info, TokenAccount>,
	pub mint: InterfaceAccount<'info, Mint>,
	#[account(token::mint = mint)]
	pub destination_token: InterfaceAccount<'info, TokenAccount>,
	/// CHECK: source token account owner, can be SystemAccount or PDA owned by another program
	pub owner: UncheckedAccount<'info>,
	/// CHECK: ExtraAccountMetaList Account,
	#[account(seeds = [b"extra-account-metas", mint.key().as_ref()], bump)]
	pub extra_account_meta_list: UncheckedAccount<'info>,
	pub wsol_mint: InterfaceAccount<'info, Mint>,
	pub token_program: Interface<'info, TokenInterface>,
	pub associated_token_program: Program<'info, AssociatedToken>,
	#[account(
        mut,
        seeds = [b"delegate"], 
        bump
    )]
	pub delegate: SystemAccount<'info>,
	#[account(
        mut,
        token::mint = wsol_mint, 
        token::authority = delegate,
    )]
	pub delegate_wsol_token_account: InterfaceAccount<'info, TokenAccount>,
	#[account(
        mut,
        token::mint = wsol_mint, 
        token::authority = owner,
    )]
	pub sender_wsol_token_account: InterfaceAccount<'info, TokenAccount>,
}

pub fn handle_index_token_transfer_hook(
	ctx: Context<IndexTokenTransferHook>,
	amount: u64
) -> Result<()> {
	let signer_seeds: &[&[&[u8]]] = &[&[b"delegate", &[ctx.bumps.delegate]]];
	msg!("Transfer WSOL using delegate PDA");

	// transfer WSOL from sender to delegate token account using delegate PDA
	transfer_checked(
		CpiContext::new(
			ctx.accounts.token_program.to_account_info(),
			TransferChecked {
				from: ctx.accounts.sender_wsol_token_account.to_account_info(),
				mint: ctx.accounts.wsol_mint.to_account_info(),
				to: ctx.accounts.delegate_wsol_token_account.to_account_info(),
				authority: ctx.accounts.delegate.to_account_info(),
			}
		).with_signer(signer_seeds),
		amount,
		ctx.accounts.wsol_mint.decimals
	)?;
	Ok(())
}
