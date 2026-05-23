//! Reorg engine: handles chain reorganization when a heavier fork is detected.
//! Integrates ChainStore, UtxoSet, and mempool for safe fork switching.

use crate::block::Block;
use crate::chain::ChainStore;
use crate::state::{BlockDiff, UtxoKey, UtxoSet};

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

    // Check if this block extends a known chain (sidechain or canonical fork)
    // by computing the accumulated work of the chain it would create.
    let tip_hash = store.chain_tip_hash();
    let current_work = store.chain_tip_work();
    let block_work = crate::chain::compute_block_work(&block.header) as u128;
    let parent_work = store.work_at(&prev_hash);
    let new_work = parent_work.saturating_add(block_work);

    if new_work > current_work {
        // Heavier chain! Need to reorg.
        // Use parent hash for LCA (new block isn't in the store yet)
        let lca = store.find_lca(&prev_hash, &tip_hash);
        if let Some(fork_point) = lca {
            let to_unwind = store.get_chain_to_fork(&tip_hash, &fork_point);
            let mut to_apply = store.get_chain_to_fork(&prev_hash, &fork_point);
            to_apply.reverse(); // now fork_point → ... → parent
            to_apply.push(hash); // add the new block
            return ForkDecision::ReorgToNew { to_unwind, to_apply };
        }
    }

    // Not heavier — just a sidechain or non-competing block
    ForkDecision::Sidechain
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

    // Snapshot for atomic rollback on failure
    let state_snapshot = state.clone();
    let store_snapshot = store.clone();

    match execute_reorg_inner(to_unwind, to_apply, store, state) {
        Ok(resurrect) => Ok(resurrect),
        Err(e) => {
            *state = state_snapshot;
            *store = store_snapshot;
            println!("REORG FAILED: rolling back snapshot -- {}", e);
            Err(e)
        }
    }
}

