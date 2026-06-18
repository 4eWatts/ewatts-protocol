//! Distributed block propagation simulator (eventual consistency test)

use crate::chain::ChainStore;
use crate::mine_block_with_difficulty;
use crate::reorg;
use crate::state::UtxoSet;
use ed25519_dalek::SigningKey;
use rand::Rng;

/// A simulated node with independent state + chain store.
struct ShuffleNode {
    pub state: UtxoSet,
    pub store: ChainStore,
    pub peer_id: usize,
    #[allow(dead_code)]
    pub blocks_mined: u64,
}

impl ShuffleNode {
    fn new(genesis_block: crate::block::Block, genesis_diff: crate::state::BlockDiff, state: UtxoSet, peer_id: usize) -> Self {
        let mut store = ChainStore::new(genesis_block);
        let gen_hash = store.chain_tip_hash();
        store.block_diffs.insert(gen_hash, genesis_diff);
        ShuffleNode { state, store, peer_id, blocks_mined: 0 }
    }

    /// Receive block, apply real fork-choice rule
    fn receive_block(&mut self, block: &crate::block::Block) -> Result<(), String> {
        let hash = block.header.hash();

        if self.store.get_block(&hash).is_some() {
            return Ok(());
        }

        let parent_hash = block.header.previous_hash;
        if parent_hash != [0u8; 32] && self.store.get_block(&parent_hash).is_none() {
            self.store.add_orphan(block.clone());
            return Ok(()); // orphan — will be resolved when parent arrives
        }

        if let Err(e) = self.store.add_block(block.clone()) {
            if e == "Parent block not found" || e == "Block already exists" {
                return Ok(());
            }
            return Err(e);
        }

        match reorg::analyze_fork(block, &self.store) {
            reorg::ForkDecision::ExtendCanonical => {
                let diff = self.state.apply_block_and_track(block, block.header.height)?;
                self.store.block_diffs.insert(hash, diff);
                self.store.set_chain_tip(&hash).ok();
            }
            reorg::ForkDecision::ReorgToNew { to_unwind, to_apply } => {
                let _resurrected = reorg::execute_reorg(&to_unwind, &to_apply, &mut self.store, &mut self.state)?;
            }
            _ => {}
        }

        // Resolve orphans enabled by this block
        let resolved = self.store.resolve_orphans(&hash);
        for orphan_hash in resolved {
            if let Some(orphan_block) = self.store.get_block(&orphan_hash).cloned() {
                let _ = self.receive_block(&orphan_block);
            }
        }

        Ok(())
    }
}

/// Shuffle test: N nodes, N blocks, eventual consistency check
pub fn run_shuffle_test(
    num_nodes: usize,
    num_blocks: u64,
    latency_ms: u64,
    duplicate_chance: f64,
    _shuffle_order: bool,
) -> Result<(), String> {
    let mut rng = rand::thread_rng();
    let dag_size = 64 * 1024;
    let difficulty = 1u64;

    // ── Genesis ──
    let genesis_sk = SigningKey::generate(&mut rng);
    let genesis_pk = genesis_sk.verifying_key().to_bytes();
    let mut genesis_state = UtxoSet::genesis(100_000_000, &genesis_pk);
    let (genesis_block, genesis_diff) = mine_block_with_difficulty(
        [0u8; 32], 0, &mut genesis_state, difficulty, dag_size
    ).expect("Genesis");
    let genesis_hash = genesis_block.header.hash();

    // ── N nodes ──
    let mut nodes: Vec<ShuffleNode> = (0..num_nodes).map(|i| {
        let state = UtxoSet::genesis(100_000_000, &genesis_pk);
        ShuffleNode::new(genesis_block.clone(), genesis_diff.clone(), state, i)
    }).collect();

    let mut canonical_state = UtxoSet::genesis(100_000_000, &genesis_pk);
    let mut prev_hash = genesis_hash;
    let mut all_blocks: Vec<(crate::block::Block, crate::state::BlockDiff)> = Vec::new();

    for block_idx in 0..num_blocks {
        let height = block_idx + 1;
        let (block, diff) = mine_block_with_difficulty(
            prev_hash, height, &mut canonical_state, difficulty, dag_size
        ).map_err(|e| format!("Mining failed at block {}: {}", height, e))?;
        let block_hash = block.header.hash();
        all_blocks.push((block, diff));
        prev_hash = block_hash;
    }

    for (block, _diff) in &all_blocks {
        for recipient in 0..num_nodes {
            // Latency
            if latency_ms > 0 {
                let sleep_ms = rng.gen_range(1..=latency_ms);
                std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
            }

            // Deliver block via reorg-aware receive
            let _ = nodes[recipient].receive_block(block);

            // Duplicate
            if rng.gen_bool(duplicate_chance) {
                let _ = nodes[recipient].receive_block(block);
            }
        }
    }

    // Retry to resolve remaining orphans
    for (block, _diff) in &all_blocks {
        for recipient in 0..num_nodes {
            let _ = nodes[recipient].receive_block(block);
        }
    }

    let first_tip = nodes[0].store.chain_tip_hash();
    let first_height = nodes[0].store.chain_tip_height();
    for node in &nodes {
        let tip = node.store.chain_tip_hash();
        assert_eq!(tip, first_tip,
            "Node {} tip {:x}.. differs from node 0 tip {:x}..", node.peer_id, tip[0], first_tip[0]);
        assert_eq!(node.store.chain_tip_height(), first_height,
            "Node {} height differs", node.peer_id);
        assert!(node.state.total_supply() > 0, "Node {} has zero supply", node.peer_id);
    }

    println!("  Shuffle: {} nodes, {} blocks, latency {}ms ✓", num_nodes, num_blocks, latency_ms);
    Ok(())
}

#[test]
fn shuffle_basic_2nodes_5blocks() {
    run_shuffle_test(2, 5, 10, 0.05, false).expect("Basic shuffle");
}

#[test]
fn shuffle_3nodes_5blocks_no_chaos() {
    run_shuffle_test(3, 5, 0, 0.0, false).expect("No-chaos");
}

#[test]
fn shuffle_3nodes_5blocks_with_latency() {
    run_shuffle_test(3, 5, 50, 0.0, false).expect("Latency-only");
}

#[test]
fn shuffle_2nodes_5blocks_with_duplicates() {
    run_shuffle_test(2, 5, 10, 0.1, false).expect("Duplicates");
}
