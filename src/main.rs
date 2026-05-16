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

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match cmd {
        "init" => cmd_init(),
        "balance" => cmd_balance(&args),
        "info" => cmd_info(),
        _ => cmd_help(),
    }
}

fn cmd_help() {
    println!("Ewatts Protocol v{}", crate::constants::PROTOCOL_VERSION);
    println!();
    println!("Commands:");
    println!("  init              Create genesis state (fresh start)");
    println!("  balance <pubkey>  Show balance for a public key (hex)");
    println!("  info              Show node status");
    println!("  help              Show this help");
}

fn cmd_init() {
    if crate::store::has_data() {
        println!("Node already has data. Delete ewatts_data/ to reinitialize.");
        return;
    }
    // Genesis: 1 billion Ewatts to a default key (all zeros for testing)
    let pubkey = [0u8; 32];
    let utxo_set = crate::state::UtxoSet::genesis(100_000_000_000_000, &pubkey);
    if let Err(e) = crate::store::save_utxo_set(&utxo_set) {
        println!("Error saving genesis: {}", e);
    } else {
        println!("Genesis created.");
        println!("  Total supply: 1,000,000 Ewatt");
        println!("  Genesis key:  {}", hex::encode(&pubkey));
        println!("  Balance:      {} base units", utxo_set.get_balance(&pubkey));
    }
}

fn cmd_balance(args: &[String]) {
    if args.len() < 3 {
        println!("Usage: ewatts balance <pubkey_hex>");
        return;
    }
    let pk_hex = &args[2];
    let pk_bytes = match hex::decode(pk_hex) {
        Ok(b) if b.len() == 32 => { let mut pk = [0u8; 32]; pk.copy_from_slice(&b); pk.to_vec() }
        _ => { println!("Invalid public key. Must be 32 bytes (64 hex chars)."); return; }
    };
    match crate::store::load_utxo_set() {
        Ok(state) => println!("Balance: {} base units", state.get_balance(&pk_bytes)),
        Err(e) => println!("Error loading state: {}", e),
    }
}

fn cmd_info() {
    if !crate::store::has_data() {
        println!("No data. Run 'ewatts init' first.");
        return;
    }
    match crate::store::load_utxo_set() {
        Ok(state) => {
            let blocks = crate::store::load_blocks().unwrap_or_default();
            println!("Ewatts Node");
            println!("  Blocks:    {}", blocks.len());
            println!("  UTXOs:     {}", state.utxo_count());
            println!("  Supply:    {} base units", state.total_supply());
        }
        Err(e) => println!("Error loading state: {}", e),
    }
}
