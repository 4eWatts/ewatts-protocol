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
pub mod p2p;

#[cfg(test)]
pub mod tests;
 
 
 

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use rand::RngCore;
use ed25519_dalek::Signer;

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    match cmd {
        "init" => cmd_init(),
        "mine" => cmd_mine(),
        "simulate" => cmd_simulate(&args),
        "balance" => cmd_balance(&args),
        "send" => cmd_send(&args),
        "keygen" => cmd_keygen(),
        "wallet" => cmd_wallet(&args),
        "info" => cmd_info(),
        "p2p" => cmd_p2p(&args),
        _ => cmd_help(),
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn genesis_keypair() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[0u8; 32])
}

fn cmd_help() {
    println!("Ewatts Protocol v{}", crate::constants::PROTOCOL_VERSION);
    println!();
    println!("Commands:");
    println!("  init                     Create genesis state");
    println!("  mine                     Mine one block (testnet DAG)");
    println!("  simulate <blocks>        Mine N blocks in sequence");
    println!("  balance <pubkey_hex>     Show balance");
    println!("  send <to_pubkey> <amt>   Send from genesis key");
    println!("  keygen                   Generate a new keypair");
    println!("  wallet new               Create a new wallet key");
    println!("  wallet list              List wallet keys and balances");
    println!("  wallet send <idx> <to_pk> <amt>  Send from wallet key");
    println!("  info                     Show node status");
    println!("  p2p [addr] [bootstrap]     Start P2P node");
    println!("  help                     Show this help");
}

fn cmd_init() {
    if crate::store::has_data() {
        println!("Already initialized. Delete ewatts_data/ to reset.");
        return;
    }
    let sk = genesis_keypair();
    let pubkey = sk.verifying_key().to_bytes();
    let utxo_set = crate::state::UtxoSet::genesis(100_000_000_000_000, &pubkey);
    if let Err(e) = crate::store::save_utxo_set(&utxo_set) {
        println!("Error: {}", e);
        return;
    }
    // Also save genesis key in wallet
    let seed = [0u8; 32];
    crate::wallet::save_key(&seed, &pubkey);
    println!("Genesis: 1,000,000 Ewatt to {}", hex::encode(pubkey));
}

fn cmd_keygen() {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
    let pk = sk.verifying_key().to_bytes();
    println!("Secret key: {}", hex::encode(seed));
    println!("Public key: {}", hex::encode(pk));
}

fn miner_keypair() -> ed25519_dalek::SigningKey {
    // Use a deterministic miner key for now (different from genesis)
    let mut seed = [0u8; 32];
    seed[0] = 0x01;
    ed25519_dalek::SigningKey::from_bytes(&seed)
}

