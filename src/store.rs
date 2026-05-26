use crate::block::Block;
use crate::state::UtxoSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

const DATA_DIR: &str = "ewatts_data";

/// In-memory block cache to avoid O(N) load_blocks on every gossip event.
/// Invalidated whenever a new block is saved.
static BLOCK_CACHE: Mutex<Option<Vec<Block>>> = Mutex::new(None);

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
    // Fall back to disk
    load_blocks().unwrap_or_default().len()
}

/// Return the tip height (0-indexed) from cache. Returns None if no blocks.
pub fn chain_tip_height() -> Option<u64> {
    if let Ok(cache) = BLOCK_CACHE.lock() {
        if let Some(blocks) = &*cache {
            if blocks.is_empty() {
                return None;
            }
            return Some(blocks.len() as u64 - 1);
        }
    }
    // Fall back to disk
    let blocks = load_blocks().unwrap_or_default();
    if blocks.is_empty() {
        None
    } else {
        Some(blocks.len() as u64 - 1)
    }
}

/// Return the chain tip block hash. Returns None if no blocks.
pub fn chain_tip_hash() -> Option<[u8; 32]> {
    if let Ok(cache) = BLOCK_CACHE.lock() {
        if let Some(blocks) = &*cache {
            return blocks.last().map(|b| b.header.hash());
        }
    }
    let blocks = load_blocks().unwrap_or_default();
    blocks.last().map(|b| b.header.hash())
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

    // Append to cache if it's loaded
    if let Ok(mut cache) = BLOCK_CACHE.lock() {
        if let Some(ref mut blocks) = *cache {
            blocks.push(block.clone());
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

    // Populate cache
    if let Ok(mut cache) = BLOCK_CACHE.lock() {
        *cache = Some(blocks.clone());
    }

    Ok(blocks)
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
            if let Ok(store) = serde_json::from_str::<crate::chain::ChainStore>(&data) {
                if store.has_genesis() {
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
