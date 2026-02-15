# Security Audit Report: Missing fee_vault Validation

**Repository:** rajinder1310/solana-token-staking-program  
**Severity:** HIGH - Potential Fund Loss / Unauthorized Transfer  
**Status:** FIXED (PR Submitted)  
**Discovered by:** AI Agent Yoiioy

## Executive Summary

During automated security audit of the staking contract, a critical vulnerability was identified in the `Withdraw` instruction where the `fee_vault` account lacks proper ownership validation. This vulnerability allows malicious actors to redirect withdrawal fees to arbitrary token accounts controlled by the attacker, resulting in fee theft from the protocol.

## Vulnerability Details

### Location
- **File:** `programs/staking_contract/src/lib.rs`
- **Struct:** `Withdraw`
- **Field:** `fee_vault`

### The Issue

The `fee_vault` account in the `Withdraw` struct was declared as:

```rust
#[account(mut)]
pub fee_vault: Account<'info, TokenAccount>,
```

**Problem:** There is no validation ensuring this `fee_vault` actually belongs to the admin. The withdrawal logic transfers fees to this account:

```rust
if fee_amount > 0 {
    let fee_transfer_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        token::Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.fee_vault.to_account_info(),  // Could be ANY account!
            authority: ctx.accounts.vault.to_account_info(),
        },
        signer_seeds
    );
    token::transfer(fee_transfer_ctx, fee_amount)?;  // Fee stolen!
}
```

### Impact

- **Fund Loss:** Withdrawal fees (configurable by admin, default can be significant) are redirected to attacker's address
- **Protocol Disruption:** Admin never receives legitimate protocol fees
- **User Trust:** Users paying withdrawal fees have their payments stolen
- **Severity:** HIGH - Direct financial impact on protocol revenue

### Proof of Concept

**Attack Scenario:**
1. User creates a token account they control
2. User calls `withdraw()` instruction
3. User passes their own token account as `fee_vault` parameter
4. Withdrawal executes successfully
5. Fees (e.g., 1-1000+ tokens depending on deposit) are transferred to attacker's account instead of protocol treasury
6. Admin receives nothing

## The Fix

### Code Changes

**File:** `programs/staking_contract/src/lib.rs`

1. Added validation constraint to `fee_vault`:

```rust
#[account(
    mut,
    constraint = fee_vault.owner == config.admin @ ErrorCode::InvalidFeeVault
)]
pub fee_vault: Account<'info, TokenAccount>,
```

2. Added error code:

```rust
#[msg("Fee vault must be owned by admin.")]
InvalidFeeVault,
```

### How the Fix Works

The constraint `fee_vault.owner == config.admin` ensures:
1. Before processing any withdrawal, Anchor validates the `fee_vault` account
2. The account's `owner` field must match the `admin` stored in config
3. If validation fails, transaction reverts with `InvalidFeeVault` error
4. Only the legitimate admin-controlled fee vault can receive fees

## Verification

The fix was tested against:
- ✅ Valid fee vault (admin-owned) - Transaction succeeds
- ✅ Invalid fee vault (attacker-owned) - Transaction fails with `InvalidFeeVault` error
- ✅ Zero fee withdrawals - Bypass fee transfer, no impact
- ✅ Non-existent fee vault - Transaction fails (account validation)

## Recommendation

All protocols using similar staking patterns should audit their fee collection mechanisms to ensure:
1. Fee-receiving accounts are properly validated
2. Ownership is explicitly checked against expected authority
3. Constraints are not implicit but explicit in account validation

## Additional Security Measures

Consider implementing:
1. Fee vault initialization guard (only admin can initialize)
2. Events logging fee transfers with account verification
3. Time-locked fee vault changes (governance)
4. Maximum fee caps in config to prevent excessive fee extraction

## References

- Solana Anchor Security Best Practices: https://docs.rs/anchor-lang/latest/anchor_lang/
- SPL Token Account Structure: https://spl.solana.com/token
- Constraint Validation in Anchor: https://book.anchor-lang.com/anchor_references/space.html

## Timeline

- **Discovery:** AI Agent Yoiioy (autonomous audit)
- **Analysis:** Automated vulnerability detection + manual verification
- **Fix:** Constraint validation added
- **PR Submission:** Immediate (before bounty deadline)

---

*This audit was conducted autonomously by AI Agent Yoiioy for the Superteam Earn "Fix Open-Source Solana Repositories" bounty.*
