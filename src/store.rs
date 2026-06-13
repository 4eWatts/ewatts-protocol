use crate::block::Block;
use crate::state::UtxoSet;
use sha3::Keccak256;
use digest::Digest;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Mutex;

/// Default data directory (relative to CWD).
const DEFAULT_DATA_DIR: &str = "ewatts_data";

/// Overridable data directory for testing.
/// When set via `set_data_dir()`, all store ops use this path (absolute).
/// Uses Mutex so it can be updated across tests.
static OVERRIDE_DATA_DIR: Mutex<Option<String>> = Mutex::new(None);

/// Return the active data directory path.
pub fn data_dir() -> String {
    if let Ok(lock) = OVERRIDE_DATA_DIR.lock() {
        if let Some(ref path) = *lock {
            return path.clone();
        }
    }
    DEFAULT_DATA_DIR.to_string()
}

/// Override the data directory (used in tests to point to a temp dir).
pub fn set_data_dir(path: String) {
    if let Ok(mut lock) = OVERRIDE_DATA_DIR.lock() {
        *lock = Some(path);
    }
}

/// Clear the override and go back to the default.
pub fn clear_data_dir() {
    if let Ok(mut lock) = OVERRIDE_DATA_DIR.lock() {
        *lock = None;
    }
}

/// Maximum number of blocks to keep in the in-memory BLOCK_CACHE.
/// Prevents unbounded memory growth while keeping recent blocks
/// available for fast lookups (gossip, mining, RPC).
const MAX_CACHED_BLOCKS: usize = 10_000;

/// In-memory block cache to avoid O(N) load_blocks on every gossip event.
/// Only holds the most recent `MAX_CACHED_BLOCKS`. Older blocks are
/// still available from disk (blocks.jsonl) and loaded on demand.
static BLOCK_CACHE: Mutex<Option<Vec<Block>>> = Mutex::new(None);

/// Truncate the block cache to the most recent `MAX_CACHED_BLOCKS`.
fn truncate_cache(cache: &mut Vec<Block>) {
    if cache.len() > MAX_CACHED_BLOCKS {
        let drain_count = cache.len() - MAX_CACHED_BLOCKS;
        cache.drain(0..drain_count);
    }
}

/// Invalidate the block cache (call after saving a new block).
pub fn invalidate_cache() {
    if let Ok(mut cache) = BLOCK_CACHE.lock() {
        *cache = None;
    }
}

/// Return the number of blocks we have cached (fast, no disk I/O).
pub fn cached_block_count() -> usize {
    if let Ok(cache) = BLOCK_CACHE.lock() {
        if let Some(blocks) = &*cache {
            return blocks.len();
        }
    }
    // Fall back to disk — fast line count without parsing blocks
    block_count().unwrap_or(0)
}

/// Fast block count by counting lines in blocks.jsonl (no block parsing).
pub fn block_count() -> Result<usize, String> {
    let path = format!("{}/blocks.jsonl", data_dir());
    if !Path::new(&path).exists() {
        return Ok(0);
    }
    let file = fs::File::open(&path).map_err(|e| format!("open: {}", e))?;
    let reader = BufReader::new(file);
    Ok(reader.lines().filter_map(|l| l.ok()).filter(|l| !l.trim().is_empty()).count())
}

/// Return the tip height (0-indexed) from cache. Returns None if no blocks.
pub fn chain_tip_height() -> Option<u64> {
    if let Ok(cache) = BLOCK_CACHE.lock() {
        if let Some(blocks) = &*cache {
            if blocks.is_empty() {
                return None;
            }
            return Some(blocks.last().map(|b| b.header.height).unwrap_or(0));
        }
    }
    // Fall back to disk count
    let count = block_count().ok()?;
    if count == 0 {
        None
    } else {
        Some((count - 1) as u64)
    }
}

