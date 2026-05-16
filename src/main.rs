pub mod constants;
pub mod dag;
pub mod proof;
pub mod commitment;
pub mod vr;
pub mod block;
pub mod reward;
pub mod difficulty;
pub mod state;
pub mod store;

use std::env;
use rand::RngCore;

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    match cmd {
        "init" => cmd_init(),
        "mine" => cmd_mine(),
        "balance" => cmd_balance(&args),
        "send" => cmd_send(&args),
        "keygen" => cmd_keygen(),
        "info" => cmd_info(),
        _ => cmd_help(),
    }
}

fn cmd_help() {
    println!("Ewatts Protocol v{}", crate::constants::PROTOCOL_VERSION);
    println!();
    println!("Commands:");
    println!("  init                    Create genesis state");
    println!("  mine                    Mine a block (testnet DAG)");
    println!("  balance <pubkey_hex>    Show balance");
    println!("  send <to_pubkey_hex> <amount>  Send from genesis key");
    println!("  keygen                  Generate a new keypair");
    println!("  info                    Show node status");
    println!("  help                    Show this help");
}

fn cmd_init() {
    if crate::store::has_data() {
        println!("Already initialized. Delete ewatts_data/ to reset.");
        return;
    }
    let pubkey = [0u8; 32];
    let utxo_set = crate::state::UtxoSet::genesis(100_000_000_000_000, &pubkey);
    if let Err(e) = crate::store::save_utxo_set(&utxo_set) {
        println!("Error: {}", e);
    } else {
        println!("Genesis: 1,000,000 Ewatt to 00..00");
    }
}

fn cmd_keygen() {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
    let pk = sk.verifying_key().to_bytes();
    println!("Secret key: {}", hex::encode(seed));
    println!("Public key: {}", hex::encode(pk));
}

fn cmd_send(args: &[String]) {
    if args.len() < 4 { println!("Usage: ewatts send <to_pubkey_hex> <amount>"); return; }
    let to_hex = &args[2];
    let amount: u64 = match args[3].parse() { Ok(a) => a, _ => { println!("Invalid amount"); return; } };
    let to_pk = match hex::decode(to_hex) {
        Ok(b) if b.len() == 32 => { let mut pk = [0u8; 32]; pk.copy_from_slice(&b); pk.to_vec() }
        _ => { println!("Invalid pubkey"); return; }
    };
    let mut state = match crate::store::load_utxo_set() {
        Ok(s) => s, Err(e) => { println!("Error loading: {}", e); return; }
    };
    // Genesis key (all zeros) signs the transaction
    let sk = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let from_pk = sk.verifying_key().to_bytes().to_vec();
    let balance = state.get_balance(&from_pk);
    if balance < amount { println!("Insufficient balance. Have: {}", balance); return; }
    // Create transaction: spend all genesis UTXOs, send amount to recipient
    let utxo_keys: Vec<crate::state::UtxoKey> = state.utxo_keys_for(&from_pk);
    if utxo_keys.is_empty() { println!("No UTXOs to spend"); return; }
    let mut total_input = 0u64;
    let mut inputs = Vec::new();
    let mut key_images = Vec::new();
    for key in &utxo_keys {
        let entry = state.get_utxo(key).unwrap();
        total_input += entry.amount;
        let mut ki = [0u8; 32]; rand::thread_rng().fill_bytes(&mut ki);
        inputs.push(crate::block::TxInput {
            previous_tx_hash: key.tx_hash,
            output_index: key.output_index,
            key_image: ki,
        });
        key_images.push(ki);
        if total_input >= amount { break; }
    }
    let mut outputs = vec![crate::block::TxOutput { amount, public_key: to_pk }];
    if total_input > amount {
        outputs.push(crate::block::TxOutput { amount: total_input - amount, public_key: from_pk });
    }
    let mut tx = crate::block::Transaction {
        version: 1, inputs, outputs, ring_size: 1, signatures: vec![],
    };
    let msg = crate::state::tx_msg(&tx);
    let sig = sk.sign(&msg);
    tx.signatures = vec![sig.to_bytes().to_vec()];
    if let Err(e) = state.validate_transaction(&tx) {
        println!("Validation failed: {}", e); return;
    }
    if let Err(e) = state.spend_transaction_inputs(&tx) {
        println!("Spend failed: {}", e); return;
    }
    let tx_hash = tx.hash();
    state.add_transaction_outputs(&tx_hash, &tx, 0, 0);
    if let Err(e) = crate::store::save_utxo_set(&state) {
        println!("Save failed: {}", e);
    } else {
        println!("Sent {} to {}", amount, hex::encode(&args[2]));
        println!("Tx hash: {}", hex::encode(&tx_hash[..8]));
    }
}

fn cmd_mine() {
    if !crate::store::has_data() { println!("No data. Run init first."); return; }
    println!("Generating DAG (4MB testnet)...");
    let dag = crate::dag::Dag::generate_with_size(0, 4 * 1024 * 1024);
    println!("DAG: {} elements. Mining...", dag.len());
    match crate::proof::mine(&[0xbbu8; 32], 1000, &dag, 5000) {
        Some(sol) => {
            println!("Block mined! Nonce: {}, {}ms", sol.nonce, sol.elapsed_ms);
            let r = crate::proof::WorkReport::from_solution(&sol);
            println!("Work: {:.4} GB at {:.2} GB/s", r.gb_processed, r.gbps);
        }
        None => println!("No solution found."),
    }
}

fn cmd_balance(args: &[String]) {
    if args.len() < 3 { println!("Usage: ewatts balance <pubkey_hex>"); return; }
    let pk_hex = &args[2];
    let pk_bytes = match hex::decode(pk_hex) {
        Ok(b) if b.len() == 32 => { let mut pk = [0u8; 32]; pk.copy_from_slice(&b); pk.to_vec() }
        _ => { println!("Invalid key. 64 hex chars."); return; }
    };
    match crate::store::load_utxo_set() {
        Ok(state) => println!("Balance: {}", state.get_balance(&pk_bytes)),
        Err(e) => println!("Error: {}", e),
    }
}

fn cmd_info() {
    if !crate::store::has_data() { println!("No data."); return; }
    match crate::store::load_utxo_set() {
        Ok(state) => {
            let blocks = crate::store::load_blocks().unwrap_or_default();
            println!("Ewatts Node");
            println!("  Blocks: {}", blocks.len());
            println!("  UTXOs:  {}", state.utxo_count());
            println!("  Supply: {}", state.total_supply());
        }
        Err(e) => println!("Error: {}", e),
    }
}
