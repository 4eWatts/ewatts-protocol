//! ChainStore: fork-aware block tree with heaviest-chain tracking.
//! Manages all known blocks, chain tip, orphan queue, and fork detection.

use crate::block::{Block, BlockHeader};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// A lightweight block reference for the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockEntry {
    pub height: u64,
    pub accumulated_work: u128,
    pub block: Block,  // full block, kept in memory
}

/// How many orphan blocks we keep before evicting the oldest.
const MAX_ORPHANS: usize = 500;

/// The fork-aware block store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStore {
    /// All known blocks keyed by their hash.
    blocks: HashMap<[u8; 32], BlockEntry>,
    /// Block diffs keyed by block hash (for reorg unwinding).
    /// Populated when a block is applied via apply_block_and_track.
    #[serde(skip)]
    pub block_diffs: HashMap<[u8; 32], crate::state::BlockDiff>,
    /// Hash of the current canonical chain tip.
    chain_tip: [u8; 32],
    /// Orphan blocks: blocks whose parent is not yet known, keyed by hash.
    orphans: HashMap<[u8; 32], Block>,
    /// Insertion order for orphan eviction (FIFO).
    orphan_order: VecDeque<[u8; 32]>,
    /// Height of the current chain tip (cached for fast access).
    tip_height: u64,
    /// Cumulative work of the current chain tip.
    tip_work: u128,
}

impl ChainStore {
    /// Create a new ChainStore with genesis block.
    pub fn new(genesis: Block) -> Self {
        let genesis_hash = genesis.header.hash();
        let work = compute_block_work(&genesis.header) as u128;
        let mut blocks = HashMap::new();
        blocks.insert(genesis_hash, BlockEntry {
            height: 0,
            accumulated_work: work,
            block: genesis,
        });
        ChainStore {
            blocks,
            chain_tip: genesis_hash,
            orphans: HashMap::new(),
            orphan_order: VecDeque::new(),
            block_diffs: HashMap::new(),
            tip_height: 0,
            tip_work: work,
        }
    }

    /// Create empty ChainStore (for loading from disk).
    pub fn empty() -> Self {
        ChainStore {
            blocks: HashMap::new(),
            chain_tip: [0u8; 32],
            orphans: HashMap::new(),
            orphan_order: VecDeque::new(),
            block_diffs: HashMap::new(),
            tip_height: 0,
            tip_work: 0,
        }
    }

    // ── Getters ──

    pub fn chain_tip_hash(&self) -> [u8; 32] {
        self.chain_tip
    }

    pub fn chain_tip_height(&self) -> u64 {
        self.tip_height
    }

    pub fn chain_tip_work(&self) -> u128 {
        self.tip_work
    }

    pub fn get_block(&self, hash: &[u8; 32]) -> Option<&Block> {
        self.blocks.get(hash).map(|e| &e.block)
    }