/// Return the chain tip block hash. Returns None if no blocks.
pub fn chain_tip_hash() -> Option<[u8; 32]> {
    if let Ok(cache) = BLOCK_CACHE.lock() {
        if let Some(blocks) = &*cache {
            return blocks.last().map(|b| b.header.hash());
        }
    }
    // Fast path: read last line from disk without loading all blocks
    latest_block_hash().ok().flatten()
}

/// Read only the last block hash from disk (fast, no full parse of all blocks).
pub fn latest_block_hash() -> Result<Option<[u8; 32]>, String> {
    let path = format!("{}/blocks.jsonl", data_dir());
    if !Path::new(&path).exists() {
        return Ok(None);
    }
    let file = fs::File::open(&path).map_err(|e| format!("open: {}", e))?;
    let reader = BufReader::new(file);
    let mut last_line = String::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read: {}", e))?;
        if !line.trim().is_empty() {
            last_line = line;
        }
    }
    if last_line.is_empty() {
        return Ok(None);
    }
    let block: Block = serde_json::from_str(&last_line).map_err(|e| format!("parse: {}", e))?;
    Ok(Some(block.header.hash()))
}

/// Get a block by hash from the cache, or load on demand from disk.
pub fn get_block_by_hash(target_hash: &[u8; 32]) -> Option<Block> {
    // Check cache first
    if let Ok(cache) = BLOCK_CACHE.lock() {
        if let Some(blocks) = &*cache {
            for block in blocks.iter().rev() {
                if block.header.hash() == *target_hash {
                    return Some(block.clone());
                }
            }
        }
    }
    // Cache miss: scan disk (linear, but only done once per uncached request)
    let path = format!("{}/blocks.jsonl", data_dir());
    if !Path::new(&path).exists() {
        return None;
    }
    let file = fs::File::open(&path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.ok()?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(block) = serde_json::from_str::<Block>(&line) {
            if block.header.hash() == *target_hash {
                return Some(block);
            }
        }
    }
    None
}

/// Load blocks from disk starting at a given height (inclusive).
/// More efficient than loading all blocks when only recent history is needed.
pub fn load_blocks_since(from_height: u64) -> Result<Vec<Block>, String> {
    let path = format!("{}/blocks.jsonl", data_dir());
    if !Path::new(&path).exists() {
        return Ok(vec![]);
    }
    let file = fs::File::open(&path).map_err(|e| format!("open: {}", e))?;
    let reader = BufReader::new(file);
    let mut blocks = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read: {}", e))?;
        if line.trim().is_empty() {
            continue;
        }
        let block: Block = serde_json::from_str(&line).map_err(|e| format!("parse: {}", e))?;
        if block.header.height >= from_height {
            blocks.push(block);
        }
    }
    Ok(blocks)
}

fn ensure_dir() -> std::io::Result<()> {
    fs::create_dir_all(data_dir())
}

pub fn save_utxo_set(state: &UtxoSet) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    let json = serde_json::to_string_pretty(state).map_err(|e| format!("serializar: {}", e))?;
    let path = format!("{}/utxo.json", data_dir());
    let tmp = format!("{}/utxo.json.tmp", data_dir());
    fs::write(&tmp, &json).map_err(|e| format!("escrever: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {}", e))?;
    Ok(())
}

pub fn load_utxo_set() -> Result<UtxoSet, String> {
    let data =
        fs::read_to_string(format!("{}/utxo.json", data_dir())).map_err(|e| format!("ler: {}", e))?;
    serde_json::from_str(&data).map_err(|e| format!("parse: {}", e))
}

pub fn save_block(block: &Block) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    let json = serde_json::to_string(block).map_err(|e| format!("serializar: {}", e))?;
    let path = format!("{}/blocks.jsonl", data_dir());

    // Append-only write with fsync. No tmp+rename for append logs —
    // that would overwrite all previous blocks with just the latest one.
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("abrir: {}", e))?;
    writeln!(file, "{}", json).map_err(|e| format!("escrever: {}", e))?;
    file.flush().map_err(|e| format!("flush: {}", e))?;
    file.sync_data().map_err(|e| format!("sync: {}", e))?;

    // Append to cache if it's loaded, then truncate to MAX_CACHED_BLOCKS
    if let Ok(mut cache) = BLOCK_CACHE.lock() {
        if let Some(ref mut blocks) = *cache {
            blocks.push(block.clone());
            truncate_cache(blocks);
        }
    }

    Ok(())
}