pub(crate) fn mine_block(prev_hash: [u8; 32], height: u64, state: &mut crate::state::UtxoSet)
    -> Result<block::Block, String>
{
    use crate::block::*;
    use crate::commitment;

    let epoch = height / constants::DAG_EPOCH_BLOCKS;
    let difficulty: u64 = 100; // testnet fixed

    // Generate testnet DAG (4MB)
    println!("  DAG generation...");
    let dag = crate::dag::Dag::generate_with_size(epoch, 4 * 1024 * 1024);

    // Miner setup
    let sk = miner_keypair();
    let miner_pk = sk.verifying_key().to_bytes();

    // Build header to mine
    let mut header = BlockHeader {
        version: constants::PROTOCOL_VERSION,
        previous_hash: prev_hash,
        merkle_root: [0u8; 32], // TODO: real merkle
        timestamp: now_secs(),
        epoch,
        height,
        difficulty_target: difficulty,
        total_effective_commit: 0.0,    // filled after mining
        emission_rate: 0.0,             // filled after mining
        miner_effective_commit: 0.0,
        vr_block: 0.0,
        nonce: 0,
        elapsed_ms: 0,
    };
    let header_hash = header.hash();

    // Mine
    println!("  Mining (difficulty={})...", difficulty);
    let sol = crate::proof::mine(&header_hash, difficulty, &dag, 50000)
        .ok_or("No solution found")?;

    // Work report
    let wr = crate::proof::WorkReport::from_solution(&sol);
    println!("  Solved! Nonce={}, {:.2} GB at {:.2} GB/s in {}ms",
        sol.nonce, wr.gb_processed, wr.gbps, sol.elapsed_ms);

    // Fill header with solution data
    header.nonce = sol.nonce;
    header.elapsed_ms = sol.elapsed_ms as u32;

    // Create commitment
    let declared_gbps = 50.0; // fixed for testnet (wr.gbps is 0 on small DAG) // what the miner actually delivered
    let mut commit = commitment::Commitment {
        miner_id: miner_pk,
        bandwidth_gbps: declared_gbps,
        block_number: height,
        work_gb: wr.gb_processed.max(0.0001), // min work for fast testnet
        time_seconds: (sol.elapsed_ms.max(1) as f64) / 1000.0,
        signature: vec![],
    };
    let msg = commitment::commit_msg(&commit);
    commit.signature = sk.sign(&msg).to_bytes().to_vec();

    // Validate commitment
    let recent = &[]; // first block — no history
    commitment::validate_commitment(&commit, recent)
        .map_err(|e| format!("Commitment invalid: {}", e))?;

    // Compute effective commitment
    let eff = commitment::compute_efficiency(commit.work_gb, commit.bandwidth_gbps, commit.time_seconds);
    let ce = commitment::effective_commitment(commit.bandwidth_gbps, eff);
    header.miner_effective_commit = ce;

    // Emission rate: single miner, avg_hist = BASE_EMISSION for first block
    let avg_hist = if height == 0 { constants::BASE_EMISSION } else { constants::BASE_EMISSION };
    let total_eff = ce;
    let em = crate::reward::compute_emission_rate(total_eff, avg_hist);
    header.total_effective_commit = total_eff;
    header.emission_rate = em;

    // Reward for this miner, with ramp-up cap (first 10K blocks: max 80%, excess burned)
    let miner_reward = ce / total_eff * em; // = em for solo miner
    let mut reward_list = vec![(miner_pk.to_vec(), miner_reward)];
    let _burned = crate::reward::apply_ramp_up_cap(height, &mut reward_list);
    let post_burn_reward = reward_list[0].1;
    let post_burn_emission = post_burn_reward;
    header.emission_rate = post_burn_emission;

    // VR (use post-burn reward for accuracy)
    let vr_result = crate::vr::compute_vr(ce, post_burn_reward, 1, constants::TARGET_BLOCK_TIME_SECS);
    header.vr_block = vr_result.vr_kwh_per_ewatt;

    // Coinbase transaction: miner reward (post-burn) to miner
    // During ramp-up, up to 20% may be burned (coinbase_burn)
    let reward_base_units = (post_burn_reward * 100_000_000.0) as u64; // 1 Ewatt = 10^8 base
    let coinbase = Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![TxOutput { amount: reward_base_units, public_key: miner_pk.to_vec(), spendable_after: crate::reward::founder_lock_block(height) }],
        ring_size: 1,
        signatures: vec![],
    };

    // Assemble block
    let block = Block {
        header,
        body: BlockBody {
            transactions: vec![coinbase],
            commitments: vec![commit],
        },
    };

    // Apply to UTXO set
    state.apply_block(&block, height)?;

    Ok(block)
}