/// Inner reorg (no rollback). Caller handles atomicity via snapshot.
fn execute_reorg_inner(
    to_unwind: &[[u8; 32]],
    to_apply: &[[u8; 32]],
    store: &mut ChainStore,
    state: &mut UtxoSet,
) -> Result<Vec<[u8; 32]>, String> {
    // Phase 1: Unwind current chain (tip -> fork_point)
    for hash in to_unwind {
        let block = store.get_block(hash)
            .ok_or_else(|| "Block not found during unwind".to_string())?;
        let height = block.header.height;
        // Use BlockDiff unwind when available (P2P path), fallback to legacy unwind
        if let Some(diff) = store.block_diffs.get(hash) {
            state.unwind_with_diff(diff)?;
            println!("  Unwound block #{} {:x}.. (diff)", height, hash[0]);
        } else {
            // Fallback: construct BlockDiff from block data (disk-loaded or pre-diff era)
            let mut fallback_diff = BlockDiff::new();
            for (tx_idx, tx) in block.body.transactions.iter().enumerate() {
                let tx_hash = tx.hash();
                if tx_idx == 0 {
                    // Coinbase: track created outputs and supply delta
                    let coinbase_amount: u64 = tx.outputs.iter().map(|o| o.amount).sum();
                    fallback_diff.supply_delta = fallback_diff.supply_delta.wrapping_add(coinbase_amount as i64);
                    for (i, _) in tx.outputs.iter().enumerate() {
                        fallback_diff.created.push(UtxoKey { tx_hash, output_index: i as u32 });
                    }
                } else {
                    // Regular tx: track created outputs and key images to un-mark
                    for (i, _) in tx.outputs.iter().enumerate() {
                        fallback_diff.created.push(UtxoKey { tx_hash, output_index: i as u32 });
                    }
                    for input in &tx.inputs {
                        fallback_diff.key_images.push(input.key_image);
                    }
                }
            }
            state.unwind_with_diff(&fallback_diff)?;
            println!("  Unwound block #{} {:x}.. (fallback)", height, hash[0]);
        }
    }

    // Phase 2: Apply new chain (fork_point -> new tip), tracking diffs
    for hash in to_apply {
        let block = store.get_block(hash)
            .ok_or_else(|| "Block not found during reorg apply".to_string())?;
        let height = block.header.height;
        // Capture diff for future unwinds
        let diff = state.apply_block_and_track(block, height)?;
        store.block_diffs.insert(*hash, diff);
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

    // Determine which txs from unwound blocks should be resurrected
    let mut resurrect = Vec::new();
    for hash in to_unwind {
        if let Some(block) = store.get_block(hash) {
            for (tx_idx, tx) in block.body.transactions.iter().enumerate() {
                if tx_idx > 0 && !tx.inputs.is_empty() {
                    let all_still_spent = tx.inputs.iter()
                        .all(|i| state.spent_key_images().contains(&i.key_image));
                    if !all_still_spent {
                        let tx_hash = tx.hash();
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
    #![allow(deprecated)] // unwind_block fallback for blocks without diffs
    use super::*;
    use crate::block::*;

    static NEXT_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    fn make_header(height: u64, prev: [u8; 32]) -> BlockHeader {
        let nonce = NEXT_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        BlockHeader {
            version: 1, previous_hash: prev, merkle_root: [0u8; 32],
            timestamp: 1000 + height, height, epoch: 0,
            difficulty_target: 100, total_effective_commit: 0.0,
            emission_rate: 0, miner_effective_commit: 0.0,
            vr_block: 0.0, coinbase_burn: 0, nonce, elapsed_ms: 0,
            proof_merkle_root: None,
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

        // B4 is the NEW block we're analyzing — NOT yet in the store
        let b4_received = make_block(4, b3_hash);

        // Check: B4 should trigger a reorg since B chain is heavier (4 blocks > 3)
        let decision = analyze_fork(&b4_received, &store);
        match &decision {
            ForkDecision::ReorgToNew { to_unwind, to_apply } => {
                assert_eq!(to_unwind.len(), 3, "should unwind A3, A2, A1"); // A3, A2, A1
                assert_eq!(to_apply.len(), 4, "should apply B1, B2, B3, B4");  // B1, B2, B3, B4
            }
            other => panic!("Expected ReorgToNew, got {:?}", other),
        }
    }

    #[test]
    fn test_full_reorg_state_integrity() {
        // Full reorg test: create two chains, execute reorg, validate state.
        // Chain A (height 1-3) vs Chain B (height 1-4, heavier via more blocks).
        // Each block has a coinbase tx creating a unique UTXO for tracking.

        let mut rng = rand::thread_rng();
        use crate::privacy::Commitment;
        use crate::state::UtxoSet;

        // ── Genesis ──
        let genesis = make_block(0, [0u8; 32]);
        let g_hash = genesis.header.hash();
        let mut store = ChainStore::new(genesis);

        // ── Helper: create a block with one coinbase tx ──
        let mut seq: u64 = 0;
        let mut make_coinbase_block = |height: u64, prev: [u8; 32], amount: u64, _label: &str| -> Block {
            seq += 1;
            let nonce = NEXT_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let header = BlockHeader {
                version: 1, previous_hash: prev, merkle_root: [0u8; 32],
                timestamp: 1000 + height, height, epoch: 0,
                difficulty_target: 100, total_effective_commit: 0.0,
                emission_rate: 0, miner_effective_commit: 0.0,
                vr_block: 0.0, coinbase_burn: 0, nonce, elapsed_ms: 0,
                proof_merkle_root: None,
            };
            // Create a range proof with blinding=0 for test stability
            let rp = crate::tests::range_proof_zero_blinding(amount, &mut rng);
            let comm = Commitment::new_with_blinding(amount, curve25519_dalek::scalar::Scalar::from(0u64));
            let tx = Transaction {
                version: 1, inputs: vec![],
                outputs: vec![TxOutput {
                    amount, public_key: vec![], spendable_after: crate::reward::founder_lock_block(height),
                    stealth_dest: None,
                    commitment_bytes: Some(comm.0.compress().to_bytes()),
                    range_proof_bytes: Some(serde_json::to_vec(&rp).unwrap()),
                    ephemeral: None,
                }],
                ring_size: 1, signatures: vec![], mlsag: None, ring_members: None,
            };
            Block { header, body: BlockBody { transactions: vec![tx], commitments: vec![] } }
        };

        // ── Genesis state ──
        let mut state = UtxoSet::new();

        // ── Build Chain A (height 1-3) ──
        let a1 = make_coinbase_block(1, g_hash, 100, "A1");
        let a1_hash = a1.header.hash();
        let a1_diff = state.apply_block_and_track(&a1, 1).unwrap();
        store.add_block_with_diff(a1, a1_diff).unwrap();
        store.set_chain_tip(&a1_hash).unwrap();
        let _old_supply = state.total_supply();

        let a2 = make_coinbase_block(2, a1_hash, 200, "A2");
        let a2_hash = a2.header.hash();
        let a2_diff = state.apply_block_and_track(&a2, 2).unwrap();
        store.add_block_with_diff(a2, a2_diff).unwrap();
        store.set_chain_tip(&a2_hash).unwrap();

        let a3 = make_coinbase_block(3, a2_hash, 300, "A3");
        let a3_hash = a3.header.hash();
        let a3_diff = state.apply_block_and_track(&a3, 3).unwrap();
        store.add_block_with_diff(a3, a3_diff).unwrap();
        store.set_chain_tip(&a3_hash).unwrap();

        let state_after_a = state.total_supply();
        assert_eq!(state_after_a, 100 + 200 + 300, "Chain A supply should sum coinbases");

        // ── Build Chain B (height 1-4, heavier) ──
        let b1 = make_coinbase_block(1, g_hash, 50, "B1");
        let b1_hash = b1.header.hash();
        store.add_block(b1).unwrap(); // add without diff first (simulating sidechain)

        let b2 = make_coinbase_block(2, b1_hash, 60, "B2");
        let b2_hash = b2.header.hash();
        store.add_block(b2).unwrap();

        let b3 = make_coinbase_block(3, b2_hash, 70, "B3");
        let b3_hash = b3.header.hash();
        store.add_block(b3).unwrap();

        // B4 is NOT added to the store (analyze_fork checks the block before adding)
        let b4 = make_coinbase_block(4, b3_hash, 80, "B4");
        let b4_hash = b4.header.hash();

        // Verify fork detection on the NEW block
        let decision = analyze_fork(&b4, &store);
        let (to_unwind, to_apply) = match &decision {
            ForkDecision::ReorgToNew { to_unwind, to_apply } => (to_unwind, to_apply),
            other => panic!("Expected ReorgToNew, got {:?}", other),
        };
        assert_eq!(to_unwind.len(), 3, "should unwind A3, A2, A1");
        assert_eq!(to_apply.len(), 4, "should apply B1, B2, B3, B4");

        // Add B4 to store so execute_reorg can find it
        store.add_block(b4).unwrap();

        // ── Execute reorg ──
        let _state_before_reorg = state.clone();
        let result = execute_reorg(to_unwind, to_apply, &mut store, &mut state);
        assert!(result.is_ok(), "Reorg should succeed: {:?}", result);

        // ── Verify state after reorg ──
        let state_after_reorg_supply = state.total_supply();
        // Chain B coinbases: 50 + 60 + 70 + 80 = 260
        assert_eq!(state_after_reorg_supply, 50 + 60 + 70 + 80,
            "State after reorg should match Chain B coinbases, got {} expected {}",
            state_after_reorg_supply, 50 + 60 + 70 + 80);

        // Verify tip updated
        assert_eq!(store.chain_tip_hash(), b4_hash, "Tip should be B4");
        assert_eq!(store.chain_tip_height(), 4, "Tip height should be 4");

        // Verify diffs are stored for the new chain
        for hash in &[b1_hash, b2_hash, b3_hash, b4_hash] {
            assert!(store.block_diffs.contains_key(hash),
                "Diff should exist for {:x}..", hash[0]);
        }

        // Verify Chain A diffs are GONE (removed by unwind phase via unwind_with_diff)
        // Actually, they stay in block_diffs — they just weren't re-added.
        // The unwind phase reads them, the apply phase writes new diffs.
        // A's diffs should still be in the map (not removed).
        for hash in &[a1_hash, a2_hash, a3_hash] {
            assert!(store.block_diffs.contains_key(hash),
                "Chain A diffs should persist in store for potential re-reorg");
        }
    }
}
