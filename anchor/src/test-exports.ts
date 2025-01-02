// Here we export some useful types and functions for interacting with the Anchor program.
import { AnchorProvider, Program } from '@coral-xyz/anchor'
import { Cluster, PublicKey } from '@solana/web3.js'
import TestIDL from '../target/idl/test.json'
import type { Test } from '../target/types/test'

// Re-export the generated IDL and type
export { Test, TestIDL }

// The programId is imported from the program IDL.
export const TEST_PROGRAM_ID = new PublicKey(TestIDL.address)

// This is a helper function to get the Test Anchor program.
export function getTestProgram(provider: AnchorProvider, address?: PublicKey) {
  return new Program({ ...TestIDL, address: address ? address.toBase58() : TestIDL.address } as Test, provider)
}

// This is a helper function to get the program ID for the Test program depending on the cluster.
export function getTestProgramId(cluster: Cluster) {
  switch (cluster) {
    case 'devnet':
    case 'testnet':
      // This is the program ID for the Test program on devnet and testnet.
      return new PublicKey('coUnmi3oBUtwtd9fjeAvSsJssXh5A5xyPbhpewyzRVF')
    case 'mainnet-beta':
    default:
      return TEST_PROGRAM_ID
  }
}
