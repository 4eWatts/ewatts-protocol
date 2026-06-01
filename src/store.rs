use crate::block::Block;
use crate::state::UtxoSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Mutex;

const DATA_DIR: &str = "ewatts_data";

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
    let path = format!("{}/blocks.jsonl", DATA_DIR);
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
    let path = format!("{}/blocks.jsonl", DATA_DIR);
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
    let path = format!("{}/blocks.jsonl", DATA_DIR);
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
    let path = format!("{}/blocks.jsonl", DATA_DIR);
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
    fs::create_dir_all(DATA_DIR)
}

pub fn save_utxo_set(state: &UtxoSet) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    let json = serde_json::to_string_pretty(state).map_err(|e| format!("serializar: {}", e))?;
    let path = format!("{}/utxo.json", DATA_DIR);
    let tmp = format!("{}/utxo.json.tmp", DATA_DIR);
    fs::write(&tmp, &json).map_err(|e| format!("escrever: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename: {}", e))?;
    Ok(())
}

pub fn load_utxo_set() -> Result<UtxoSet, String> {
    let data =
        fs::read_to_string(format!("{}/utxo.json", DATA_DIR)).map_err(|e| format!("ler: {}", e))?;
    serde_json::from_str(&data).map_err(|e| format!("parse: {}", e))
}

pub fn save_block(block: &Block) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    let json = serde_json::to_string(block).map_err(|e| format!("serializar: {}", e))?;
    let path = format!("{}/blocks.jsonl", DATA_DIR);

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
    let path = format!("{}/blocks.jsonl", DATA_DIR);
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
    let path = format!("{}/blocks.jsonl", DATA_DIR);
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
    let tmp = format!("{}/blocks.jsonl.tmp", DATA_DIR);
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
    Path::new(DATA_DIR).exists() && Path::new(&format!("{}/utxo.json", DATA_DIR)).exists()
}

pub fn save_genesis_key(seed: &[u8; 32]) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    fs::write(format!("{}/genesis.key", DATA_DIR), seed)
        .map_err(|e| format!("write genesis key: {}", e))
}

pub fn load_genesis_key() -> Result<[u8; 32], String> {
    let data = fs::read(format!("{}/genesis.key", DATA_DIR))
        .map_err(|e| format!("load genesis key: {}", e))?;
    if data.len() != 32 {
        return Err("Invalid genesis key file".into());
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&data);
    Ok(key)
}

pub fn has_genesis_key() -> bool {
    Path::new(&format!("{}/genesis.key", DATA_DIR)).exists()
}

// ── ChainStore persistence (temporary: save/load from JSON) ──

/// Save the chain store to disk (block tree for fork resolution).
pub fn save_chain_store(store: &crate::chain::ChainStore) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    let json = serde_json::to_string(store).map_err(|e| format!("serialize chain: {}", e))?;
    let path = format!("{}/chain_store.json", DATA_DIR);
    let tmp = format!("{}/chain_store.json.tmp", DATA_DIR);
    std::fs::write(&tmp, &json).map_err(|e| format!("write chain: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename chain: {}", e))?;
    Ok(())
}

/// Load the chain store from disk, rebuilding from blocks if needed.
pub fn load_chain_store() -> crate::chain::ChainStore {
    // Try to load from chain_store.json
    let path = format!("{}/chain_store.json", DATA_DIR);
    if Path::new(&path).exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(mut store) = serde_json::from_str::<crate::chain::ChainStore>(&data) {
                if store.has_genesis() {
                    // Populate block_cache from blocks.jsonl (block_cache is #[serde(skip)]
                    // so it's empty after deserialization)
                    if let Ok(blocks) = load_blocks() {
                        for block in &blocks {
                            let bh = block.header.hash();
                            if store.get_entry(&bh).is_some() {
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
            let hash = block.header.hash();
            // Skip if we already have it (add_block checks for duplicates)
            if store.get_block(&hash).is_none() {
                // For genesis, we need to add it specially
                if block.header.height == 0 {
                    store = crate::chain::ChainStore::new(block.clone());
                } else if block.header.previous_hash == [0u8; 32] {
                    // Chain start block (first mined block), allow without genesis in store
                    let _ = store.add_block(block.clone());
                } else if store.get_block(&block.header.previous_hash).is_some() {
                    let _ = store.add_block(block.clone());
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
    fs::write(format!("{}/miner.key", DATA_DIR), seed)
        .map_err(|e| format!("write miner key: {}", e))
}

pub fn load_miner_key() -> Result<[u8; 32], String> {
    let data = fs::read(format!("{}/miner.key", DATA_DIR))
        .map_err(|e| format!("load miner key: {}", e))?;
    if data.len() != 32 {
        return Err("Invalid miner key file".into());
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&data);
    Ok(key)
}

pub fn has_miner_key() -> bool {
    Path::new(&format!("{}/miner.key", DATA_DIR)).exists()
}
