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

    // Initialize Function: Staking vault banane ke liye.
    // Sirf Admin call kar sakta hai.
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        // 1. Check karo ki call karne wala ADMIN hi hai na?
        // Agar koi aur call karega to 'Unauthorized' error aayega.
        require_keys_eq!(ctx.accounts.payer.key(), ADMIN_PUBKEY, ErrorCode::Unauthorized);

        msg!("Staking Vault Initialized Successfully!");
        Ok(())
    }

    // Deposit Function: User apne tokens stake (jama) karne ke liye call karega.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
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
    pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
        let stake_info = &mut ctx.accounts.stake_info;
        let staker = &mut ctx.accounts.staker;

        // 1. Check Balance (Khate me paisa hai bhi ya nahi?)
        // Agar balance 0 hai to error feko.
        require!(stake_info.amount > 0, ErrorCode::InvalidWithdraw);

        let amount_to_withdraw = stake_info.amount; // Sara paisa nikalna hai
        let bump = ctx.bumps.vault;                 // PDA ka bump seed
        let mint_key = ctx.accounts.mint.key();     // Token ka mint address

        // PDA Seeds for Signing (Vault khud sign karega)
        // Kyunki Vault ek PDA (Program Derived Address) hai, uske paas private key nahi hoti.
        // Wo seeds use karke virtual signature generate- karta hai.
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"vault",
            mint_key.as_ref(),
            &[bump]
        ]];

        // 2. Transfer Tokens (Vault -> User)
        // Is baar Authority 'Vault PDA' hai, staker nahi.
        let transfer_instruction = token::Transfer {
            from: ctx.accounts.vault.to_account_info(),            // Kahan se nikale (Vault)
            to: ctx.accounts.staker_token_account.to_account_info(), // Kahan dale (User)
            authority: ctx.accounts.vault.to_account_info(),         // Authority Vault khud hai
        };

        // 'new_with_signer' use karte hain jab PDA se sign karwana ho.
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_instruction,
            signer_seeds // Ye seeds proof hain ki program hi owner hai
        );

        //
        //  execute karo
        token::transfer(cpi_ctx, amount_to_withdraw)?;

        // 3. Reset User Ledger (User ka khata nil karo)
        stake_info.amount = 0;

        // 4. Emit Event (Log)
        emit!(TokensWithdrawn {
            staker: staker.key(),
            amount: amount_to_withdraw,
            total_staked: 0,
        });

        msg!("Withdrawn {} tokens successfully. Account Reset.", amount_to_withdraw);
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

    pub system_program: Program<'info, System>, // Solana system program (account creation ke liye zaroori)
    pub token_program: Program<'info, Token>,   // SPL Token program (token transfer ke liye zaroori)
    pub rent: Sysvar<'info, Rent>,              // Rent sysvar (rent calculation ke liye)
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

    pub token_program: Program<'info, Token>,
}


// ----------------- DATA ACCOUNTS -----------------
// On-chain data storage structure specific to our program

#[account]
pub struct UserStakeInfo {
    pub amount: u64,        // Kitna paisa jama hai (8 bytes)
    pub deposit_ts: i64,    // Kab jama kiya (Timestamp) (8 bytes)
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
    pub total_staked: u64,
}
