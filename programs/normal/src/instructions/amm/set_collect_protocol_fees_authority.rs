use anchor_lang::prelude::*;

use crate::state::AMMsConfig;

#[derive(Accounts)]
pub struct SetCollectProtocolFeesAuthority<'info> {
    #[account(mut)]
    pub amms_config: Account<'info, AMMsConfig>,

    #[account(address = amms_config.collect_protocol_fees_authority)]
    pub collect_protocol_fees_authority: Signer<'info>,

    /// CHECK: safe, the account that will be new authority can be arbitrary
    pub new_collect_protocol_fees_authority: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<SetCollectProtocolFeesAuthority>) -> Result<()> {
    ctx.accounts
        .amms_config
        .update_collect_protocol_fees_authority(
            ctx.accounts.new_collect_protocol_fees_authority.key(),
        );
    Ok(())
}