pub fn load_blocks() -> Result<Vec<Block>, String> {
    // Try cache first
    if let Ok(cache) = BLOCK_CACHE.lock() {
        if let Some(ref blocks) = *cache {
            return Ok(blocks.clone());
        }
    }

    // Cache miss: load from disk and populate cache
    let path = format!("{}/blocks.jsonl", data_dir());
    if !Path::new(&path).exists() {
        return Ok(vec![]);
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("ler: {}", e))?;
    let mut blocks = Vec::new();
    for line in data.lines() {
        if !line.trim().is_empty() {
            blocks.push(serde_json::from_str(line).map_err(|e| format!("parse: {}", e))?);
        }
    }

    // Populate cache — only keep the most recent MAX_CACHED_BLOCKS
    if let Ok(mut cache) = BLOCK_CACHE.lock() {
        let mut cached = blocks.clone();
        truncate_cache(&mut cached);
        *cache = Some(cached);
    }

    Ok(blocks)
}

/// Prune blocks from disk that are before a given height.
/// Reads all blocks, filters out old ones, writes back (tmp + rename).
/// Does NOT automatically update the cache (caller should invalidate_cache if needed).
pub fn prune_blocks(before_height: u64) -> Result<usize, String> {
    let path = format!("{}/blocks.jsonl", data_dir());
    if !Path::new(&path).exists() {
        return Ok(0);
    }
    let file = fs::File::open(&path).map_err(|e| format!("open: {}", e))?;
    let reader = BufReader::new(file);
    let mut kept = Vec::new();
    let mut pruned = 0usize;
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read: {}", e))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(block) = serde_json::from_str::<Block>(&line) {
            if block.header.height < before_height {
                pruned += 1;
                continue;
            }
        }
        kept.push(line);
    }
    if pruned == 0 {
        return Ok(0);
    }
    // Write back using tmp + atomic rename
    let tmp = format!("{}/blocks.jsonl.tmp", data_dir());
    let mut out = fs::File::create(&tmp).map_err(|e| format!("create tmp: {}", e))?;
    for line in &kept {
        writeln!(out, "{}", line).map_err(|e| format!("write: {}", e))?;
    }
    out.flush().map_err(|e| format!("flush: {}", e))?;
    out.sync_all().map_err(|e| format!("sync: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {}", e))?;

    // Invalidate cache since disk has changed significantly
    invalidate_cache();
    Ok(pruned)
}

pub fn has_data() -> bool {
    Path::new(&data_dir()).exists() && Path::new(&format!("{}/utxo.json", data_dir())).exists()
}

pub fn save_genesis_key(seed: &[u8; 32]) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    fs::write(format!("{}/genesis.key", data_dir()), seed)
        .map_err(|e| format!("write genesis key: {}", e))
}

pub fn load_genesis_key() -> Result<[u8; 32], String> {
    let data = fs::read(format!("{}/genesis.key", data_dir()))
        .map_err(|e| format!("load genesis key: {}", e))?;
    if data.len() != 32 {
        return Err("Invalid genesis key file".into());
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&data);
    Ok(key)
}

pub fn has_genesis_key() -> bool {
    Path::new(&format!("{}/genesis.key", data_dir())).exists()
}

// ── ChainStore persistence (temporary: save/load from JSON) ──

/// Save the chain store to disk (block tree for fork resolution).
pub fn save_chain_store(store: &crate::chain::ChainStore) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    let json = serde_json::to_string(store).map_err(|e| format!("serialize chain: {}", e))?;
    let path = format!("{}/chain_store.json", data_dir());
    let tmp = format!("{}/chain_store.json.tmp", data_dir());
    std::fs::write(&tmp, &json).map_err(|e| format!("write chain: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename chain: {}", e))?;
    Ok(())
}