fn cmd_mine() {
    if !crate::store::has_data() {
        println!("No data. Run init first.");
        return;
    }

    let mut state = match crate::store::load_utxo_set() {
        Ok(s) => s,
        Err(e) => { println!("Error loading state: {}", e); return; }
    };

    // Get last block hash (or genesis zero hash)
    let blocks = crate::store::load_blocks().unwrap_or_default();
    let height = blocks.len() as u64;
    let prev_hash = if height == 0 {
        [0u8; 32]
    } else {
        blocks.last().unwrap().header.hash()
    };

    println!("Mining block #{}...", height);

    match mine_block(prev_hash, height, &mut state) {
        Ok(block) => {
            let block_hash = block.header.hash();

            // Save
            if let Err(e) = crate::store::save_block(&block) {
                println!("Error saving block: {}", e);
                return;
            }
            if let Err(e) = crate::store::save_utxo_set(&state) {
                println!("Error saving state: {}", e);
                return;
            }

            let reward_ewatt = block.body.transactions[0].outputs.iter()
                .map(|o| o.amount).sum::<u64>() as f64 / 100_000_000.0;

            println!();
            println!("Block #{} mined!", height);
            println!("  Hash:   {}", hex::encode(&block_hash[..8]));
            println!("  Reward: {:.2} Ewatt", reward_ewatt);
            println!("  VR:     {}",
                crate::vr::format_vr(block.header.vr_block));
            println!("  UTXOs:  {}", state.utxo_count());
            println!("  Supply: {} base units", state.total_supply());

            // Check genesis miner balance
            let genesis_pk = genesis_keypair().verifying_key().to_bytes().to_vec();
            let miner_pk = miner_keypair().verifying_key().to_bytes().to_vec();
            println!("  Genesis balance: {}", state.get_balance(&genesis_pk));
            println!("  Miner balance:   {}", state.get_balance(&miner_pk));
        }
        Err(e) => println!("Mining failed: {}", e),
    }
}

fn cmd_simulate(args: &[String]) {
    if args.len() < 3 {
        println!("Usage: ewatts simulate <num_blocks>");
        return;
    }
    let n: u64 = match args[2].parse() {
        Ok(v) => v,
        _ => { println!("Invalid number"); return; }
    };

    if !crate::store::has_data() {
        println!("No data. Run init first.");
        return;
    }

    let mut state = match crate::store::load_utxo_set() {
        Ok(s) => s,
        Err(e) => { println!("Error loading state: {}", e); return; }
    };

    let blocks = crate::store::load_blocks().unwrap_or_default();
    let mut height = blocks.len() as u64;
    let mut prev_hash = if height == 0 {
        [0u8; 32]
    } else {
        blocks.last().unwrap().header.hash()
    };

    println!("Simulating {} blocks starting from #{}...", n, height);

    for i in 0..n {
        let current_height = height + i;
        println!("\n--- Block #{} ---", current_height);

        match mine_block(prev_hash, current_height, &mut state) {
            Ok(block) => {
                let hash = block.header.hash();

                if let Err(e) = crate::store::save_block(&block) {
                    println!("Error saving block: {}", e); return;
                }

                prev_hash = hash;
                print!("  ✓ VR: {}", crate::vr::format_vr(block.header.vr_block));
                println!(" | UTXOs: {} | Supply: {}",
                    state.utxo_count(), state.total_supply());
            }
            Err(e) => {
                println!("  ✗ Failed at block {}: {}", current_height, e);
                break;
            }
        }
    }

    // Final save
    if let Err(e) = crate::store::save_utxo_set(&state) {
        println!("Error saving final state: {}", e);
    }

    height += n;
    println!("\n--- Simulation complete ---");
    println!("Total blocks: {}", height);
    println!("UTXOs: {} | Supply: {}", state.utxo_count(), state.total_supply());

    let genesis_pk = genesis_keypair().verifying_key().to_bytes().to_vec();
    let miner_pk = miner_keypair().verifying_key().to_bytes().to_vec();
    println!("Genesis balance: {}", state.get_balance(&genesis_pk));
    println!("Miner balance:   {}", state.get_balance(&miner_pk));
}

