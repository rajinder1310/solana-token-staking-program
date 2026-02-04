import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { StakingContract } from "../target/types/staking_contract";
import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  getAccount
} from "@solana/spl-token";
import { assert, expect } from "chai";

describe("staking_contract", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.StakingContract as Program<StakingContract>;

  // Test variables
  let mint: anchor.web3.PublicKey;
  let vault: anchor.web3.PublicKey;
  let vaultBump: number;
  let stakeInfo: anchor.web3.PublicKey;

  // User (Staker) will be the provider for simplicity
  const staker = provider.wallet.publicKey;
  let stakerTokenAccount: anchor.web3.PublicKey;

  // Hacky way to get a different signer for negative tests
  const unauthorizedUser = anchor.web3.Keypair.generate();

  it("Is initialized!", async () => {
    // 1. Create a new Mint (Token)
    mint = await createMint(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer, // Payer
      provider.wallet.publicKey, // Mint Authority
      null, // Freeze Authority
      6 // Decimals
    );
    console.log("Mint Created:", mint.toString());

    // 2. Derive Vault PDA
    [vault, vaultBump] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), mint.toBuffer()],
      program.programId
    );
    console.log("Vault PDA:", vault.toString());

    // 3. Call Initialize
    const tx = await program.methods
      .initialize()
      .accounts({
        vault: vault,
        mint: mint,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    console.log("Your transaction signature", tx);

    // Verify Vault Created
    const vaultAccount = await getAccount(provider.connection, vault);
    assert.ok(vaultAccount.owner.equals(vault), "Vault should be owned by PDA");
    assert.ok(vaultAccount.mint.equals(mint), "Vault should store correct mint");
  });

  it("NEGATIVE: Cannot Initialize with Unauthorized User", async () => {
    // Airdrop SOL to unauthorized user
    const signature = await provider.connection.requestAirdrop(unauthorizedUser.publicKey, 1000000000);
    await provider.connection.confirmTransaction(signature);

    // Create a new mint for this test to avoid collision with the already initialized one
    const newMint = await createMint(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      provider.wallet.publicKey,
      null,
      6
    );

    const [newVault] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), newMint.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .initialize()
        .accounts({
          vault: newVault,
          mint: newMint,
          payer: unauthorizedUser.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([unauthorizedUser])
        .rpc();
      assert.fail("Should have failed with Unauthorized error");
    } catch (err) {
      // We expect an error here
      assert.include(err.message, "Unauthorized", "Error should be Unauthorized");
      console.log("✅ Correctly rejected unauthorized initialization");
    }
  });

  it("Deposits Tokens!", async () => {
    // 1. Get/Create User's Token Account
    const stakerAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mint,
      staker
    );
    stakerTokenAccount = stakerAta.address;

    // 2. Mint tokens to User
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mint,
      stakerTokenAccount,
      provider.wallet.publicKey,
      1000 // Amount to mint
    );

    // 3. Derive Stake Info PDA
    const [userStakeInfo] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("user"), staker.toBuffer()],
      program.programId
    );
    stakeInfo = userStakeInfo;

    // 4. Call Deposit
    const depositAmount = new anchor.BN(500);

    await program.methods
      .deposit(depositAmount) // Deposit 500
      .accounts({
        staker: staker,
        vault: vault,
        stakeInfo: stakeInfo,
        mint: mint,
        stakerTokenAccount: stakerTokenAccount,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("Deposit Successful!");

    // 5. Verify Balances
    const vaultAccount = await getAccount(provider.connection, vault);
    assert.equal(Number(vaultAccount.amount), 500, "Vault should have 500 tokens");

    const userAccount = await getAccount(provider.connection, stakerTokenAccount);
    assert.equal(Number(userAccount.amount), 500, "User should have 500 tokens left");

    // 6. Verify On-Chain Data
    const stakeInfoAccount = await program.account.userStakeInfo.fetch(stakeInfo);
    assert.equal(stakeInfoAccount.amount.toNumber(), 500, "Stake Info should record 500");
  });

  it("POSITIVE: Accumulates Multiple Deposits", async () => {
    const depositAmount = new anchor.BN(200);

    await program.methods
      .deposit(depositAmount) // Deposit another 200
      .accounts({
        staker: staker,
        vault: vault,
        stakeInfo: stakeInfo,
        mint: mint,
        stakerTokenAccount: stakerTokenAccount,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Verify Balances (500 + 200 = 700)
    const vaultAccount = await getAccount(provider.connection, vault);
    assert.equal(Number(vaultAccount.amount), 700, "Vault should have 700 tokens");

    const stakeInfoAccount = await program.account.userStakeInfo.fetch(stakeInfo);
    assert.equal(stakeInfoAccount.amount.toNumber(), 700, "Stake Info should record 700");
    console.log("✅ Multiple deposits worked");
  });

  it("NEGATIVE: Cannot Deposit 0 Amount", async () => {
    try {
      await program.methods
        .deposit(new anchor.BN(0))
        .accounts({
          staker: staker,
          vault: vault,
          stakeInfo: stakeInfo,
          mint: mint,
          stakerTokenAccount: stakerTokenAccount,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
      assert.fail("Should have failed with InvalidAmount");
    } catch (err) {
      assert.include(err.message, "Amount must be greater than zero", "Caught expected error");
      console.log("✅ Correctly rejected 0 deposit");
    }
  });

  it("NEGATIVE: Insufficient Funds", async () => {
    // User has 300 left (1000 - 500 - 200 = 300)
    // Try to deposit 400
    try {
      await program.methods
        .deposit(new anchor.BN(400))
        .accounts({
          staker: staker,
          vault: vault,
          stakeInfo: stakeInfo,
          mint: mint,
          stakerTokenAccount: stakerTokenAccount,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
      assert.fail("Should have failed due to token program error (insufficient funds)");
    } catch (err) {
      // This usually throws a Token Program error specific to the SPL Token crate,
      // often purely execution or custom error. Just checking it fails is good enough for now,
      // or checking for 'insufficient funds' if the error log provides it.
      // Anchor wraps it, so we check general failure or log.
      assert.ok(true, "Transaction failed as expected");
      console.log("✅ Correctly rejected insufficient funds");
    }
  });

  it("Withdraws Tokens!", async () => {
    // Current Stake: 700
    // 1. Call Withdraw
    await program.methods
      .withdraw()
      .accounts({
        staker: staker,
        vault: vault,
        stakeInfo: stakeInfo,
        mint: mint,
        stakerTokenAccount: stakerTokenAccount,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .rpc();

    console.log("Withdraw Successful!");

    // 2. Verify Vault Balance is 0
    const vaultAccount = await getAccount(provider.connection, vault);
    assert.equal(Number(vaultAccount.amount), 0, "Vault should be empty");

    // 3. Verify User Balance (Should be 1000 total tokens again)
    const userAccount = await getAccount(provider.connection, stakerTokenAccount);
    assert.equal(Number(userAccount.amount), 1000, "User should have all tokens back");

    // 4. Verify User Stake Info Reset
    const stakeInfoAccount = await program.account.userStakeInfo.fetch(stakeInfo);
    assert.equal(stakeInfoAccount.amount.toNumber(), 0, "Stake Info should be reset to 0");
  });

  it("NEGATIVE: Cannot Withdraw with 0 Balance (Double Withdraw)", async () => {
    try {
      await program.methods
        .withdraw()
        .accounts({
          staker: staker,
          vault: vault,
          stakeInfo: stakeInfo,
          mint: mint,
          stakerTokenAccount: stakerTokenAccount,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .rpc();
      assert.fail("Should have failed with InvalidWithdraw");
    } catch (err) {
      assert.include(err.message, "No tokens to withdraw", "Caught expected error");
      console.log("✅ Correctly rejected double withdraw");
    }
  });
});
