use crate::state::vault::Vault;

pub mod collateral;
pub mod delete_vault;
pub mod initialize_vault;
pub mod liquidate_vault;
pub mod set_vault_status_to_being_liquidated;
pub mod resolve_vault_bankruptcy;
pub mod update_vault_idle;
pub mod update_vault_delegate;

pub struct UpdateVault<'info> {
	#[account(
        mut,
        seeds = [b"vault", authority.key.as_ref()],
        bump,
    )]
	pub vault: AccountLoader<'info, Vault>,
	pub authority: Signer<'info>,
}
