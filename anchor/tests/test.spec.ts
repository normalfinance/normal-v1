import * as anchor from '@coral-xyz/anchor'
import {Program} from '@coral-xyz/anchor'
import {Keypair} from '@solana/web3.js'
import {Test} from '../target/types/test'

describe('test', () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env()
  anchor.setProvider(provider)
  const payer = provider.wallet as anchor.Wallet

  const program = anchor.workspace.Test as Program<Test>

  const testKeypair = Keypair.generate()

  it('Initialize Test', async () => {
    await program.methods
      .initialize()
      .accounts({
        test: testKeypair.publicKey,
        payer: payer.publicKey,
      })
      .signers([testKeypair])
      .rpc()

    const currentCount = await program.account.test.fetch(testKeypair.publicKey)

    expect(currentCount.count).toEqual(0)
  })

  it('Increment Test', async () => {
    await program.methods.increment().accounts({ test: testKeypair.publicKey }).rpc()

    const currentCount = await program.account.test.fetch(testKeypair.publicKey)

    expect(currentCount.count).toEqual(1)
  })

  it('Increment Test Again', async () => {
    await program.methods.increment().accounts({ test: testKeypair.publicKey }).rpc()

    const currentCount = await program.account.test.fetch(testKeypair.publicKey)

    expect(currentCount.count).toEqual(2)
  })

  it('Decrement Test', async () => {
    await program.methods.decrement().accounts({ test: testKeypair.publicKey }).rpc()

    const currentCount = await program.account.test.fetch(testKeypair.publicKey)

    expect(currentCount.count).toEqual(1)
  })

  it('Set test value', async () => {
    await program.methods.set(42).accounts({ test: testKeypair.publicKey }).rpc()

    const currentCount = await program.account.test.fetch(testKeypair.publicKey)

    expect(currentCount.count).toEqual(42)
  })

  it('Set close the test account', async () => {
    await program.methods
      .close()
      .accounts({
        payer: payer.publicKey,
        test: testKeypair.publicKey,
      })
      .rpc()

    // The account should no longer exist, returning null.
    const userAccount = await program.account.test.fetchNullable(testKeypair.publicKey)
    expect(userAccount).toBeNull()
  })
})
