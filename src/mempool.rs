//! Mempool: pending transaction pool for the fast-lane transaction hash layer.
//! Transactions are validated on submission and queued for the next mining block.

use crate::block::Transaction;
use std::sync::Mutex;

static MEMPOOL: Mutex<Vec<Transaction>> = Mutex::new(Vec::new());

/// Maximum pending transactions in the mempool.
const MAX_MEMPOOL_TXS: usize = 5000;

/// Submit a transaction to the mempool after validation.
pub fn submit(tx: Transaction, state: &crate::state::UtxoSet) -> Result<(), String> {
    // Validate basic structure
    state.validate_transaction(&tx)?;

    // If it's a MLSAG private tx, verify the ring signature
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
            let flat: Vec<curve25519_dalek::ristretto::RistrettoPoint> = ring
                .into_iter()
                .map(|v| {
                    v.into_iter()
                        .next()
                        .unwrap_or(curve25519_dalek::traits::Identity::identity())
                })
                .collect();
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

    // Acquire mempool lock once for double-spend check + push (🔴 race window fix)
    let mut pool = MEMPOOL.lock().unwrap();

    // Check mempool size limit (🔴 DoS fix)
    if pool.len() >= MAX_MEMPOOL_TXS {
        return Err("Mempool full".into());
    }

    // Check for double-spend against mempool (key images)
    for pending in pool.iter() {
        for pending_input in &pending.inputs {
            for input in &tx.inputs {
                if pending_input.key_image == input.key_image {
                    return Err("Double-spend: key image already in mempool".into());
                }
            }
        }
    }

    pool.push(tx);
    Ok(())
}

/// Drain all pending transactions from the mempool (FIFO).
pub fn drain() -> Vec<Transaction> {
    let mut pool = MEMPOOL.lock().unwrap();
    std::mem::take(&mut *pool)
}

/// Peek at pending transactions (returns first 100 to avoid cloning everything).
pub fn peek() -> Vec<Transaction> {
    let pool = MEMPOOL.lock().unwrap();
    pool.iter().take(100).cloned().collect()
}

/// Number of pending transactions.
pub fn pending_count() -> usize {
    let pool = MEMPOOL.lock().unwrap();
    pool.len()
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
