use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use anchor_lang::solana_program::pubkey;

// Code ka Unique ID (Program ID). Ye deploy karne ke baad milta hai.
declare_id!("9vF8iR37L3nKtBR4x6mhy8dE8eMLUzcuCNbSCGCpnYHG");

// Admin Key (Deployer): Sirf ye wallet hi 'initialize' call kar sakta hai.
// Ye hardcoded address hai, sirf yahi banda initially setup kar payega.
const ADMIN_PUBKEY: Pubkey = pubkey!("HfLwDVax4RaftkctDGGw5a84jheVZtSint919Xy9D3dD");

#[program]
pub mod staking_contract {
    use super::*;

    // Fee Constant REMOVED. Ab dynamic GlobalConfig use hoga.
    // const WITHDRAW_FEE_BPS: u64 = 100;

    // Initialize Function: Staking vault aur Config banane ke liye.
    // Sirf Admin call kar sakta hai.
    pub fn initialize(ctx: Context<Initialize>, initial_fee_bps: u64) -> anchor_lang::Result<()> {
        // 1. Check karo ki call karne wala ADMIN hi hai na?
        require_keys_eq!(ctx.accounts.payer.key(), ADMIN_PUBKEY, ErrorCode::Unauthorized);

        // Global Config Setup
        let config = &mut ctx.accounts.config;
        config.admin = ADMIN_PUBKEY;
        config.withdraw_fee_bps = initial_fee_bps;

        msg!("Staking Vault & Config Initialized! Initial Fee: {} bps", initial_fee_bps);
        Ok(())
    }

    // Update Fee Function: Admin kabhi bhi fee change kar sakta hai.
    pub fn update_fee(ctx: Context<UpdateFee>, new_fee_bps: u64) -> anchor_lang::Result<()> {
        let config = &mut ctx.accounts.config;

        // Validation: Admin check struct me hi ho raha hai (has_one = admin)

        let old_fee = config.withdraw_fee_bps;
        config.withdraw_fee_bps = new_fee_bps;

        emit!(FeeUpdated {
            old_fee,
            new_fee: new_fee_bps
        });

        msg!("Fee updated from {} to {}", old_fee, new_fee_bps);
        Ok(())
    }

    // Deposit Function: User apne tokens stake (jama) karne ke liye call karega.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> anchor_lang::Result<()> {
        // 1. Check karo ki amount 0 se jyada honi chahiye.
        require!(amount > 0, ErrorCode::InvalidAmount);

        let staker = &mut ctx.accounts.staker;
        let stake_info = &mut ctx.accounts.stake_info;

        // 2. Token Transfer Logic (User -> Vault)
        // Ye instruction banata hai ki user ke account se vault me paise bhejo.
        let transfer_instruction = token::Transfer {
            from: ctx.accounts.staker_token_account.to_account_info(), // Kahan se nikale (User)
            to: ctx.accounts.vault.to_account_info(),                  // Kahan dale (Vault)
            authority: staker.to_account_info(),                       // Permission kiski (User)
        };

        // CPI (Cross Program Invocation) Context banaya Token Program ke liye.
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_instruction,
        );

        // Asli transfer yahan execute hota hai using Anchor's token helper.
        token::transfer(cpi_ctx, amount)?;

        // 3. Update User Record (User ka khata update karo)
        // Stake info account me likho ki kitna jama kiya aur kab kiya.
        stake_info.amount += amount; // Amount add kar diya
        stake_info.deposit_ts = Clock::get()?.unix_timestamp; // Abhi ka time store kiya

        // 4. Emit Event (Log generate karo taaki frontend ko pata chale)
        emit!(TokensStaked {
            staker: staker.key(),
            amount,
            total_staked: stake_info.amount,
        });

        msg!("Staked {} tokens successfully. Total: {}", amount, stake_info.amount);
        Ok(())
    }

    // Withdraw Function: User apne tokens wapis nikalne ke liye call karega.
    pub fn withdraw(ctx: Context<Withdraw>) -> anchor_lang::Result<()> {
        let stake_info = &mut ctx.accounts.stake_info;
        let staker = &mut ctx.accounts.staker;

        // 1. Check Balance (Khate me paisa hai bhi ya nahi?)
        // Agar balance 0 hai to error feko.
        require!(stake_info.amount > 0, ErrorCode::InvalidWithdraw);

        let total_amount = stake_info.amount; // Sara paisa nikalna hai

        // Dynamic Fee Calculation
        // Config se current fee rate padho
        let fee_bps = ctx.accounts.config.withdraw_fee_bps;
        let fee_amount = (total_amount * fee_bps) / 10000;
        let user_amount = total_amount - fee_amount;

        let bump = ctx.bumps.vault;                 // PDA ka bump seed
        let mint_key = ctx.accounts.mint.key();     // Token ka mint address

        // PDA Seeds for Signing (Vault khud sign karega)
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"vault",
            mint_key.as_ref(),
            &[bump]
        ]];

        // 2a. Transfer Fee (Vault -> Fee Vault)
        if fee_amount > 0 {
            let fee_transfer_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.fee_vault.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                signer_seeds
            );
            token::transfer(fee_transfer_ctx, fee_amount)?;
        }

        // 2b. Transfer Remaining Tokens (Vault -> User)
        let user_transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.staker_token_account.to_account_info(),
                authority: ctx.accounts.vault.to_account_info(),
            },
            signer_seeds // Ye seeds proof hain ki program hi owner hai
        );
        token::transfer(user_transfer_ctx, user_amount)?;

        // 3. Reset User Ledger (User ka khata nil karo)
        stake_info.amount = 0;

        // 4. Emit Event (Log)
        emit!(TokensWithdrawn {
            staker: staker.key(),
            amount: user_amount,
            fee: fee_amount,
            total_staked: 0,
        });

        msg!("Withdrawn {} tokens. Fee deducted: {}. Total Reset.", user_amount, fee_amount);
        Ok(())
    }
}

