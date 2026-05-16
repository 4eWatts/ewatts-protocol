pub mod block;
pub mod commitment;
pub mod constants;
pub mod dag;
pub mod difficulty;
pub mod proof;
pub mod reward;
pub mod state;
pub mod store;
pub mod vr;
pub mod wallet;

use ed25519_dalek::Signer;
use rand::RngCore;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    match cmd {
        "init" => cmd_init(),
        "mine" => cmd_mine(&args),
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
    println!("  mine [pubkey_hex]       Mine a block (testnet DAG)");
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
    if crate::store::has_data() {
        println!("Already initialized.");
        return;
    }
    let sk = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let pubkey = sk.verifying_key().to_bytes();
    let utxo_set = crate::state::UtxoSet::genesis(100_000_000_000_000, &pubkey);
    if let Err(e) = crate::store::save_utxo_set(&utxo_set) {
        println!("Error: {}", e);
    } else {
        println!("Genesis: 1,000,000 Ewatt to {}", hex::encode(pubkey));
    }
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
            if keys.is_empty() {
                println!("Wallet is empty.");
                return;
            }
            let state = crate::store::load_utxo_set().ok();
            for (i, (_sec, pk)) in keys.iter().enumerate() {
                let bal = state.as_ref().map(|s| s.get_balance(pk)).unwrap_or(0);
                println!("[{}] {}  Balance: {}", i, hex::encode(pk), bal);
            }
        }
        "send" => {
            if args.len() < 6 {
                println!("Usage: wallet send <idx> <to_pk> <amount>");
                return;
            }
            let idx: usize = match args[3].parse() {
                Ok(i) => i,
                _ => {
                    println!("Invalid index");
                    return;
                }
            };
            let to_hex = &args[4];
            let amount: u64 = match args[5].parse() {
                Ok(a) => a,
                _ => {
                    println!("Invalid amount");
                    return;
                }
            };
            let to_pk = match hex::decode(to_hex) {
                Ok(b) if b.len() == 32 => b,
                _ => {
                    println!("Invalid pubkey");
                    return;
                }
            };
            let keys = crate::wallet::load_keys();
            if idx >= keys.len() {
                println!("Invalid index");
                return;
            }
            let (secret, from_pk) = &keys[idx];
            let mut state = match crate::store::load_utxo_set() {
                Ok(s) => s,
                Err(e) => {
                    println!("Error: {}", e);
                    return;
                }
            };
            let balance = state.get_balance(from_pk);
            if balance < amount {
                println!("Insufficient. Have: {}", balance);
                return;
            }
            let keys_to_spend: Vec<crate::state::UtxoKey> = state.utxo_keys_for(from_pk);
            if keys_to_spend.is_empty() {
                println!("No UTXOs");
                return;
            }
            let mut total_input = 0u64;
            let mut inputs = Vec::new();
            for k in &keys_to_spend {
                let entry = state.get_utxo(k).unwrap();
                total_input += entry.amount;
                let mut ki = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut ki);
                inputs.push(crate::block::TxInput {
                    previous_tx_hash: k.tx_hash,
                    output_index: k.output_index,
                    key_image: ki,
                });
                if total_input >= amount {
                    break;
                }
            }
            let mut outputs = vec![crate::block::TxOutput {
                amount,
                public_key: to_pk,
            }];
            if total_input > amount {
                outputs.push(crate::block::TxOutput {
                    amount: total_input - amount,
                    public_key: from_pk.clone(),
                });
            }
            let sk = ed25519_dalek::SigningKey::from_bytes(secret);
            let mut tx = crate::block::Transaction {
                version: 1,
                inputs,
                outputs,
                ring_size: 1,
                signatures: vec![],
            };
            let sig = sk.sign(&crate::state::tx_msg(&tx));
            tx.signatures = vec![sig.to_bytes().to_vec()];
            if let Err(e) = state.spend_transaction_inputs(&tx) {
                println!("Spend failed: {}", e);
                return;
            }
            let h = tx.hash();
            state.add_transaction_outputs(&h, &tx, 0, 0);
            if let Err(e) = crate::store::save_utxo_set(&state) {
                println!("Save: {}", e);
            } else {
                println!(
                    "Sent {} to {}. Tx: {}",
                    amount,
                    &args[4][..16],
                    hex::encode(&h[..8])
                );
            }
        }
        _ => println!("wallet new | list | send <idx> <to> <amt>"),
    }
}

fn cmd_send(args: &[String]) {
    if args.len() < 4 {
        println!("Usage: send <to_pk> <amount>");
        return;
    }
    let to_pk = match hex::decode(&args[2]) {
        Ok(b) if b.len() == 32 => b,
        _ => {
            println!("Invalid key");
            return;
        }
    };
    let amount: u64 = match args[3].parse() {
        Ok(a) => a,
        _ => {
            println!("Invalid amount");
            return;
        }
    };
    let mut state = match crate::store::load_utxo_set() {
        Ok(s) => s,
        Err(e) => {
            println!("Error: {}", e);
            return;
        }
    };
    let sk = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let from_pk = sk.verifying_key().to_bytes().to_vec();
    let balance = state.get_balance(&from_pk);
    if balance < amount {
        println!("Insufficient. Have: {}", balance);
        return;
    }
    let keys = state.utxo_keys_for(&from_pk);
    let mut total_input = 0u64;
    let mut inputs = Vec::new();
    for k in &keys {
        let e = state.get_utxo(k).unwrap();
        total_input += e.amount;
        let mut ki = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut ki);
        inputs.push(crate::block::TxInput {
            previous_tx_hash: k.tx_hash,
            output_index: k.output_index,
            key_image: ki,
        });
        if total_input >= amount {
            break;
        }
    }
    let mut outputs = vec![crate::block::TxOutput {
        amount,
        public_key: to_pk,
    }];
    if total_input > amount {
        outputs.push(crate::block::TxOutput {
            amount: total_input - amount,
            public_key: from_pk,
        });
    }
    let mut tx = crate::block::Transaction {
        version: 1,
        inputs,
        outputs,
        ring_size: 1,
        signatures: vec![],
    };
    let sig = sk.sign(&crate::state::tx_msg(&tx));
    tx.signatures = vec![sig.to_bytes().to_vec()];
    if let Err(e) = state.spend_transaction_inputs(&tx) {
        println!("Spend failed: {}", e);
        return;
    }
    let h = tx.hash();
    state.add_transaction_outputs(&h, &tx, 0, 0);
    if let Err(e) = crate::store::save_utxo_set(&state) {
        println!("Save: {}", e);
    } else {
        println!("Sent {}. Tx: {}", amount, hex::encode(&h[..8]));
    }
}