fn cmd_send(args: &[String]) {
    if args.len() < 4 {
        println!("Usage: ewatts send <to_pubkey_hex> <amount>");
        return;
    }
    let to_hex = &args[2];
    let amount: u64 = match args[3].parse() {
        Ok(a) => a,
        _ => { println!("Invalid amount"); return; }
    };
    let to_pk = match hex::decode(to_hex) {
        Ok(b) if b.len() == 32 => { let mut pk = [0u8; 32]; pk.copy_from_slice(&b); pk.to_vec() }
        _ => { println!("Invalid pubkey. 64 hex chars."); return; }
    };

    let mut state = match crate::store::load_utxo_set() {
        Ok(s) => s,
        Err(e) => { println!("Error loading: {}", e); return; }
    };

    let sk = genesis_keypair();
    let from_pk = sk.verifying_key().to_bytes().to_vec();
    let balance = state.get_balance(&from_pk);
    if balance < amount {
        println!("Insufficient balance. Have: {}", balance);
        return;
    }

    let utxo_keys: Vec<crate::state::UtxoKey> = state.utxo_keys_for(&from_pk);
    if utxo_keys.is_empty() {
        println!("No UTXOs to spend");
        return;
    }

    let mut total_input = 0u64;
    let mut inputs = Vec::new();
    for key in &utxo_keys {
        let entry = state.get_utxo(key).unwrap();
        total_input += entry.amount;
        let mut ki = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut ki);
        inputs.push(crate::block::TxInput {
            previous_tx_hash: key.tx_hash,
            output_index: key.output_index,
            key_image: ki,
        });
        if total_input >= amount { break; }
    }

    let mut outputs = vec![crate::block::TxOutput { amount, public_key: to_pk, spendable_after: 0 }];
    if total_input > amount {
        outputs.push(crate::block::TxOutput {
            amount: total_input - amount,
            public_key: from_pk, spendable_after: 0,
        });
    }

    let mut tx = crate::block::Transaction {
        version: 1,
        inputs,
        outputs,
        ring_size: 1,
        signatures: vec![],
    };
    let msg = crate::state::tx_msg(&tx);
    let sig = sk.sign(&msg);
    tx.signatures = vec![sig.to_bytes().to_vec()];

    if let Err(e) = state.validate_transaction(&tx) {
        println!("Validation failed: {}", e);
        return;
    }
    if let Err(e) = state.spend_transaction_inputs(&tx, 0) {
        println!("Spend failed: {}", e);
        return;
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

fn cmd_wallet(args: &[String]) {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("help");
    match sub {
        "new" => {
            let (secret, pubkey) = crate::wallet::generate_key();
            crate::wallet::save_key(&secret, &pubkey);
            println!("New wallet key: {}", hex::encode(&pubkey));
        }
        "list" => {
            let keys = crate::wallet::load_keys();
            let state = crate::store::load_utxo_set().ok();
            if keys.is_empty() {
                println!("No wallet keys.");
                return;
            }
            for (i, (_, pk)) in keys.iter().enumerate() {
                let bal = state.as_ref().map(|s| s.get_balance(pk)).unwrap_or(0);
                println!("  [{}] {}  (balance: {})", i, hex::encode(pk), bal);
            }
        }
        "send" => {
            if args.len() < 5 {
                println!("Usage: ewatts wallet send <idx> <to_pk_hex> <amount>");
                return;
            }
            let idx: usize = match args[3].parse() {
                Ok(i) => i,
                _ => { println!("Invalid index"); return; }
            };
            let to_hex = &args[4];
            let amount: u64 = match args.get(5).and_then(|a| a.parse().ok()) {
                Some(a) => a,
                None => { println!("Invalid amount"); return; }
            };
            let to_pk = match hex::decode(to_hex) {
                Ok(b) if b.len() == 32 => { let mut pk = [0u8; 32]; pk.copy_from_slice(&b); pk.to_vec() }
                _ => { println!("Invalid pubkey"); return; }
            };
            let keys = crate::wallet::load_keys();
            let (secret, from_pk) = match keys.get(idx) {
                Some(k) => (k.0, k.1.clone()),
                None => { println!("Key index {} not found", idx); return; }
            };
            let sk = ed25519_dalek::SigningKey::from_bytes(&secret);
            let mut state = match crate::store::load_utxo_set() {
                Ok(s) => s,
                Err(e) => { println!("Error: {}", e); return; }
            };
            let balance = state.get_balance(&from_pk);
            if balance < amount {
                println!("Insufficient balance. Have: {}", balance);
                return;
            }
            let utxo_keys = state.utxo_keys_for(&from_pk);
            if utxo_keys.is_empty() {
                println!("No UTXOs to spend");
                return;
            }
            let mut total_input = 0u64;
            let mut inputs = Vec::new();
            for key in &utxo_keys {
                let entry = state.get_utxo(key).unwrap();
                total_input += entry.amount;
                let mut ki = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut ki);
                inputs.push(crate::block::TxInput {
                    previous_tx_hash: key.tx_hash,
                    output_index: key.output_index,
                    key_image: ki,
                });
                if total_input >= amount { break; }
            }
            let mut outputs = vec![crate::block::TxOutput { amount, public_key: to_pk, spendable_after: 0 }];
            if total_input > amount {
                outputs.push(crate::block::TxOutput {
                    amount: total_input - amount,
                    public_key: from_pk, spendable_after: 0,
                });
            }
            let mut tx = crate::block::Transaction {
                version: 1, inputs, outputs, ring_size: 1, signatures: vec![],
            };
            let msg = crate::state::tx_msg(&tx);
            tx.signatures = vec![sk.sign(&msg).to_bytes().to_vec()];
            if let Err(e) = state.validate_transaction(&tx) {
                println!("Validation: {}", e); return;
            }
            if let Err(e) = state.spend_transaction_inputs(&tx, 0) {
                println!("Spend: {}", e); return;
            }
            let tx_hash = tx.hash();
            state.add_transaction_outputs(&tx_hash, &tx, 0, 0);
            if let Err(e) = crate::store::save_utxo_set(&state) {
                println!("Save: {}", e);
            } else {
                println!("Sent {} from wallet[{}]", amount, idx);
                println!("  Tx: {}", hex::encode(&tx_hash[..8]));
            }
        }
        _ => {
            println!("Wallet commands: new, list, send <idx> <to> <amt>");
        }
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
        _ => { println!("Invalid key. 64 hex chars."); return; }
    };
    match crate::store::load_utxo_set() {
        Ok(state) => println!("Balance: {}", state.get_balance(&pk_bytes)),
        Err(e) => println!("Error: {}", e),
    }
}

