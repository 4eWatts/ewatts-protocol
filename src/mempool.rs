//! Pending transaction pool with fee-based priority and eviction.

use crate::block::Transaction;
use crate::state::UtxoSet;
use std::collections::HashMap;
use std::sync::Mutex;

struct PendingTx {
    tx: Transaction,
    fee: u64,
    tx_hash: [u8; 32],
}

/// Sorted by fee descending. Uses indices for double-spend checks.
struct MempoolInner {
    pending: Vec<PendingTx>,
    key_images: HashMap<[u8; 32], [u8; 32]>,
    utxo_spends: HashMap<([u8; 32], u32), [u8; 32]>,
}

fn new_mempool() -> MempoolInner {
    MempoolInner {
        pending: Vec::new(),
        key_images: HashMap::new(),
        utxo_spends: HashMap::new(),
    }
}

static MEMPOOL: Mutex<Option<MempoolInner>> = Mutex::new(None);

/// Maximum pending transactions in the mempool.
const MAX_MEMPOOL_TXS: usize = 5000;

fn get_pool() -> std::sync::MutexGuard<'static, Option<MempoolInner>> {
    let mut guard = MEMPOOL.lock().unwrap();
    if guard.is_none() {
        *guard = Some(new_mempool());
    }
    guard
}

/// Fee = sum(inputs) - sum(outputs). Coinbase returns 0.
fn compute_fee(tx: &Transaction, state: &UtxoSet) -> Result<u64, String> {
    if tx.inputs.is_empty() {
        return Ok(0); // coinbase has no fee
    }
    let utxo_map = state.utxos_map();
    let mut inputs_sum = 0u64;
    for input in &tx.inputs {
        let key = crate::state::UtxoKey {
            tx_hash: input.previous_tx_hash,
            output_index: input.output_index,
        };
        let utxo = utxo_map
            .get(&key)
            .ok_or_else(|| format!("UTXO not found: {:x}..{}", input.previous_tx_hash[0], input.output_index))?;
        inputs_sum = inputs_sum
            .checked_add(utxo.amount)
            .ok_or("Input amount overflow")?;
    }
    let outputs_sum: u64 = tx
        .outputs
        .iter()
        .try_fold(0u64, |acc, o| acc.checked_add(o.amount).ok_or("Output overflow"))
        .map_err(|e: &str| e.to_string())?;
    Ok(inputs_sum.saturating_sub(outputs_sum))
}

/// Validate tx, verify MLSAG, check double-spends, insert in fee order.
/// Evicts lowest-fee tx when full if this tx has higher fee.
pub fn submit(tx: Transaction, state: &UtxoSet) -> Result<(), String> {
    state.validate_transaction(&tx)?;

    let fee = compute_fee(&tx, state)?;
    let tx_hash = tx.hash();

    if let Some(ref mlsag) = tx.mlsag {
        let ring_members = tx
            .ring_members
            .as_ref()
            .ok_or("Missing ring members for MLSAG tx")?;
        let msg = crate::state::tx_msg(&tx);

        let utxo_map = state.utxos_map();
        let mut ring_layers: Vec<Vec<curve25519_dalek::ristretto::RistrettoPoint>> = Vec::new();
        for members_for_input in ring_members.iter() {
            let ring = crate::state::build_ring_inline(utxo_map, members_for_input)?;
            let flat: Vec<curve25519_dalek::ristretto::RistrettoPoint> = ring.into_iter().flatten().collect();
            ring_layers.push(flat);
        }
        if ring_layers.is_empty() {
            return Err("No rings in MLSAG tx".into());
        }
        let n_layers = ring_layers.len();
        let ring_size = ring_layers[0].len();
        let mut ring_formatted = vec![Vec::with_capacity(n_layers); ring_size];
        for ring_pos in 0..ring_size {
            for layer in 0..n_layers {
                ring_formatted[ring_pos].push(ring_layers[layer][ring_pos]);
            }
        }
        let sig = mlsag
            .to_sig()
            .map_err(|e| format!("MLSAG deserialization: {}", e))?;
        if !sig.verify(&ring_formatted, &msg) {
            return Err("MLSAG signature invalid".into());
        }
    }

    let mut pool_opt = get_pool();
    let pool = pool_opt.as_mut().unwrap();

    for input in &tx.inputs {
        if state.spent_key_images().contains(&input.key_image) {
            return Err("Double-spend: key image already spent in chain".into());
        }
    }

    for input in &tx.inputs {
        if pool.key_images.contains_key(&input.key_image) {
            return Err("Double-spend: key image already in mempool".into());
        }
    }

    for input in &tx.inputs {
        let utxo_ref = (input.previous_tx_hash, input.output_index);
        if pool.utxo_spends.contains_key(&utxo_ref) {
            return Err("Double-spend: UTXO already spent in mempool".into());
        }
    }

    if pool.pending.len() >= MAX_MEMPOOL_TXS {
        let min_fee_idx = pool
            .pending
            .iter()
            .enumerate()
            .min_by_key(|(_, pt)| pt.fee)
            .map(|(idx, _)| idx);

        if let Some(idx) = min_fee_idx {
            if fee > pool.pending[idx].fee {
                let evicted = pool.pending.swap_remove(idx);
                for input in &evicted.tx.inputs {
                    pool.key_images.remove(&input.key_image);
                    pool.utxo_spends.remove(&(input.previous_tx_hash, input.output_index));
                }
            } else {
                return Err(format!(
                    "Mempool full. This tx fee ({}) <= lowest fee in pool ({})",
                    fee,
                    pool.pending[idx].fee
                ));
            }
        } else {
            return Err("Mempool full (no evictable tx)".into());
        }
    }

    for input in &tx.inputs {
        pool.key_images.insert(input.key_image, tx_hash);
        pool.utxo_spends.insert(
            (input.previous_tx_hash, input.output_index),
            tx_hash,
        );
    }

    let pending_tx = PendingTx {
        tx,
        fee,
        tx_hash,
    };
    let insert_pos = pool
        .pending
        .binary_search_by(|pt| {
            pt.fee.cmp(&fee).reverse()
                .then_with(|| pt.tx_hash.cmp(&tx_hash))
        })
        .unwrap_or_else(|e| e);
    pool.pending.insert(insert_pos, pending_tx);

    Ok(())
}