fn cmd_mine(args: &[String]) {
    if !crate::store::has_data() {
        println!("No data.");
        return;
    }
    let miner_pk = if args.len() >= 3 {
        match hex::decode(&args[2]) {
            Ok(b) if b.len() == 32 => {
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&b);
                pk
            }
            _ => {
                println!("Invalid pubkey hex");
                return;
            }
        }
    } else {
        let keys = crate::wallet::load_keys();
        if keys.is_empty() {
            println!("No wallet keys. Use 'wallet new' first.");
            return;
        }
        let (_sec, pk) = &keys[0];
        let mut pkb = [0u8; 32];
        pkb.copy_from_slice(pk);
        pkb
    };
    let mut utxo_set = match crate::store::load_utxo_set() {
        Ok(s) => s,
        Err(e) => {
            println!("Error loading state: {}", e);
            return;
        }
    };
    let prev_blocks = crate::store::load_blocks().unwrap_or_default();
    let block_height = prev_blocks.len() as u64;
    let prev_hash = if let Some(last) = prev_blocks.last() {
        last.header.hash()
    } else {
        [0u8; 32]
    };
    let reward = crate::reward::compute_emission_rate(100.0, 100.0) as u64;
    let coinbase = crate::block::Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![crate::block::TxOutput {
            amount: reward,
            public_key: miner_pk.to_vec(),
        }],
        ring_size: 1,
        signatures: vec![],
    };
    let merkle_root = crate::block::merkle_root(&[coinbase.clone()]);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let epoch = 0;
    let difficulty = 1000;
    let header = crate::block::BlockHeader {
        version: crate::constants::PROTOCOL_VERSION,
        previous_hash: prev_hash,
        merkle_root,
        timestamp,
        epoch,
        difficulty_target: difficulty,
        total_effective_commit: 0.0,
        emission_rate: reward as f64,
        miner_effective_commit: 0.0,
        vr_block: 0.0,
        nonce: 0,
        elapsed_ms: 0,
    };
    let header_hash = header.hash();
    println!("Generating DAG (4MB)...");
    let dag = crate::dag::Dag::generate_with_size(epoch, 4 * 1024 * 1024);
    println!(
        "DAG: {} elements. Mining height {}...",
        dag.len(),
        block_height
    );
    match crate::proof::mine(&header_hash, difficulty, &dag, 5000) {
        Some(sol) => {
            let mut mined_header = header.clone();
            mined_header.nonce = sol.nonce;
            mined_header.elapsed_ms = sol.elapsed_ms as u32;
            let block = crate::block::Block {
                header: mined_header,
                body: crate::block::BlockBody {
                    transactions: vec![coinbase],
                    commitments: vec![],
                },
            };
            if let Err(e) = crate::store::save_block(&block) {
                println!("Error saving block: {}", e);
                return;
            }
            let h = block.body.transactions[0].hash();
            utxo_set.add_transaction_outputs(&h, &block.body.transactions[0], block_height, 0);
            utxo_set.add_coinbase_supply(reward);
            if let Err(e) = crate::store::save_utxo_set(&utxo_set) {
                println!("Error saving state: {}", e);
                return;
            }
            println!(
                "Block mined! Height: {} | Nonce: {} | {}ms | Reward: {} Ewatt",
                block_height, sol.nonce, sol.elapsed_ms, reward
            );
            println!("Block hash: {}", hex::encode(mined_header.hash()));
        }
        None => println!("No solution found."),
    }
}

fn cmd_balance(args: &[String]) {
    if args.len() < 3 {
        println!("Usage: balance <pubkey_hex>");
        return;
    }
    let pk = match hex::decode(&args[2]) {
        Ok(b) if b.len() == 32 => b,
        _ => {
            println!("Invalid key");
            return;
        }
    };
    match crate::store::load_utxo_set() {
        Ok(s) => println!("Balance: {}", s.get_balance(&pk)),
        Err(e) => println!("Error: {}", e),
    }
}

fn cmd_info() {
    if !crate::store::has_data() {
        println!("No data.");
        return;
    }
    match crate::store::load_utxo_set() {
        Ok(s) => {
            let blocks = crate::store::load_blocks().unwrap_or_default();
            let height = blocks.len();
            println!("Ewatts Node");
            println!("  Height: {}", height);
            println!("  Supply: {}", s.total_supply());
            println!("  UTXOs:  {}", s.utxo_count());
            if let Some(last) = blocks.last() {
                println!("  Last block: {}", hex::encode(last.header.hash()));
                println!("  Last nonce: {}", last.header.nonce);
                println!("  Last time:  {}", last.header.timestamp);
            }
        }
        Err(e) => println!("Error: {}", e),
    }
}