// ----------------- ERRORS -----------------
// Custom Errors jo humne banaye hain
#[error_code]
pub enum ErrorCode {
    #[msg("You are not authorized to perform this action.")]
    Unauthorized, // Agar koi galat banda admin function call kare
    #[msg("Amount must be greater than zero.")]
    InvalidAmount, // Agar 0 ya negative deposit karne ki koshish kare
    #[msg("No tokens to withdraw.")]
    InvalidWithdraw, // Agar khali account se withdraw kare
    #[msg("Fee vault must be owned by admin.")]
    InvalidFeeVault, // SECURITY FIX: Fee vault must belong to admin - prevents fee theft
}

// ----------------- STRUCTS (Data Validation) -----------------

// Initialize ke liye validation logic
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,                               // Naya account banao
        payer = payer,                      // Paise 'payer' dega is account ko banane ke
        seeds = [b"vault", mint.key().as_ref()], // Ye account ek PDA hai (Address deterministic hai)
        bump,                               // Bump seed collision bachane ke liye
        token::mint = mint,                 // Ye account kis token ko hold karega
        token::authority = vault,           // Iska owner ye khud (Vault PDA) hoga
    )]
    pub vault: Account<'info, TokenAccount>, // Ye wo account hai jahan sabka paisa store hoga

    pub mint: Account<'info, Mint>,          // Token ka main address (e.g., USDC ka address)

    #[account(mut)]
    pub payer: Signer<'info>,                // Jo fees pay karega (Admin)

    #[account(
        init,
        payer = payer,
        space = 8 + 32 + 8, // Discriminator + Pubkey + u64
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, GlobalConfig>, // Global Config Account

    pub system_program: Program<'info, System>, // Solana system program (account creation ke liye zaroori)
    pub token_program: Program<'info, Token>,   // SPL Token program (token transfer ke liye zaroori)
    pub rent: Sysvar<'info, Rent>,              // Rent sysvar (rent calculation ke liye)
}

// Update Fee Validation
#[derive(Accounts)]
pub struct UpdateFee<'info> {
    #[account(
        mut,
        seeds = [b"config"],
        bump,
        has_one = admin, // Confirm karo ki signer hi admin hai
    )]
    pub config: Account<'info, GlobalConfig>,

    pub admin: Signer<'info>, // Sirf admin hi call kar sakta hai
}

// Deposit ke liye validation logic
#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub staker: Signer<'info>, // Jo banda deposit kar raha hai (sign karega)

    #[account(
        mut,
        seeds = [b"vault", mint.key().as_ref()], // Wahi vault dhoondo jo initialize hua tha
        bump,
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,                     // Agar user pehli baar aaya hai to account banao check karke
        payer = staker,                     // Fees staker dega
        space = 8 + 8 + 8,                  // Kitni jagah chahiye RAM me (Discriminator + u64 + i64)
        seeds = [b"user", staker.key().as_ref()], // Har user ka alag PDA hoga uske wallet address ke base pe
        bump
    )]
    pub stake_info: Account<'info, UserStakeInfo>, // Ye user ka personal ledger hai

    pub mint: Account<'info, Mint>, // Token Mint

    #[account(mut)]
    pub staker_token_account: Account<'info, TokenAccount>, // User ka token wallet jahan se paise katenge

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// Withdraw ke liye validation logic
#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub staker: Signer<'info>, // Jo banda withdraw maang raha hai

    #[account(
        mut,
        seeds = [b"vault", mint.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, TokenAccount>, // Vault se paise nikalenge

    #[account(
        mut, // Modify karenge kyunki balance 0 karna hai
        seeds = [b"user", staker.key().as_ref()],
        bump
    )]
    pub stake_info: Account<'info, UserStakeInfo>, // User ka ledger check karenge

    pub mint: Account<'info, Mint>,

    #[account(mut)]
    pub staker_token_account: Account<'info, TokenAccount>, // Jahan paisa wapis aayega user ke paas

    #[account(
        mut,
        constraint = fee_vault.owner == config.admin @ ErrorCode::InvalidFeeVault // CRITICAL FIX: Ensure fee_vault belongs to admin
    )]
    pub fee_vault: Account<'info, TokenAccount>, // Admin ka account jahan fee jayegi - NOW VALIDATED

    #[account(
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, GlobalConfig>, // Fee rate janne ke liye config chahiye

    pub token_program: Program<'info, Token>,
}


// ----------------- DATA ACCOUNTS -----------------
// On-chain data storage structure specific to our program

#[account]
pub struct UserStakeInfo {
    pub amount: u64,        // Kitna paisa jama hai (8 bytes)
    pub deposit_ts: i64,    // Kab jama kiya (Timestamp) (8 bytes)
}

#[account]
pub struct GlobalConfig {
    pub admin: Pubkey,       // Admin kaun hai
    pub withdraw_fee_bps: u64, // Current Fee (Basis Points)
}

// ----------------- EVENTS -----------------
// Ye logs hain jo frontend catch kar sakta hai bina chain state padhe

#[event]
pub struct TokensStaked {
    pub staker: Pubkey,
    pub amount: u64,
    pub total_staked: u64,
}

#[event]
pub struct TokensWithdrawn {
    pub staker: Pubkey,
    pub amount: u64,
    pub fee: u64,
    pub total_staked: u64,
}

#[event]
pub struct FeeUpdated {
    pub old_fee: u64,
    pub new_fee: u64,
}