/// Load the chain store from disk, rebuilding from blocks if needed.
/// Validate a block's internal consistency after loading from disk.
/// Returns Ok(()) if the block passes integrity checks, or an error describing the issue.
pub(crate) fn validate_block_integrity(block: &Block) -> Result<(), String> {
    // 1. Validate merkle_root against actual transaction hashes.
    //    This catches ANY tampering with block transactions (added, removed, modified).
    let mut tx_hashes: Vec<[u8; 32]> = block.body.transactions.iter().map(|tx| tx.hash()).collect();
    if !tx_hashes.is_empty() {
        while tx_hashes.len() > 1 {
            let mut next = Vec::with_capacity((tx_hashes.len() + 1) / 2);
            for i in (0..tx_hashes.len()).step_by(2) {
                let mut h = Keccak256::new();
                h.update(tx_hashes[i]);
                if i + 1 < tx_hashes.len() {
                    h.update(tx_hashes[i + 1]);
                } else {
                    h.update(tx_hashes[i]); // duplicate odd leaf
                }
                next.push(h.finalize().into());
            }
            tx_hashes = next;
        }
        if tx_hashes[0] != block.header.merkle_root {
            return Err(format!(
                "Block #{}: merkle_root mismatch — tx list tampered or corrupted",
                block.header.height
            ));
        }
    }
    // 2. Validate that previous_hash is not self-referential for non-genesis blocks
    if block.header.height > 0 && block.header.previous_hash == block.header.hash() {
        return Err(format!(
            "Block #{}: previous_hash equals own hash — invalid",
            block.header.height
        ));
    }
    // 3. Validate emission_rate is within genesis anchor bounds
    //    Genesis emission: M_MAX x C_min ≈ 2000 eWatt = 2_000_000_000 base units
    //    At mature supply: base_rate < 100 eWatt = 100_000_000 base units
    //    Any emission > 5000 eWatt per block indicates corruption
    let max_emission_per_block = 5_000_000_000u64; // 5000 eWatt
    if block.header.emission_rate > max_emission_per_block {
        return Err(format!(
            "Block #{}: emission_rate {} exceeds maximum ({}), possible corruption",
            block.header.height, block.header.emission_rate, max_emission_per_block
        ));
    }
    // 4. Validate proof_hash is non-zero (basic sanity)
    if block.header.height > 0 && block.proof_hash == [0u8; 32] {
        return Err(format!(
            "Block #{}: proof_hash is zero — block not properly mined",
            block.header.height
        ));
    }
    Ok(())
}

