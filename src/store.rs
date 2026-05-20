use crate::block::Block;
use crate::state::UtxoSet;
use std::fs;
use std::io::Write;
use std::path::Path;

const DATA_DIR: &str = "ewatts_data";

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
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{}/blocks.jsonl", DATA_DIR))
        .map_err(|e| format!("abrir: {}", e))?;
    writeln!(file, "{}", json).map_err(|e| format!("escrever: {}", e))?;
    file.flush().map_err(|e| format!("flush: {}", e))?;
    file.sync_data().map_err(|e| format!("sync: {}", e))?;
    Ok(())
}

pub fn load_blocks() -> Result<Vec<Block>, String> {
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
