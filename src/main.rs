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
pub mod wallet;

use std::env;
use ed25519_dalek::Signer;
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
        "wallet" => cmd_wallet(&args),
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
    println!("  send <to_pk> <amount>   Send from genesis key");
    println!("  keygen                  Generate a keypair");
    println!("  wallet new              Create a new wallet key");
    println!("  wallet list             List wallet keys and balances");
    println!("  wallet send <idx> <to_pk> <amt>  Send from wallet key");
    println!("  info                    Show node status");
    println!("  help                    Show this help");
}

fn cmd_init() {
    if crate::store::has_data() { println!("Already initialized."); return; }
    let sk = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let pubkey = sk.verifying_key().to_bytes();
    let utxo_set = crate::state::UtxoSet::genesis(100_000_000_000_000, &pubkey);
    if let Err(e) = crate::store::save_utxo_set(&utxo_set) { println!("Error: {}", e); }
    else { println!("Genesis: 1,000,000 Ewatt to {}", hex::encode(pubkey)); }
}

fn cmd_keygen() {
    let (secret, pubkey) = crate::wallet::generate_key();
    println!("Secret: {}", hex::encode(secret));
    println!("Public: {}", hex::encode(pubkey));
}

fn cmd_wallet(args: &[String]) {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("help");
    match sub {
        "new" => {
            let (secret, pubkey) = crate::wallet::generate_key();
            crate::wallet::save_key(&secret, &pubkey);
            println!("Key saved to wallet. Public: {}", hex::encode(pubkey));
        }
        "list" => {
            let keys = crate::wallet::load_keys();
            if keys.is_empty() { println!("Wallet is empty."); return; }
            let state = crate::store::load_utxo_set().ok();
            for (i, (sec, pk)) in keys.iter().enumerate() {
                let bal = state.as_ref().map(|s| s.get_balance(pk)).unwrap_or(0);
                println!("[{}] {}  Balance: {}", i, hex::encode(pk), bal);
            }
        }
        "send" => {
            if args.len() < 6 { println!("Usage: wallet send <idx> <to_pk> <amount>"); return; }
            let idx: usize = match args[3].parse() { Ok(i) => i, _ => { println!("Invalid index"); return; } };
            let to_hex = &args[4];
            let amount: u64 = match args[5].parse() { Ok(a) => a, _ => { println!("Invalid amount"); return; } };
            let to_pk = match hex::decode(to_hex) {
                Ok(b) if b.len() == 32 => b, _ => { println!("Invalid pubkey"); return; }
            };
            let keys = crate::wallet::load_keys();
            if idx >= keys.len() { println!("Invalid index"); return; }
            let (secret, from_pk) = &keys[idx];
            let mut state = match crate::store::load_utxo_set() { Ok(s) => s, Err(e) => { println!("Error: {}", e); return; } };
            let balance = state.get_balance(from_pk);
            if balance < amount { println!("Insufficient. Have: {}", balance); return; }
            let keys_to_spend: Vec<crate::state::UtxoKey> = state.utxo_keys_for(from_pk);
            if keys_to_spend.is_empty() { println!("No UTXOs"); return; }
            let mut total_input = 0u64;
            let mut inputs = Vec::new();
            for k in &keys_to_spend {
                let entry = state.get_utxo(k).unwrap();
                total_input += entry.amount;
                let mut ki = [0u8; 32]; rand::thread_rng().fill_bytes(&mut ki);
                inputs.push(crate::block::TxInput { previous_tx_hash: k.tx_hash, output_index: k.output_index, key_image: ki });
                if total_input >= amount { break; }
            }
            let mut outputs = vec![crate::block::TxOutput { amount, public_key: to_pk }];
            if total_input > amount {
                outputs.push(crate::block::TxOutput { amount: total_input - amount, public_key: from_pk.clone() });
            }
            let sk = ed25519_dalek::SigningKey::from_bytes(secret);
            let mut tx = crate::block::Transaction { version: 1, inputs, outputs, ring_size: 1, signatures: vec![] };
            let sig = sk.sign(&crate::state::tx_msg(&tx));
            tx.signatures = vec![sig.to_bytes().to_vec()];
            if let Err(e) = state.spend_transaction_inputs(&tx) { println!("Spend failed: {}", e); return; }
            let h = tx.hash();
            state.add_transaction_outputs(&h, &tx, 0, 0);
            if let Err(e) = crate::store::save_utxo_set(&state) { println!("Save: {}", e); }
            else { println!("Sent {} to {}. Tx: {}", amount, &args[4][..16], hex::encode(&h[..8])); }
        }
        _ => println!("wallet new | list | send <idx> <to> <amt>"),
    }
}