    pub fn get_entry(&self, hash: &[u8; 32]) -> Option<&BlockEntry> {
        self.blocks.get(hash)
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub fn orphan_count(&self) -> usize {
        self.orphans.len()
    }

    /// Check if the chain has a genesis block loaded.
    pub fn has_genesis(&self) -> bool {
        self.blocks.contains_key(&self.chain_tip)
    }

    // ── Block insertion ──

    /// Add a block to the tree. Does NOT change the chain tip.
    /// Returns the parent's accumulated work so the caller can compute the fork's total work.
    pub fn add_block(&mut self, block: Block) -> Result<u128, String> {
        let hash = block.header.hash();
        self.add_block_inner(block, None)
    }

    /// Add a block and store its BlockDiff for reorg unwinding.
    pub fn add_block_with_diff(&mut self, block: Block, diff: crate::state::BlockDiff) -> Result<u128, String> {
        let hash = block.header.hash();
        let result = self.add_block_inner(block, Some(diff));
        result
    }

    fn add_block_inner(&mut self, block: Block, diff: Option<crate::state::BlockDiff>) -> Result<u128, String> {
        let hash = block.header.hash();
        let height = block.header.height;
        let parent_hash = block.header.previous_hash;

        // Prevent duplicate
        if self.blocks.contains_key(&hash) {
            return Err("Block already exists".into());
        }

        // Look up parent to compute accumulated work.
        // Use explicit match to avoid borrow conflicts with self.blocks.insert() below.
        let parent_work = match self.blocks.get(&parent_hash) {
            Some(e) => e.accumulated_work,
            None => return Err("Parent block not found".to_string()),
        };

        let block_work = compute_block_work(&block.header) as u128;
        let acc_work = parent_work.saturating_add(block_work);

        self.blocks.insert(hash, BlockEntry {
            height,
            accumulated_work: acc_work,
            block,
        });

        // Store BlockDiff if provided
        if let Some(d) = diff {
            self.block_diffs.insert(hash, d);
        }

        Ok(acc_work)
    }

    /// Set the canonical chain tip to the given block hash.
    /// The block MUST already be in the store.
    pub fn set_chain_tip(&mut self, hash: &[u8; 32]) -> Result<(), String> {
        let entry = self.blocks.get(hash)
            .ok_or_else(|| "Block not found when setting chain tip".to_string())?;
        self.chain_tip = *hash;
        self.tip_height = entry.height;
        self.tip_work = entry.accumulated_work;
        Ok(())
    }

    // ── Orphan management ──

    /// Add an orphan block (parent not yet known).
    pub fn add_orphan(&mut self, block: Block) {
        let hash = block.header.hash();
        if self.orphans.contains_key(&hash) {
            return;
        }
        // Evict oldest if full
        while self.orphans.len() >= MAX_ORPHANS {
            if let Some(oldest) = self.orphan_order.front().copied() {
                self.orphans.remove(&oldest);
                self.orphan_order.pop_front();
            } else {
                break;
            }
        }
        self.orphans.insert(hash, block);
        self.orphan_order.push_back(hash);
    }

    /// Try to resolve orphans after adding a new block.
    /// Returns the list of newly-resolved blocks (may include the original block
    /// if it was an orphan that can now connect).
    pub fn resolve_orphans(&mut self, parent_hash: &[u8; 32]) -> Vec<[u8; 32]> {
        let mut resolved = Vec::new();
        let mut found = true;
        while found {
            found = false;
            // Collect orphans whose parent is now known
            // Collect into a separate vec to avoid borrow conflicts with self.orphans.remove()
            let mut to_resolve = Vec::new();
            for (h, b) in &self.orphans {
                if b.header.previous_hash == *parent_hash {
                    to_resolve.push(*h);
                }
            }

            for hash in &to_resolve {
                if let Some(block) = self.orphans.remove(hash) {
                    self.orphan_order.retain(|h| h != hash);
                    if let Ok(_) = self.add_block(block) {
                        resolved.push(*hash);
                        // Recursively resolve children
                        let children = self.resolve_orphans(hash);
                        resolved.extend(children);
                    }
                }
            }
            if !to_resolve.is_empty() {
                found = true;
            }
        }
        resolved
    }

    // ── Ancestor walking ──

    /// Walk from `from_hash` back to genesis, collecting all ancestor hashes.
    pub fn get_ancestors(&self, from_hash: &[u8; 32]) -> Vec<[u8; 32]> {
        let mut ancestors = Vec::new();
        let mut current = *from_hash;
        loop {
            ancestors.push(current);
            if let Some(entry) = self.blocks.get(&current) {
                let prev = entry.block.header.previous_hash;
                if prev == [0u8; 32] {
                    break; // genesis reached
                }
                current = prev;
            } else {
                break;
            }
        }
        ancestors
    }

    /// Find the lowest common ancestor (fork point) between two chains.
    pub fn find_lca(&self, hash_a: &[u8; 32], hash_b: &[u8; 32]) -> Option<[u8; 32]> {
        let ancestors_a = self.get_ancestors(hash_a);
        let ancestors_b: std::collections::HashSet<[u8; 32]> =
            self.get_ancestors(hash_b).into_iter().collect();
        for h in &ancestors_a {
            if ancestors_b.contains(h) {
                return Some(*h);
            }
        }
        None
    }

    /// Compute the accumulated work of a block (from genesis to this block).
    pub fn work_at(&self, hash: &[u8; 32]) -> u128 {
        self.blocks.get(hash).map(|e| e.accumulated_work).unwrap_or(0)
    }

    /// Check if a block extends our canonical chain tip.
    pub fn extends_canonical(&self, header: &BlockHeader) -> bool {
        header.previous_hash == self.chain_tip
    }

    /// Check if this block creates a competing fork (different parent at same height).
    pub fn is_competing_fork(&self, header: &BlockHeader) -> bool {
        if header.previous_hash == self.chain_tip {
            return false; // extends canonical
        }
        // Check if there's already a block at (prev, height+1) in our canonical chain
        if let Some(tip_entry) = self.blocks.get(&self.chain_tip) {
            // If prev_hash is an ancestor of our tip and height is tip+1, it's a fork
            let ancestors: std::collections::HashSet<[u8; 32]> =
                self.get_ancestors(&self.chain_tip).into_iter().collect();
            return ancestors.contains(&header.previous_hash) && header.height == tip_entry.height + 1;
        }
        false
    }

    /// Get the chain from `fork_point` to `tip_hash` (exclusive of fork_point, inclusive of tip).
    /// Order: from tip down to fork_point (for unwinding).
    pub fn get_chain_to_fork(&self, tip_hash: &[u8; 32], fork_point: &[u8; 32]) -> Vec<[u8; 32]> {
        let mut chain = Vec::new();
        let mut current = *tip_hash;
        loop {
            if current == *fork_point || current == [0u8; 32] {
                break;
            }
            chain.push(current);
            if let Some(entry) = self.blocks.get(&current) {
                current = entry.block.header.previous_hash;
            } else {
                break;
            }
        }
        chain // from tip down to fork_point
    }
}

/// Compute the amount of work represented by a single block.
/// Uses the same formula as Bitcoin: work = 2^64 / difficulty_target.
/// For difficulty 100, work = 2^64 / 100 ≈ 1.84e17.
pub fn compute_block_work(header: &BlockHeader) -> u64 {
    if header.difficulty_target == 0 {
        return 0;
    }
    u64::MAX / header.difficulty_target.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::*;

    static NEXT_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    fn make_header(height: u64, prev: [u8; 32]) -> BlockHeader {
        let nonce = NEXT_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        BlockHeader {
            version: 1,
            previous_hash: prev,
            merkle_root: [0u8; 32],
            timestamp: 1000 + height,
            height,
            epoch: 0,
            difficulty_target: 100,
            total_effective_commit: 0.0,
            emission_rate: 0,
            miner_effective_commit: 0.0,
            vr_block: 0.0,
            coinbase_burn: 0,
            nonce,
            elapsed_ms: 0,
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
    fn test_simple_chain() {
        let genesis = make_block(0, [0u8; 32]);
        let g_hash = genesis.header.hash();
        let mut store = ChainStore::new(genesis);
        assert_eq!(store.block_count(), 1);
        assert_eq!(store.chain_tip_height(), 0);

        let b1 = make_block(1, g_hash);
        let b1_hash = b1.header.hash();
        store.add_block(b1).unwrap();
        store.set_chain_tip(&b1_hash).unwrap();
        assert_eq!(store.chain_tip_height(), 1);
    }

    #[test]
    fn test_fork_lca() {
        let genesis = make_block(0, [0u8; 32]);
        let g_hash = genesis.header.hash();
        let mut store = ChainStore::new(genesis);

        // Chain A: genesis → A1 → A2
        let a1 = make_block(1, g_hash);
        let a1_hash = a1.header.hash();
        store.add_block(a1).unwrap();
        store.set_chain_tip(&a1_hash).unwrap();

        let a2 = make_block(2, a1_hash);
        let a2_hash = a2.header.hash();
        store.add_block(a2).unwrap();
        store.set_chain_tip(&a2_hash).unwrap();

        // Chain B: genesis → B1 (competing with A1 at height 1)
        let b1 = make_block(1, g_hash);
        let b1_hash = b1.header.hash();
        store.add_block(b1).unwrap();

        // LCA between A2 and B1 should be genesis
        let lca = store.find_lca(&a2_hash, &b1_hash).unwrap();
        assert_eq!(lca, g_hash);
    }

    #[test]
    fn test_orphan_resolution() {
        let genesis = make_block(0, [0u8; 32]);
        let g_hash = genesis.header.hash();
        let mut store = ChainStore::new(genesis);

        // Orphan: b2 whose parent b1 is not yet known
        let b1 = make_block(1, g_hash);
        let b1_hash = b1.header.hash();

        let b2 = make_block(2, b1_hash);
        store.add_orphan(b2);
        assert_eq!(store.orphan_count(), 1);

        // Now add b1 — this should resolve b2
        store.add_block(b1).unwrap();
        let resolved = store.resolve_orphans(&b1_hash);
        // Check b2 was resolved (block at height 2 exists)
        let b2_exists = store.blocks.iter().any(|(_, e)| e.height == 2);
        assert!(b2_exists || resolved.len() > 0);
    }
}
