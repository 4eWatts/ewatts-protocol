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
        "mine" => cmd_mine(),
        "balance" => cmd_balance(&args),
        "info" => cmd_info(),
        _ => cmd_help(),
    }
}

fn cmd_help() {
    println!("Ewatts Protocol v{}", crate::constants::PROTOCOL_VERSION);
    println!();
    println!("Commands:");
    println!("  init              Create genesis state");
    println!("  mine              Mine a block (testnet DAG: 256MB)");
    println!("  balance <pubkey>  Show balance (hex)");
    println!("  info              Show node status");
    println!("  help              Show this help");
}

fn cmd_init() {
    if crate::store::has_data() {
        println!("Node already has data. Delete ewatts_data/ to reinitialize.");
        return;
    }
    let pubkey = [0u8; 32];
    let utxo_set = crate::state::UtxoSet::genesis(100_000_000_000_000, &pubkey);
    if let Err(e) = crate::store::save_utxo_set(&utxo_set) {
        println!("Error: {}", e);
    } else {
        println!("Genesis created. 1,000,000 Ewatt to address 00..00");
    }
}

fn cmd_mine() {
    if !crate::store::has_data() {
        println!("No data. Run 'ewatts init' first.");
        return;
    }
    println!("Generating DAG (256MB testnet size)...");
    let dag = crate::dag::Dag::generate_with_size(0, 4 * 1024 * 1024);
    println!("DAG generated ({} elements). Mining...", dag.len());

    let header = [0xbbu8; 32];
    match crate::proof::mine(&header, 1, &dag, 500) {
        Some(sol) => {
            println!("Block mined! Nonce: {}, elapsed: {}ms", sol.nonce, sol.elapsed_ms);
            let report = crate::proof::WorkReport::from_solution(&sol);
            println!("Work: {:.2} GB at {:.2} GB/s", report.gb_processed, report.gbps);
            let vr = crate::vr::compute_vr(report.gbps, 100.0, 1, 600);
            println!("VR: {:.8} kWh/Ewatt", vr.vr_kwh_per_ewatt);
        }
        None => println!("No solution found (try more attempts)."),
    }
}

fn cmd_balance(args: &[String]) {
    if args.len() < 3 { println!("Usage: ewatts balance <pubkey_hex>"); return; }
    let pk_hex = &args[2];
    let pk_bytes = match hex::decode(pk_hex) {
        Ok(b) if b.len() == 32 => { let mut pk = [0u8; 32]; pk.copy_from_slice(&b); pk.to_vec() }
        _ => { println!("Invalid key. Must be 64 hex chars."); return; }
    };
    match crate::store::load_utxo_set() {
        Ok(state) => println!("Balance: {} base units", state.get_balance(&pk_bytes)),
        Err(e) => println!("Error: {}", e),
    }
}

fn cmd_info() {
    if !crate::store::has_data() { println!("No data. Run 'ewatts init' first."); return; }
    match crate::store::load_utxo_set() {
        Ok(state) => {
            let blocks = crate::store::load_blocks().unwrap_or_default();
            println!("Ewatts Node");
            println!("  Blocks: {}", blocks.len());
            println!("  UTXOs:  {}", state.utxo_count());
            println!("  Supply: {} base units", state.total_supply());
        }
        Err(e) => println!("Error: {}", e),
    }
}
