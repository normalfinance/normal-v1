use crate::error::ErrorCode;
use crate::signer::get_signer_seeds;
use crate::validate;
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::extension::transfer_fee::TransferFeeConfig;
use anchor_spl::token_2022::spl_token_2022::extension::{
    BaseStateWithExtensions,
    StateWithExtensions,
};
use anchor_spl::token_2022::spl_token_2022::state::Mint as MintInner;
use anchor_spl::token_interface::{
    self,
    InitializeAccount,
    CloseAccount,
    InitializeMint,
    Mint,
    MintTo,
    Burn,
    TokenAccount,
    TokenInterface,
    Transfer,
    TransferChecked,
};

pub fn create_synthetic_token<'info>(
    token_program: &Interface<'info, TokenInterface>,
    authority: &AccountInfo<'info>,
    nonce: u8,
    amount: u64
) -> Result<()> {
    // ...

    // // 1. Create the token mint account (pay rent and create account)
    // let rent = Rent::get()?.minimum_balance(Mint::LEN);
    // let create_account_ix = system_instruction::create_account(
    //     payer_account.key, // Payer's public key
    //     market.amm.token_synth_mint.key, // New mint account's public key
    //     rent, // Rent-exempt balance
    //     Mint::LEN as u64, // Space required for the mint account
    //     &spl_token::id() // SPL Token program ID
    // );

    // invoke(&create_account_ix, &[payer_account.clone(), market.amm.token_synth_mint.clone()])?;

    // // 2. Initialize the mint account
    // let initialize_mint_ix = initialize_mint(
    //     &spl_token::id(), // SPL Token program ID
    //     market.amm.token_synth_mint.key, // Mint account public key
    //     mint_authority_account.key, // Mint authority public key
    //     None, // Freeze authority (optional)
    //     decimals // Number of decimals for the token
    // )?;

    // invoke(
    //     &initialize_mint_ix,
    //     &[
    //         mint_accmarket.amm.token_synth_mintount.clone(),
    //         mint_authority_account.clone(),
    //         payer_account.clone(),
    //     ]
    // )?;
}

pub fn mint_synthetic_tokens<'info>(
    token_program: &Interface<'info, TokenInterface>,
    to: &InterfaceAccount<'info, TokenAccount>,
    authority: &AccountInfo<'info>,
    amount: u64,
    mint: &InterfaceAccount<'info, Mint>
) -> Result<()> {
    let mint_account_info = mint.to_account_info();

    validate_mint_fee(&mint_account_info)?;

    let cpi_accounts = MintTo {
        to: to.to_account_info(),
        mint: mint_account_info,
        authority: authority.to_account_info(),
    };
    let cpi_program = token_program.to_account_info();
    let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
    token_interface::mint_to(cpi_context, amount)
}

pub fn burn_synthetic_tokens<'info>(
    token_program: &Interface<'info, TokenInterface>,
    from: &InterfaceAccount<'info, TokenAccount>,
    authority: &AccountInfo<'info>,
    nonce: u8,
    amount: u64,
    mint: &InterfaceAccount<'info, Mint>
) -> Result<()> {
    let mint_account_info = mint.to_account_info();

    validate_mint_fee(&mint_account_info)?;

    let cpi_accounts = Burn {
        from: from.to_account_info(),
        mint: mint_account_info,
        authority: authority.to_account_info(),
    };
    let cpi_program = token_program.to_account_info();
    let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
    token_interface::burn(cpi_context, amount)
}

pub fn send_from_program_vault<'info>(
    token_program: &Interface<'info, TokenInterface>,
    from: &InterfaceAccount<'info, TokenAccount>,
    to: &InterfaceAccount<'info, TokenAccount>,
    authority: &AccountInfo<'info>,
    nonce: u8,
    amount: u64,
    mint: &Option<InterfaceAccount<'info, Mint>>
) -> Result<()> {
    let signature_seeds = get_signer_seeds(&nonce);
    let signers = &[&signature_seeds[..]];

    if let Some(mint) = mint {
        let mint_account_info = mint.to_account_info();

        validate_mint_fee(&mint_account_info)?;

        let cpi_accounts = TransferChecked {
            from: from.to_account_info(),
            mint: mint_account_info,
            to: to.to_account_info(),
            authority: authority.to_account_info(),
        };

        let cpi_program = token_program.to_account_info();
        let cpi_context = CpiContext::new_with_signer(cpi_program, cpi_accounts, signers);
        token_interface::transfer_checked(cpi_context, amount, mint.decimals)
    } else {
        let cpi_accounts = Transfer {
            from: from.to_account_info(),
            to: to.to_account_info(),
            authority: authority.to_account_info(),
        };

        let cpi_program = token_program.to_account_info();
		let cpi_context = CpiContext::new_with_signer(
			cpi_program,
			cpi_accounts,
			signers
		);
        #[allow(deprecated)]
        token_interface::transfer(cpi_context, amount)
    }
}

pub fn receive<'info>(
    token_program: &Interface<'info, TokenInterface>,
    from: &InterfaceAccount<'info, TokenAccount>,
    to: &InterfaceAccount<'info, TokenAccount>,
    authority: &AccountInfo<'info>,
    amount: u64,
    mint: &Option<InterfaceAccount<'info, Mint>>
) -> Result<()> {
    if let Some(mint) = mint {
        let mint_account_info = mint.to_account_info();

        validate_mint_fee(&mint_account_info)?;

        let cpi_accounts = TransferChecked {
            from: from.to_account_info(),
            to: to.to_account_info(),
            mint: mint_account_info,
            authority: authority.to_account_info(),
        };
        let cpi_program = token_program.to_account_info();
        let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
        token_interface::transfer_checked(cpi_context, amount, mint.decimals)
    } else {
        let cpi_accounts = Transfer {
            from: from.to_account_info(),
            to: to.to_account_info(),
            authority: authority.to_account_info(),
        };
        let cpi_program = token_program.to_account_info();
        let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
        #[allow(deprecated)]
        token_interface::transfer(cpi_context, amount)
    }
}

pub fn close_vault<'info>(
    token_program: &Interface<'info, TokenInterface>,
    account: &InterfaceAccount<'info, TokenAccount>,
    destination: &AccountInfo<'info>,
    authority: &AccountInfo<'info>,
    nonce: u8
) -> Result<()> {
    let signature_seeds = get_signer_seeds(&nonce);
    let signers = &[&signature_seeds[..]];
    let cpi_accounts = CloseAccount {
        account: account.to_account_info(),
        destination: destination.clone(),
        authority: authority.to_account_info(),
    };
    let cpi_program = token_program.to_account_info();
	let cpi_context = CpiContext::new_with_signer(
		cpi_program,
		cpi_accounts,
		signers
	);
    token_interface::close_account(cpi_context)
}

pub fn validate_mint_fee(account_info: &AccountInfo) -> Result<()> {
    let mint_data = account_info.try_borrow_data()?;
	let mint_with_extension = StateWithExtensions::<MintInner>::unpack(
		&mint_data
	)?;
	if
		let Ok(fee_config) =
			mint_with_extension.get_extension::<TransferFeeConfig>()
	{
        let fee = u16::from(
            fee_config.get_epoch_fee(Clock::get()?.epoch).transfer_fee_basis_points
        );
        validate!(fee == 0, ErrorCode::NonZeroTransferFee)?;
    }

    Ok(())
}
