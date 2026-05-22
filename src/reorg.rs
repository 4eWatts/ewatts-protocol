//! Reorg engine: handles chain reorganization when a heavier fork is detected.
//! Integrates ChainStore, UtxoSet, and mempool for safe fork switching.

use crate::block::Block;
use crate::chain::ChainStore;
use crate::state::UtxoSet;

/// Result of checking a new block against the current chain.
#[derive(Debug)]
pub enum ForkDecision {
    /// Block extends canonical chain → accept normally.
    ExtendCanonical,
    /// Block is a heavier competing fork → reorg to this chain.
    ReorgToNew {
        /// Blocks to unwind (from current tip down to fork point).
        to_unwind: Vec<[u8; 32]>,
        /// Blocks to apply (from fork point up to new tip).
        to_apply: Vec<[u8; 32]>,
    },
    /// Block is a lighter competing fork → store but don't reorg.
    Sidechain,
    /// Block is an orphan (parent unknown) → store for later.
    Orphan,
    /// Block is invalid or already known.
    Reject(String),
}

/// Check what to do with a newly received block.
/// Does NOT modify the store or state.
pub fn analyze_fork(
    block: &Block,
    store: &ChainStore,
) -> ForkDecision {
    let hash = block.header.hash();
    let height = block.header.height;
    let prev_hash = block.header.previous_hash;

    // Already have this block?
    if store.get_block(&hash).is_some() {
        return ForkDecision::Reject("Duplicate block".into());
    }

    // Known parent?
    if store.get_block(&prev_hash).is_none() {
        if height == 0 {
            return ForkDecision::Reject("Genesis already exists".into());
        }
        return ForkDecision::Orphan;
    }

    // Extends canonical chain?
    if store.extends_canonical(&block.header) {
        return ForkDecision::ExtendCanonical;
    }

    // Competing fork?
    if store.is_competing_fork(&block.header) {
        let new_hash = hash;
        let tip_hash = store.chain_tip_hash();
        let current_work = store.chain_tip_work();
        let new_work = store.work_at(&new_hash);

        if new_work > current_work {
            // Heavier chain! Need to reorg.
            let lca = store.find_lca(&new_hash, &tip_hash);
            if let Some(fork_point) = lca {
                let to_unwind = store.get_chain_to_fork(&tip_hash, &fork_point);
                let to_apply_rev = store.get_chain_to_fork(&new_hash, &fork_point);
                // Reverse to_apply so it's from fork_point up to new tip
                let mut to_apply = to_apply_rev;
                to_apply.reverse();
                return ForkDecision::ReorgToNew { to_unwind, to_apply };
            }
        }

        return ForkDecision::Sidechain;
    }

    // Block at or below current height — check if extends a known sidechain
    if let Some(_) = store.get_block(&prev_hash) {
        // It extends a side chain
        let tip_hash = store.chain_tip_hash();
        let new_work = store.work_at(&hash);
        let current_work = store.chain_tip_work();

        if new_work > current_work {
            let lca = store.find_lca(&hash, &tip_hash);
            if let Some(fork_point) = lca {
                let to_unwind = store.get_chain_to_fork(&tip_hash, &fork_point);
                let mut to_apply = store.get_chain_to_fork(&hash, &fork_point);
                to_apply.reverse();
                return ForkDecision::ReorgToNew { to_unwind, to_apply };
            }
        }
        return ForkDecision::Sidechain;
    }

    ForkDecision::Orphan
}

