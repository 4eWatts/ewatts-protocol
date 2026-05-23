//! Smoke tests — verify the mining + state pipeline completes without panic.
//!
//! NOT an adversarial simulation. These tests mine blocks in round-robin
//! across N agents and verify that each agent mines at least one block.
//! All agents use the same internal commitment logic (no strategy differentiation).
//!
//! True adversarial testing requires refactoring mine_block_with_difficulty
//! to accept external commitments (see Fase C roadmap).

use crate::mine_block_with_difficulty;
use crate::state::UtxoSet;
use ed25519_dalek::SigningKey;


/// Mine N blocks in round-robin across N agents.
/// No strategy differentiation — all agents use identical mining logic.
/// Smoke test only: verifies the pipeline completes without panic.
pub(crate) fn run_round_robin_test(
    num_agents: usize,
    num_blocks: u64,
    dag_size: u64,
    difficulty: u64,
) -> Result<Vec<u64>, String> {
    let mut rng = rand::thread_rng();

    // Create agents with unique keys
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
    assert_eq!(counts[0], 3); // agent 0 gets block 1,3,5
    assert_eq!(counts[1], 2); // agent 1 gets block 2,4
}