/// All pending txs (clone). For large mempools, use take_for_mining().
pub fn peek_all() -> Vec<Transaction> {
    let pool_opt = MEMPOOL.lock().unwrap();
    let pool_ref = match pool_opt.as_ref() {
        Some(p) => p,
        None => return vec![],
    };
    pool_ref.pending.iter().map(|pt| pt.tx.clone()).collect()
}

/// Take up to N highest-fee txs. Call confirm_mined() after block creation.
pub fn take_for_mining(limit: usize) -> Vec<Transaction> {
    let mut pool_opt = get_pool();
    let pool = pool_opt.as_mut().unwrap();
    let take_count = std::cmp::min(limit, pool.pending.len());
    let txs: Vec<Transaction> = pool.pending.iter().take(take_count).map(|pt| pt.tx.clone()).collect();
    txs
}

/// Remove mined txs from mempool, rebuild indices.
pub fn confirm_mined(tx_hashes: &[[u8; 32]]) {
    let mut pool_opt = get_pool();
    let pool = pool_opt.as_mut().unwrap();
    let hash_set: std::collections::HashSet<[u8; 32]> = tx_hashes.iter().copied().collect();
    pool.pending.retain(|pt| !hash_set.contains(&pt.tx_hash));
    // Rebuild indices
    pool.key_images.clear();
    pool.utxo_spends.clear();
    for pt in &pool.pending {
        for input in &pt.tx.inputs {
            pool.key_images.insert(input.key_image, pt.tx_hash);
            pool.utxo_spends.insert((input.previous_tx_hash, input.output_index), pt.tx_hash);
        }
    }
}

/// Drain all pending txs. Use take_for_mining() + confirm_mined() for non-lossy mining.
pub fn drain() -> Vec<Transaction> {
    let mut pool_opt = get_pool();
    let pool = pool_opt.as_mut().unwrap();
    let txs: Vec<Transaction> = pool.pending.drain(..).map(|pt| pt.tx).collect();
    pool.key_images.clear();
    pool.utxo_spends.clear();
    txs
}

/// Peek up to 100 highest-fee txs.
pub fn peek() -> Vec<Transaction> {
    let pool_opt = MEMPOOL.lock().unwrap();
    match pool_opt.as_ref() {
        Some(p) => p.pending.iter().take(100).map(|pt| pt.tx.clone()).collect(),
        None => vec![],
    }
}

/// Number of pending transactions.
pub fn pending_count() -> usize {
    let pool_opt = MEMPOOL.lock().unwrap();
    pool_opt.as_ref().map(|p| p.pending.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mempool_empty_start() {
        assert_eq!(pending_count(), 0);
        let txs = drain();
        assert!(txs.is_empty());
    }
}