#[tokio::main]
async fn cmd_p2p(args: &[String]) {
    let addr = args.get(2).map(|s| s.as_str()).unwrap_or("/ip4/0.0.0.0/tcp/0");
    let bootstrap = args.get(3).and_then(|s| s.parse::<libp2p::Multiaddr>().ok());
    let do_mine = args.iter().any(|s| s == "--mine");
    println!("Starting P2P node on {}...", addr);
    if let Some(ref b) = bootstrap { println!("Bootstrap peer: {}", b); }
    if do_mine { println!("Mining mode: ON (1 block every ~10s)"); }

    // Load or init state
    let mut state = if !crate::store::has_data() {
        cmd_init();
        crate::store::load_utxo_set().unwrap_or_else(|_| crate::state::UtxoSet::new())
    } else {
        crate::store::load_utxo_set().unwrap_or_else(|_| crate::state::UtxoSet::new())
    };

    match crate::p2p::P2pNode::new(addr, bootstrap).await {
        Ok(mut node) => {
            println!("P2P Node ID: {}", node.peer_id);
            node.run(do_mine, &mut state).await;
        }
        Err(e) => println!("P2P error: {}", e),
    }
}

fn cmd_info() {
    if !crate::store::has_data() {
        println!("No data. Run init first.");
        return;
    }
    match crate::store::load_utxo_set() {
        Ok(state) => {
            let blocks = crate::store::load_blocks().unwrap_or_default();
            println!("Ewatts Node");
            println!("  Blocks: {}", blocks.len());
            println!("  UTXOs:  {}", state.utxo_count());
            println!("  Supply: {}", state.total_supply());
            // Show recent VR if blocks exist
            if let Some(last) = blocks.last() {
                println!("  VR:     {}", crate::vr::format_vr(last.header.vr_block));
            }
        }
        Err(e) => println!("Error: {}", e),
    }
}
