//! Mempool: pending transaction pool for the fast-lane transaction hash layer.
//! Transactions are validated on submission and queued for the next mining block.

use std::sync::Mutex;
use crate::block::Transaction;

static MEMPOOL: Mutex<Vec<Transaction>> = Mutex::new(Vec::new());

/// Submit a transaction to the mempool after validation.
pub fn submit(tx: Transaction, state: &crate::state::UtxoSet) -> Result<(), String> {
    // Validate basic structure
    state.validate_transaction(&tx)?;

    // If it's a MLSAG private tx, verify the ring signature
    if let Some(ref mlsag) = tx.mlsag {
        let ring_members = tx.ring_members.as_ref()
            .ok_or("Missing ring members for MLSAG tx")?;
        let msg = crate::state::tx_msg(&tx);

        let mut all_rings = Vec::new();
        for members_for_input in ring_members.iter() {
            let ring = crate::state::build_ring_inline(state, members_for_input)?;
            all_rings.push(ring);
        }
        if all_rings.is_empty() {
            return Err("No rings in MLSAG tx".into());
        }
        let n_layers = all_rings.len();
        let ring_size = all_rings[0].len();
        let mut ring_formatted = vec![Vec::with_capacity(n_layers); ring_size];
        for ring_pos in 0..ring_size {
            for layer in 0..n_layers {
                ring_formatted[ring_pos].push(all_rings[layer][ring_pos]);
            }
        }
        let sig = mlsag.to_sig();
        if !sig.verify(&ring_formatted, &msg) {
            return Err("MLSAG signature invalid".into());
        }
    }

    // Check for double-spend against mempool (key images)
    {
        let pool = MEMPOOL.lock().unwrap();
        for pending in pool.iter() {
            for pending_input in &pending.inputs {
                for input in &tx.inputs {
                    if pending_input.key_image == input.key_image {
                        return Err("Double-spend: key image already in mempool".into());
                    }
                }
            }
        }
    }

    // Add to mempool
    let mut pool = MEMPOOL.lock().unwrap();
    pool.push(tx);
    Ok(())
}

/// Drain all pending transactions from the mempool (FIFO).
pub fn drain() -> Vec<Transaction> {
    let mut pool = MEMPOOL.lock().unwrap();
    std::mem::take(&mut *pool)
}

/// Peek at pending transactions.
pub fn peek() -> Vec<Transaction> {
    let pool = MEMPOOL.lock().unwrap();
    pool.clone()
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
