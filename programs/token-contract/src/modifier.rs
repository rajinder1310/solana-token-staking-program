use anchor_lang::prelude::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod production_access_control {
    use super::*;

    // 1. Initialize Global Config (Run once on deploy)
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        config.admin = ctx.accounts.admin.key();
        config.bump = ctx.bumps.config;
        msg!("Config initialized with Admin: {:?}", config.admin);
        Ok(())
    }

    // 2. Admin Only Action (Secure)
    pub fn restricted_function(ctx: Context<AdminOnly>) -> Result<()> {
        msg!("Welcome Admin! You are authorized to execute this.");
        Ok(())
    }

    // 3. Rotate Keys (Production Feature)
    // Allows current admin to set a new admin wallet.
    pub fn transfer_ownership(ctx: Context<TransferOwnership>, new_admin: Pubkey) -> Result<()> {
        let config = &mut ctx.accounts.config;
        let old_admin = config.admin;
        config.admin = new_admin;
        msg!("Ownership transferred from {:?} to {:?}", old_admin, new_admin);
        Ok(())
    }
}

// --------------------------------------------------------
// State Structs (The "Storage")
// --------------------------------------------------------
#[account]
pub struct Config {
    pub admin: Pubkey, // Dynamic Storage for Admin Address
    pub bump: u8,
}

// --------------------------------------------------------
// Validation Contexts (The "Gatekeepers")
// --------------------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + 32 + 1, // Discriminator + Pubkey + Bump
        seeds = [b"access_config"],
        bump
    )]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    #[account(
        seeds = [b"access_config"],
        bump = config.bump,
        has_one = admin @ AccessError::Unauthorized // ✅ Automagic Check: config.admin must match signer
    )]
    pub config: Account<'info, Config>,

    pub admin: Signer<'info>, // The signer trying to call the function
}

#[derive(Accounts)]
pub struct TransferOwnership<'info> {
    #[account(
        mut,
        seeds = [b"access_config"],
        bump = config.bump,
        has_one = admin @ AccessError::Unauthorized
    )]
    pub config: Account<'info, Config>,

    pub admin: Signer<'info>, // Current Admin
}

// --------------------------------------------------------
// Errors
// --------------------------------------------------------
#[error_code]
pub enum AccessError {
    #[msg("You are not authorized to perform this action.")]
    Unauthorized,
}