pub fn load_chain_store() -> crate::chain::ChainStore {
    // Try to load from chain_store.json
    let path = format!("{}/chain_store.json", data_dir());
    if Path::new(&path).exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(mut store) = serde_json::from_str::<crate::chain::ChainStore>(&data) {
                if store.has_genesis() {
                    // Populate block_cache from blocks.jsonl (block_cache is #[serde(skip)]
                    // so it's empty after deserialization)
                    // Validate internal integrity for each block before caching
                    if let Ok(blocks) = load_blocks() {
                        // Build a set of known hashes for parent validation
                        let known_hashes: std::collections::HashSet<[u8; 32]> =
                            blocks.iter().map(|b| b.header.hash()).collect();
                        for block in &blocks {
                            let bh = block.header.hash();
                            if store.get_entry(&bh).is_some() {
                                if let Err(e) = validate_block_integrity(block) {
                                    eprintln!("CRITICAL: {} — disk corruption detected. Halting.", e);
                                    std::process::exit(1);
                                }
                                // Validate previous_hash linkage in the fast path too
                                if block.header.height > 0 {
                                    let parent = block.header.previous_hash;
                                    let parent_known = known_hashes.contains(&parent)
                                        || store.get_entry(&parent).is_some();
                                    if !parent_known {
                                        eprintln!(
                                            "CRITICAL: Block #{}: parent {:02x}.. not found in chain — disk corruption detected. Halting.",
                                            block.header.height, parent[0]
                                        );
                                        std::process::exit(1);
                                    }
                                }
                                store.add_block_to_cache(block.clone());
                            }
                        }
                    }
                    return store;
                }
            }
        }
    }

    // Fallback: rebuild from blocks and UTXO set
    let mut store = crate::chain::ChainStore::empty();
    if let Ok(blocks) = load_blocks() {
        for block in &blocks {
            // Validate internal integrity
            if let Err(e) = validate_block_integrity(block) {
                eprintln!("CRITICAL: {} — disk corruption detected. Halting.", e);
                std::process::exit(1);
            }
            let hash = block.header.hash();
            // Skip if we already have it (add_block checks for duplicates)
            if store.get_block(&hash).is_none() {
                // For genesis, we need to add it specially
                if block.header.height == 0 {
                    store = crate::chain::ChainStore::new(block.clone());
                } else if block.header.previous_hash == [0u8; 32] {
                    // Chain start block (first mined block), allow without genesis in store
                    // Also validate that this is a valid first block (not genesis, but height > 0)
                    let _ = store.add_block(block.clone());
                } else if store.get_block(&block.header.previous_hash).is_some() {
                    let _ = store.add_block(block.clone());
                } else {
                    eprintln!(
                        "CRITICAL: Block #{}: parent hash {:02x}.. not found — disk corruption detected. Halting.",
                        block.header.height, block.header.previous_hash[0]
                    );
                    std::process::exit(1);
                }
            }
        }
        // Find heaviest chain: iterate blocks and pick the one with most accumulated work.
        // Using blocks.last() is wrong — the append-only log can have sidechain/orphan
        // blocks appended after the canonical tip, causing the node to mine on a fork
        // after restart.
        let mut best_hash = [0u8; 32];
        let mut best_work: u128 = 0;
        // Check block entries that actually exist in the store (add_block may silently skip
        // orphans and blocks with unknown parents — they get queued separately).
        for block in &blocks {
            let bh = block.header.hash();
            if let Some(entry) = store.get_entry(&bh) {
                if entry.accumulated_work > best_work {
                    best_work = entry.accumulated_work;
                    best_hash = bh;
                }
            }
        }
        if best_hash != [0u8; 32] {
            store.set_chain_tip(&best_hash).ok();
        }
    }
    store
}

pub fn save_miner_key(seed: &[u8; 32]) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    fs::write(format!("{}/miner.key", data_dir()), seed)
        .map_err(|e| format!("write miner key: {}", e))
}

pub fn load_miner_key() -> Result<[u8; 32], String> {
    let data = fs::read(format!("{}/miner.key", data_dir()))
        .map_err(|e| format!("load miner key: {}", e))?;
    if data.len() != 32 {
        return Err("Invalid miner key file".into());
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&data);
    Ok(key)
}