/// Execute a full reorg: unwind current chain, apply new chain.
///
/// Returns:
/// - Ok(tx_hashes_to_resubmit): list of tx hashes from unwound blocks
///   that are NOT in the new chain (should be returned to mempool).
pub fn execute_reorg(
    to_unwind: &[[u8; 32]],
    to_apply: &[[u8; 32]],
    store: &mut ChainStore,
    state: &mut UtxoSet,
) -> Result<Vec<[u8; 32]>, String> {
    println!(
        "REORG: unwinding {} blocks, applying {} blocks",
        to_unwind.len(),
        to_apply.len()
    );

    // Safety: cap reorg depth at 100 blocks
    let max_reorg = 100;
    if to_unwind.len() > max_reorg || to_apply.len() > max_reorg {
        return Err(format!(
            "Reorg too deep: unwind={}, apply={}, max={}",
            to_unwind.len(),
            to_apply.len(),
            max_reorg
        ));
    }

    // Collect txs from unwound blocks for mempool resurrection
    let mut unwound_tx_hashes: Vec<[u8; 32]> = Vec::new();

    // Phase 1: Unwind current chain (tip → fork_point)
    for hash in to_unwind {
        let block = store.get_block(hash)
            .ok_or_else(|| "Block not found during unwind".to_string())?;
        let height = block.header.height;

        // Collect tx hashes for mempool (skip coinbase)
        for (tx_idx, tx) in block.body.transactions.iter().enumerate() {
            if tx_idx > 0 {
                unwound_tx_hashes.push(tx.hash());
            }
        }

        state.unwind_block(block, height)?;
        println!("  Unwound block #{} {:x}..", height, hash[0]);
    }

    // Phase 2: Apply new chain (fork_point → new tip)
    for hash in to_apply {
        let block = store.get_block(hash)
            .ok_or_else(|| "Block not found during reorg apply".to_string())?;
        let height = block.header.height;

        state.apply_block(block, height)?;
        println!("  Applied block #{} {:x}..", height, hash[0]);
    }

    // Phase 3: Update chain tip
    if let Some(new_tip) = to_apply.last() {
        store.set_chain_tip(new_tip).map_err(|e| format!("Set tip: {}", e))?;
    } else {
        return Err("No blocks to apply in reorg".into());
    }

    println!(
        "REORG complete: new tip #{} {:x}..",
        store.chain_tip_height(),
        store.chain_tip_hash()[0]
    );

    // Filter: only return tx hashes that are NOT in the new chain
    // (by checking which key_images are still in spent_key_images after reorg)
    let mut resurrect = Vec::new();
    for hash in to_unwind {
        if let Some(block) = store.get_block(hash) {
            for (tx_idx, tx) in block.body.transactions.iter().enumerate() {
                if tx_idx > 0 && !tx.inputs.is_empty() {
                    let all_still_spent = tx.inputs.iter()
                        .all(|i| state.spent_key_images().contains(&i.key_image));
                    if !all_still_spent {
                        // At least one input is no longer spent → tx can be resurrected
                        let tx_hash = tx.hash();
                        // But check if it's already in the new chain
                        let already_in_new_chain = to_apply.iter().any(|bh| {
                            store.get_block(bh)
                                .map(|b| b.body.transactions.iter().any(|t| t.hash() == tx_hash))
                                .unwrap_or(false)
                        });
                        if !already_in_new_chain {
                            resurrect.push(tx_hash);
                        }
                    }
                }
            }
        }
    }

    Ok(resurrect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::*;

    fn make_header(height: u64, prev: [u8; 32]) -> BlockHeader {
        BlockHeader {
            version: 1, previous_hash: prev, merkle_root: [0u8; 32],
            timestamp: 1000 + height, height, epoch: 0,
            difficulty_target: 100, total_effective_commit: 0.0,
            emission_rate: 0, miner_effective_commit: 0.0,
            vr_block: 0.0, coinbase_burn: 0, nonce: height, elapsed_ms: 0,
        }
    }

    fn make_block(height: u64, prev: [u8; 32]) -> Block {
        Block {
            header: make_header(height, prev),
            body: BlockBody { transactions: vec![], commitments: vec![] },
        }
    }

    #[test]
    fn test_extend_canonical() {
        let genesis = make_block(0, [0u8; 32]);
        let g_hash = genesis.header.hash();
        let store = ChainStore::new(genesis);

        let b1 = make_block(1, g_hash);
        let decision = analyze_fork(&b1, &store);
        assert!(matches!(decision, ForkDecision::ExtendCanonical));
    }

    #[test]
    fn test_reorg_detection() {
        let genesis = make_block(0, [0u8; 32]);
        let g_hash = genesis.header.hash();
        let mut store = ChainStore::new(genesis);

        // Chain A: 3 blocks
        let a1 = make_block(1, g_hash);
        let a1_hash = a1.header.hash();
        store.add_block(a1).unwrap();
        store.set_chain_tip(&a1_hash).unwrap();

        let a2 = make_block(2, a1_hash);
        let a2_hash = a2.header.hash();
        store.add_block(a2).unwrap();
        store.set_chain_tip(&a2_hash).unwrap();

        let a3 = make_block(3, a2_hash);
        let a3_hash = a3.header.hash();
        store.add_block(a3).unwrap();
        store.set_chain_tip(&a3_hash).unwrap();

        // Chain B: 4 blocks from genesis (heavier)
        let b1 = make_block(1, g_hash);
        let b1_hash = b1.header.hash();
        store.add_block(b1).unwrap();

        let b2 = make_block(2, b1_hash);
        let b2_hash = b2.header.hash();
        store.add_block(b2).unwrap();

        let b3 = make_block(3, b2_hash);
        let b3_hash = b3.header.hash();
        store.add_block(b3).unwrap();

        let b4 = make_block(4, b3_hash);
        let _b4_hash = b4.header.hash();
        store.add_block(b4).unwrap();

        // Check: B4 should trigger a reorg since B chain is heavier (4 blocks > 3)
        let b4_received = make_block(4, b3_hash); // same but fresh
        let decision = analyze_fork(&b4_received, &store);
        match &decision {
            ForkDecision::ReorgToNew { to_unwind, to_apply } => {
                assert_eq!(to_unwind.len(), 3); // A3, A2, A1
                assert_eq!(to_apply.len(), 4);  // B1, B2, B3, B4
            }
            other => panic!("Expected ReorgToNew, got {:?}", other),
        }
    }
}