fn cmd_send(args: &[String]) {
    if args.len() < 4 { println!("Usage: send <to_pk> <amount>"); return; }
    let to_pk = match hex::decode(&args[2]) { Ok(b) if b.len() == 32 => b, _ => { println!("Invalid key"); return; } };
    let amount: u64 = match args[3].parse() { Ok(a) => a, _ => { println!("Invalid amount"); return; } };
    let mut state = match crate::store::load_utxo_set() { Ok(s) => s, Err(e) => { println!("Error: {}", e); return; } };
    let sk = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let from_pk = sk.verifying_key().to_bytes().to_vec();
    let balance = state.get_balance(&from_pk);
    if balance < amount { println!("Insufficient. Have: {}", balance); return; }
    let keys = state.utxo_keys_for(&from_pk);
    let mut total_input = 0u64; let mut inputs = Vec::new();
    for k in &keys {
        let e = state.get_utxo(k).unwrap(); total_input += e.amount;
        let mut ki = [0u8; 32]; rand::thread_rng().fill_bytes(&mut ki);
        inputs.push(crate::block::TxInput { previous_tx_hash: k.tx_hash, output_index: k.output_index, key_image: ki });
        if total_input >= amount { break; }
    }
    let mut outputs = vec![crate::block::TxOutput { amount, public_key: to_pk }];
    if total_input > amount { outputs.push(crate::block::TxOutput { amount: total_input - amount, public_key: from_pk }); }
    let mut tx = crate::block::Transaction { version: 1, inputs, outputs, ring_size: 1, signatures: vec![] };
    let sig = sk.sign(&crate::state::tx_msg(&tx));
    tx.signatures = vec![sig.to_bytes().to_vec()];
    if let Err(e) = state.spend_transaction_inputs(&tx) { println!("Spend failed: {}", e); return; }
    let h = tx.hash(); state.add_transaction_outputs(&h, &tx, 0, 0);
    if let Err(e) = crate::store::save_utxo_set(&state) { println!("Save: {}", e); }
    else { println!("Sent {}. Tx: {}", amount, hex::encode(&h[..8])); }
}

fn cmd_mine() {
    if !crate::store::has_data() { println!("No data."); return; }
    println!("Generating DAG (4MB)...");
    let dag = crate::dag::Dag::generate_with_size(0, 4*1024*1024);
    println!("DAG: {} elements. Mining...", dag.len());
    match crate::proof::mine(&[0xbbu8;32], 1000, &dag, 5000) {
        Some(sol) => println!("Block mined! Nonce: {}, {}ms", sol.nonce, sol.elapsed_ms),
        None => println!("No solution."),
    }
}

fn cmd_balance(args: &[String]) {
    if args.len() < 3 { println!("Usage: balance <pubkey_hex>"); return; }
    let pk = match hex::decode(&args[2]) { Ok(b) if b.len() == 32 => b, _ => { println!("Invalid key"); return; } };
    match crate::store::load_utxo_set() { Ok(s) => println!("Balance: {}", s.get_balance(&pk)), Err(e) => println!("Error: {}", e) }
}

fn cmd_info() {
    if !crate::store::has_data() { println!("No data."); return; }
    match crate::store::load_utxo_set() { Ok(s) => {
        let blocks = crate::store::load_blocks().unwrap_or_default();
        println!("Ewatts Node | Blocks: {} | UTXOs: {} | Supply: {}", blocks.len(), s.utxo_count(), s.total_supply());
    } Err(e) => println!("Error: {}", e) }
}