pub fn has_miner_key() -> bool {
    Path::new(&format!("{}/miner.key", data_dir())).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{Block, BlockHeader, BlockBody};
    use crate::constants;

    /// Store tests need serial execution (shared data_dir() = "ewatts_data/" relative to CWD).
    /// Use `cargo test test_store_ -- --test-threads=1` to run.

    /// Helper: create a minimal genesis block
    fn make_genesis() -> Block {
        let header = BlockHeader {
            version: 1, previous_hash: [0; 32], merkle_root: [0; 32],
            timestamp: 0, height: 0, epoch: 0, difficulty_target: 1,
            total_effective_commit: 0, emission_rate: constants::BASE_EMISSION_UNITS,
            miner_effective_commit: 0, vr_block: 0, coinbase_burn: 0,
            nonce: 0, elapsed_ms: 0, proof_merkle_root: None,
        };
        Block { header, body: BlockBody { transactions: vec![], commitments: vec![] }, proof_hash: [0; 32] }
    }

    fn make_block(height: u64, prev: [u8; 32], nonce: u64) -> Block {
        let header = BlockHeader {
            version: 1, previous_hash: prev, merkle_root: [0; 32],
            timestamp: 1000 + height, height, epoch: 0, difficulty_target: 100,
            total_effective_commit: 0, emission_rate: constants::BASE_EMISSION_UNITS,
            miner_effective_commit: 0, vr_block: 0, coinbase_burn: 0,
            nonce, elapsed_ms: 0, proof_merkle_root: None,
        };
        Block { header, body: BlockBody { transactions: vec![], commitments: vec![] }, proof_hash: [nonce as u8; 32] }
    }

    /// Serialisation mutex: prevents parallel store tests from racing on
    /// the global OVERRIDE_DATA_DIR + BLOCK_CACHE.  Each test acquires this
    /// before touching the store and holds it until the test completes.
    static STORE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Run all store tests with: `cargo test test_store_ -- --test-threads=1`
    /// Invalidate global BLOCK_CACHE + point data_dir() to a temp dir.
    /// The cache is a process-global static, so each test must clear it.
    /// Uses set_data_dir() instead of changing CWD to avoid race conditions.
    /// Acquires STORE_TEST_LOCK to serialise across the entire serial module.
    macro_rules! setup_dir { () => {
        let _guard = crate::store::tests::STORE_TEST_LOCK.lock().unwrap();
        invalidate_cache();
        let _dir = tempfile::tempdir().unwrap();
        set_data_dir(_dir.path().to_str().unwrap().to_string());
    } }

    mod serial {
        use super::*;

        #[test]
        fn test_store_block_count_empty() {
            setup_dir!();
            assert_eq!(crate::store::block_count().unwrap(), 0);
        }

        #[test]
        fn test_store_save_and_count_block() {
            setup_dir!();
            save_block(&make_genesis()).unwrap();
            assert_eq!(block_count().unwrap(), 1);
        }

        #[test]
        fn test_store_load_blocks_roundtrip() {
            setup_dir!();
            let b1 = make_genesis();
            let b2 = make_block(1, b1.header.hash(), 42);
            save_block(&b1).unwrap();
            save_block(&b2).unwrap();
            invalidate_cache();
            let loaded = load_blocks().unwrap();
            assert_eq!(loaded.len(), 2);
            assert_eq!(loaded[0].header.height, 0);
            assert_eq!(loaded[1].header.height, 1);
        }

        #[test]
        fn test_store_get_block_by_hash() {
            setup_dir!();
            let b = make_genesis();
            let hash = b.header.hash();
            save_block(&b).unwrap();
            invalidate_cache();
            let found = get_block_by_hash(&hash);
            assert!(found.is_some());
            assert_eq!(found.unwrap().header.hash(), hash);
            assert!(get_block_by_hash(&[0xff; 32]).is_none());
        }

        #[test]
        fn test_store_load_blocks_since() {
            setup_dir!();
            let b0 = make_genesis();
            let b1 = make_block(1, b0.header.hash(), 1);
            let b2 = make_block(2, b1.header.hash(), 2);
            save_block(&b0).unwrap(); save_block(&b1).unwrap(); save_block(&b2).unwrap();
            invalidate_cache();
            let since = load_blocks_since(1).unwrap();
            assert_eq!(since.len(), 2);
            assert_eq!(since[0].header.height, 1);
        }

        #[test]
        fn test_store_chain_tip_height() {
            setup_dir!();
            assert!(chain_tip_height().is_none());
            save_block(&make_genesis()).unwrap();
            assert_eq!(chain_tip_height(), Some(0));
        }

        #[test]
        fn test_store_prune_blocks() {
            setup_dir!();
            let b0 = make_genesis();
            let b1 = make_block(1, b0.header.hash(), 1);
            let b2 = make_block(2, b1.header.hash(), 2);
            let b3 = make_block(3, b2.header.hash(), 3);
            save_block(&b0).unwrap(); save_block(&b1).unwrap(); save_block(&b2).unwrap(); save_block(&b3).unwrap();
            invalidate_cache();
            assert_eq!(prune_blocks(2).unwrap(), 2);
            invalidate_cache();
            let remaining = load_blocks().unwrap();
            assert_eq!(remaining.len(), 2);
        }

        #[test]
        fn test_store_utxo_roundtrip() {
            setup_dir!();
            let state = crate::state::UtxoSet::genesis(1_000_000, &[0xab; 32]);
            save_utxo_set(&state).unwrap();
            assert_eq!(load_utxo_set().unwrap().total_supply(), 1_000_000);
        }

        #[test]
        fn test_store_genesis_key_roundtrip() {
            setup_dir!();
            let seed = [0x42; 32];
            save_genesis_key(&seed).unwrap();
            assert!(has_genesis_key());
            assert_eq!(load_genesis_key().unwrap(), seed);
        }

        #[test]
        fn test_store_miner_key_roundtrip() {
            setup_dir!();
            let seed = [0x99; 32];
            save_miner_key(&seed).unwrap();
            assert!(has_miner_key());
            assert_eq!(load_miner_key().unwrap(), seed);
        }

        #[test]
        fn test_store_save_chain_store_roundtrip() {
            setup_dir!();
            let genesis = make_genesis();
            let store = crate::chain::ChainStore::new(genesis);
            save_chain_store(&store).unwrap();
            let loaded = load_chain_store();
            assert_eq!(loaded.block_count(), 1);
        }

        #[test]
        fn test_store_block_count_fast_vs_disk() {
            setup_dir!();
            save_block(&make_genesis()).unwrap();
            for i in 1..50 {
                save_block(&make_block(i, [i as u8; 32], i as u64)).unwrap();
            }
            assert_eq!(cached_block_count(), 50);
        }

        #[test]
        fn test_store_prune_noop_when_none_to_prune() {
            setup_dir!();
            save_block(&make_genesis()).unwrap();
            assert_eq!(prune_blocks(0).unwrap(), 0);
        }

        #[test]
        fn test_store_multiple_saves_increment_count() {
            setup_dir!();
            save_block(&make_genesis()).unwrap();
            for i in 0..5 {
                save_block(&make_block(i + 1, [i as u8; 32], i as u64)).unwrap();
            }
            assert_eq!(block_count().unwrap(), 6);
        }

        #[test]
        fn test_store_latest_block_hash() {
            setup_dir!();
            assert!(latest_block_hash().unwrap().is_none());
            let b0 = make_genesis();
            save_block(&b0).unwrap();
            assert_eq!(latest_block_hash().unwrap(), Some(b0.header.hash()));
        }

        #[test]
        fn test_store_has_data() {
            setup_dir!();
            assert!(!has_data());
            let state = crate::state::UtxoSet::genesis(1_000_000, &[0; 32]);
            save_utxo_set(&state).unwrap();
            assert!(has_data());
        }

        #[test]
        fn test_store_cache_eviction_order() {
            setup_dir!();
            let b0 = make_genesis();
            save_block(&b0).unwrap();
            let mut prev = b0.header.hash();
            for i in 1..20 {
                let b = make_block(i, prev, i as u64);
                prev = b.header.hash();
                save_block(&b).unwrap();
            }
            let loaded = load_blocks().unwrap();
            for i in 0..loaded.len()-1 {
                assert_eq!(loaded[i+1].header.previous_hash, loaded[i].header.hash());
            }
        }

        #[test]
        fn test_store_validate_block_integrity() {
            let mut b = make_genesis();
            assert!(validate_block_integrity(&b).is_ok());
            b.header.previous_hash = b.header.hash();
            b.header.height = 1;
            assert!(validate_block_integrity(&b).is_err());
            let mut b2 = make_block(1, [0xab; 32], 1);
            b2.proof_hash = [0; 32];
            assert!(validate_block_integrity(&b2).is_err());
        }
    }
}
