use std::fs;
use std::io::Write;
use std::path::Path;
use crate::state::UtxoSet;
use crate::block::Block;

const DATA_DIR: &str = "ewatts_data";

fn ensure_dir() -> std::io::Result<()> {
    fs::create_dir_all(DATA_DIR)
}

pub fn save_utxo_set(state: &UtxoSet) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    let json = serde_json::to_string_pretty(state).map_err(|e| format!("serializar: {}", e))?;
    fs::write(format!("{}/utxo.json", DATA_DIR), &json).map_err(|e| format!("escrever: {}", e))?;
    Ok(())
}

pub fn load_utxo_set() -> Result<UtxoSet, String> {
    let data = fs::read_to_string(format!("{}/utxo.json", DATA_DIR)).map_err(|e| format!("ler: {}", e))?;
    serde_json::from_str(&data).map_err(|e| format!("parse: {}", e))
}

pub fn save_block(block: &Block) -> Result<(), String> {
    ensure_dir().map_err(|e| format!("dir: {}", e))?;
    let json = serde_json::to_string(block).map_err(|e| format!("serializar: {}", e))?;
    let mut file = fs::OpenOptions::new().create(true).append(true)
        .open(format!("{}/blocks.jsonl", DATA_DIR)).map_err(|e| format!("abrir: {}", e))?;
    writeln!(file, "{}", json).map_err(|e| format!("escrever: {}", e))?;
    Ok(())
}

pub fn load_blocks() -> Result<Vec<Block>, String> {
    let path = format!("{}/blocks.jsonl", DATA_DIR);
    if !Path::new(&path).exists() { return Ok(vec![]); }
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
