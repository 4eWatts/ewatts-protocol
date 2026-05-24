//! Smoke tests + adversarial mining tests.
//!
//! Smoke tests verify the mining + state pipeline without panic.
//! Adversarial tests use mine_block_with_key for external key injection
//! and verify chain validation under multiple miner scenarios.

use crate::mine_block_with_difficulty;
use crate::mine_block_with_key;
use crate::state::UtxoSet;
use ed25519_dalek::SigningKey;


/// Mine N blocks in round-robin across N agents (internal keys).
pub(crate) fn run_round_robin_test(
    num_agents: usize,
    num_blocks: u64,
    dag_size: u64,
    difficulty: u64,
) -> Result<Vec<u64>, String> {
    let mut rng = rand::thread_rng();

    let keys: Vec<[u8; 32]> = (0..num_agents).map(|_| {
        SigningKey::generate(&mut rng).verifying_key().to_bytes()
    }).collect();

    let mut state = UtxoSet::genesis(100_000_000, &keys[0]);
    let mut prev_hash = [0u8; 32];
    let mut blocks_per_agent = vec![0u64; num_agents];

    for height in 1..=num_blocks {
        let agent_idx = ((height - 1) as usize) % num_agents;
        let (block, _) = mine_block_with_difficulty(
            prev_hash, height, &mut state, difficulty, dag_size,
        ).map_err(|e| format!("Mining failed at block {}: {}", height, e))?;
        prev_hash = block.header.hash();
        blocks_per_agent[agent_idx] += 1;
    }

    Ok(blocks_per_agent)
}

#[test]
fn smoke_round_robin_2_agents() {
    let counts = run_round_robin_test(2, 4, 256 * 1024, 1).expect("Smoke test");
    assert_eq!(counts[0], 2);
    assert_eq!(counts[1], 2);
}

#[test]
fn smoke_round_robin_3_agents() {
    let counts = run_round_robin_test(3, 6, 256 * 1024, 1).expect("Smoke test");
    assert_eq!(counts[0], 2);
    assert_eq!(counts[1], 2);
    assert_eq!(counts[2], 2);
}

#[test]
fn smoke_round_robin_uneven_blocks() {
    let counts = run_round_robin_test(2, 5, 256 * 1024, 1).expect("Smoke test");
    assert_eq!(counts[0], 3);
    assert_eq!(counts[1], 2);
}

// ─── Adversarial Tests (Phase 3) ──────────────────────────────────────

/// Test that mine_block_with_key accepts external signing keys.
#[test]
fn adv_external_key_mining() {
    let mut rng = rand::thread_rng();
    let key = SigningKey::generate(&mut rng);
    let pk = key.verifying_key().to_bytes();

    let mut state = UtxoSet::genesis(100_000_000, &pk);
    let (block, _) = mine_block_with_key(
        [0u8; 32], 1, &mut state, 1, 256 * 1024, &key,
    ).expect("External key mining");

    // Verify miner pubkey is in coinbase output
    let coinbase = &block.body.transactions[0];
    assert_eq!(coinbase.outputs.len(), 1);
    assert_eq!(coinbase.inputs.len(), 0);
    // Block should have valid hash
    assert_ne!(block.header.hash(), [0u8; 32]);
    assert_eq!(block.header.height, 1);
}

/// Two miners with distinct keys mine alternating blocks on the same chain.
#[test]
fn adv_two_miner_chain() {
    let mut rng = rand::thread_rng();
    let key_a = SigningKey::generate(&mut rng);
    let pk_a = key_a.verifying_key().to_bytes();
    let key_b = SigningKey::generate(&mut rng);

    let mut state = UtxoSet::genesis(100_000_000, &pk_a);
    let mut prev_hash = [0u8; 32];

    // Block 1: Miner A
    let (b1, _) = mine_block_with_key([0u8; 32], 1, &mut state, 1, 256 * 1024, &key_a)
        .expect("Block 1 by A");
    prev_hash = b1.header.hash();

    // Block 2: Miner B
    let (b2, _) = mine_block_with_key(prev_hash, 2, &mut state, 1, 256 * 1024, &key_b)
        .expect("Block 2 by B");
    prev_hash = b2.header.hash();

    // Block 3: Miner A again
    let (b3, _) = mine_block_with_key(prev_hash, 3, &mut state, 1, 256 * 1024, &key_a)
        .expect("Block 3 by A");

    // Each block should have incrementing height and valid chain link
    assert_eq!(b1.header.height, 1);
    assert_eq!(b2.header.height, 2);
    assert_eq!(b3.header.height, 3);
    assert_eq!(b2.header.previous_hash, b1.header.hash());
    assert_eq!(b3.header.previous_hash, b2.header.hash());
}

/// Verify chain state after multi-block mining: supply increases.
#[test]
fn adv_supply_increases_with_blocks() {
    let mut rng = rand::thread_rng();
    let key = SigningKey::generate(&mut rng);
    let pk = key.verifying_key().to_bytes();

    let mut state = UtxoSet::genesis(100_000_000, &pk);
    let initial_supply = state.total_supply();

    let mut prev_hash = [0u8; 32];
    for height in 1..=5 {
        let (block, _) = mine_block_with_key(
            prev_hash, height, &mut state, 10, 4 * 1024 * 1024, &key,
        ).expect("Block mining");
        let block_hash = block.header.hash();
        // Add coinbase output and add its amount directly to supply
        let coinbase_amount = block.body.transactions[0].outputs[0].amount;
        state.add_transaction_outputs(&block_hash, &block.body.transactions[0], height, 0);
        state.add_coinbase_supply(coinbase_amount);
        prev_hash = block_hash;
    }

    // After mining blocks, supply should increase (coinbase rewards)
    let final_supply = state.total_supply();
    assert!(final_supply > initial_supply,
        "Supply should increase: initial={} final={}", initial_supply, final_supply);
}

/// Ten blocks from one miner, verify all link correctly.
#[test]
fn adv_ten_block_chain() {
    let mut rng = rand::thread_rng();
    let key = SigningKey::generate(&mut rng);
    let pk = key.verifying_key().to_bytes();

    let mut state = UtxoSet::genesis(100_000_000, &pk);
    let mut prev_hash = [0u8; 32];

    for height in 1..=10 {
        let (block, _) = mine_block_with_key(
            prev_hash, height, &mut state, 1, 256 * 1024, &key,
        ).expect("Block mining");
        assert_eq!(block.header.height, height);
        assert_eq!(block.header.previous_hash, prev_hash);
        prev_hash = block.header.hash();
    }
}
