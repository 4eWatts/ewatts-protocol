/// Ewatts Protocol — State Management (UTXO Set)
///
/// The "ledger" — tracks who owns how many Ewatts.
/// Every transaction output that hasn't been spent is a UTXO.
/// The UTXO set is the canonical record of ownership.

use std::collections::{HashMap, HashSet};
use crate::block::{Transaction, TxInput, TxOutput, Block};
use crate::constants;

/// A single unspent transaction output.
#[derive(Debug, Clone)]
pub struct UtxoEntry {
    /// Amount in Ewatt units (1 Ewatt = 10^8 base units)
    pub amount: u64,
    /// Recipient public key (33 bytes compressed)
    pub public_key: Vec<u8>,
    /// The block height at which this UTXO was created
    pub block_height: u64,
    /// The transaction index within the block
    pub tx_index: u32,
    /// The output index within the transaction
    pub output_index: u32,
}

/// Unique identifier for a UTXO: (tx_hash, output_index)
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct UtxoKey {
    pub tx_hash: [u8; 32],
    pub output_index: u32,
}

/// The UTXO set + spent key images for the entire chain.
#[derive(Debug)]
pub struct UtxoSet {
    /// All unspent outputs, keyed by (tx_hash, output_index)
    utxos: HashMap<UtxoKey, UtxoEntry>,
    /// All spent key images (double-spend prevention)
    spent_key_images: HashSet<[u8; 32]>,
    /// Total Ewatt supply ever created
    total_supply: u64,
}

impl UtxoSet {
    /// Create a new empty UTXO set.
    pub fn new() -> Self {
        UtxoSet {
            utxos: HashMap::new(),
            spent_key_images: HashSet::new(),
            total_supply: 0,
        }
    }

    /// Add UTXOs from a transaction's outputs.
    pub fn add_transaction_outputs(&mut self, tx_hash: &[u8; 32], tx: &Transaction, block_height: u64, tx_index: u32) {
        for (i, output) in tx.outputs.iter().enumerate() {
            let key = UtxoKey {
                tx_hash: *tx_hash,
                output_index: i as u32,
            };
            self.utxos.insert(key, UtxoEntry {
                amount: output.amount,
                public_key: output.public_key.clone(),
                block_height,
                tx_index,
                output_index: i as u32,
            });
            self.total_supply += output.amount;
        }
    }

    /// Spend UTXOs referenced by a transaction's inputs.
    /// Returns error if any input is invalid (doesn't exist or already spent).
    pub fn spend_transaction_inputs(&mut self, tx: &Transaction) -> Result<(), String> {
        for input in &tx.inputs {
            let key = UtxoKey {
                tx_hash: input.previous_tx_hash,
                output_index: input.output_index,
            };

            // Check UTXO exists
            if !self.utxos.contains_key(&key) {
                return Err(format!(
                    "UTXO not found: {:?}/{}", 
                    &input.previous_tx_hash[..4], 
                    input.output_index
                ));
            }

            // Remove from UTXO set (spend it)
            self.utxos.remove(&key);

            // Track key image for double-spend prevention
            if !self.spent_key_images.insert(input.key_image) {
                return Err("Double-spend detected: key image already spent".to_string());
            }
        }
        Ok(())
    }

    /// Validate a transaction against current state.
    /// Checks: inputs exist, not spent, sum(inputs) >= sum(outputs), key images unique.
    pub fn validate_transaction(&self, tx: &Transaction) -> Result<(), String> {
        if tx.inputs.is_empty() && tx.outputs.is_empty() {
            return Err("Empty transaction".to_string());
        }

        let mut input_sum: u64 = 0;
        let mut output_sum: u64 = 0;

        for input in &tx.inputs {
            let key = UtxoKey {
                tx_hash: input.previous_tx_hash,
                output_index: input.output_index,
            };

            let utxo = self.utxos.get(&key)
                .ok_or_else(|| format!("Input UTXO not found: {:?}/{}", &input.previous_tx_hash[..4], input.output_index))?;

            input_sum = input_sum.checked_add(utxo.amount)
                .ok_or("Input sum overflow")?;

            // Check key image not already spent
            if self.spent_key_images.contains(&input.key_image) {
                return Err("Double-spend: key image already used".to_string());
            }
        }

        for output in &tx.outputs {
            output_sum = output_sum.checked_add(output.amount)
                .ok_or("Output sum overflow")?;
        }

        // Coinbase transactions (block rewards) have no inputs
        if !tx.inputs.is_empty() && input_sum < output_sum {
            return Err(format!(
                "Transaction creates Ewatts: input {} < output {}",
                input_sum, output_sum
            ));
        }

        if !tx.inputs.is_empty() && input_sum > output_sum {
            // Excess is the miner fee
        }

        Ok(())
    }

    /// Apply a block to the UTXO set (add coinbase + all tx outputs, spend all tx inputs).
    pub fn apply_block(&mut self, block: &Block, block_height: u64) -> Result<(), String> {
        for (tx_idx, tx) in block.body.transactions.iter().enumerate() {
            let tx_hash = tx.hash();

            if tx_idx == 0 {
                // Coinbase transaction: just add outputs (no inputs to validate)
                self.add_transaction_outputs(&tx_hash, tx, block_height, tx_idx as u32);
            } else {
                // Regular transaction: validate, spend inputs, add outputs
                // (In full node, validation happens before inclusion in block)
                self.spend_transaction_inputs(tx)?;
                self.add_transaction_outputs(&tx_hash, tx, block_height, tx_idx as u32);
            }
        }
        Ok(())
    }

    /// Get balance for a public key.
    pub fn get_balance(&self, public_key: &[u8]) -> u64 {
        self.utxos.values()
            .filter(|u| u.public_key.as_slice() == public_key)
            .map(|u| u.amount)
            .sum()
    }

    /// Get total number of UTXOs.
    pub fn utxo_count(&self) -> usize {
        self.utxos.len()
    }

    /// Get total supply circulating.
    pub fn total_supply(&self) -> u64 {
        self.total_supply
    }

    /// Check if a key image has been spent.
    pub fn is_key_image_spent(&self, key_image: &[u8; 32]) -> bool {
        self.spent_key_images.contains(key_image)
    }

    /// Create a genesis UTXO set (for block 0).
    pub fn genesis(coinbase_amount: u64, coinbase_public_key: &[u8]) -> Self {
        let mut state = UtxoSet::new();
        let genesis_tx = Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                amount: coinbase_amount,
                public_key: coinbase_public_key.to_vec(),
            }],
            ring_size: 1,
        };
        let tx_hash = genesis_tx.hash();
        state.add_transaction_outputs(&tx_hash, &genesis_tx, 0, 0);
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{TxInput, TxOutput};
    use crate::constants;

    fn make_tx_outputs(amounts: &[u64], pk: &[u8]) -> Vec<TxOutput> {
        amounts.iter().map(|&a| TxOutput { amount: a, public_key: pk.to_vec() }).collect()
    }

    fn make_tx_input(tx_hash: &[u8; 32], idx: u32, key_img: &[u8; 32]) -> TxInput {
        TxInput { previous_tx_hash: *tx_hash, output_index: idx, key_image: *key_img }
    }

    #[test]
    fn test_empty_utxo_set() {
        let state = UtxoSet::new();
        assert_eq!(state.utxo_count(), 0);
        assert_eq!(state.total_supply(), 0);
    }

    #[test]
    fn test_add_and_spend() {
        let mut state = UtxoSet::new();
        let pk = vec![0u8; 33];
        let tx = Transaction {
            version: 1,
            inputs: vec![],
            outputs: make_tx_outputs(&[1000, 2000], &pk),
            ring_size: 1,
        };
        let tx_hash = tx.hash();
        state.add_transaction_outputs(&tx_hash, &tx, 0, 0);

        assert_eq!(state.utxo_count(), 2);
        assert_eq!(state.total_supply(), 3000);
        assert_eq!(state.get_balance(&pk), 3000);
    }

    #[test]
    fn test_spend_utxo() {
        let mut state = UtxoSet::new();
        let pk = vec![0u8; 33];
        let pk2 = vec![1u8; 33];

        // Create coinbase
        let tx = Transaction {
            version: 1, inputs: vec![],
            outputs: make_tx_outputs(&[5000], &pk),
            ring_size: 1,
        };
        let tx_hash = tx.hash();
        state.add_transaction_outputs(&tx_hash, &tx, 0, 0);
        assert_eq!(state.get_balance(&pk), 5000);

        // Spend it
        let spend_tx = Transaction {
            version: 1,
            inputs: vec![make_tx_input(&tx_hash, 0, &[0xabu8; 32])],
            outputs: make_tx_outputs(&[3000], &pk2),
            ring_size: 1,
        };
        assert!(state.validate_transaction(&spend_tx).is_ok());
        state.spend_transaction_inputs(&spend_tx).unwrap();
        state.add_transaction_outputs(&spend_tx.hash(), &spend_tx, 1, 1);

        assert_eq!(state.get_balance(&pk), 0);
        assert_eq!(state.get_balance(&pk2), 3000);
    }

    #[test]
    fn test_double_spend_rejected() {
        let mut state = UtxoSet::new();
        let pk = vec![0u8; 33];

        let tx = Transaction {
            version: 1, inputs: vec![],
            outputs: make_tx_outputs(&[5000], &pk),
            ring_size: 1,
        };
        let tx_hash = tx.hash();
        state.add_transaction_outputs(&tx_hash, &tx, 0, 0);

        // First spend
        let spend = Transaction {
            version: 1,
            inputs: vec![make_tx_input(&tx_hash, 0, &[0xabu8; 32])],
            outputs: make_tx_outputs(&[3000], &pk),
            ring_size: 1,
        };
        state.spend_transaction_inputs(&spend).unwrap();

        // Second spend (same UTXO, different key image)
        let spend2 = Transaction {
            version: 1,
            inputs: vec![make_tx_input(&tx_hash, 0, &[0xcdu8; 32])],
            outputs: make_tx_outputs(&[3000], &pk),
            ring_size: 1,
        };
        // UTXO already spent, should fail
        assert!(state.spend_transaction_inputs(&spend2).is_err());
    }

    #[test]
    fn test_genesis() {
        let state = UtxoSet::genesis(100_000_000_000, &[0u8; 33]);
        assert_eq!(state.utxo_count(), 1);
        assert_eq!(state.total_supply(), 100_000_000_000);
    }

    #[test]
    fn test_validate_transaction_insufficient_funds() {
        let state = UtxoSet::new();
        let pk = vec![0u8; 33];
        // Try to spend from a UTXO that doesn't exist
        let tx = Transaction {
            version: 1,
            inputs: vec![make_tx_input(&[0u8; 32], 0, &[0xabu8; 32])],
            outputs: make_tx_outputs(&[100], &pk),
            ring_size: 1,
        };
        assert!(state.validate_transaction(&tx).is_err());
    }

    #[test]
    fn test_balance_after_multiple_txs() {
        let mut state = UtxoSet::new();
        let alice = vec![1u8; 33];
        let bob = vec![2u8; 33];

        // Coinbase to Alice
        let tx1 = Transaction {
            version: 1, inputs: vec![],
            outputs: make_tx_outputs(&[10000], &alice),
            ring_size: 1,
        };
        let h1 = tx1.hash();
        state.add_transaction_outputs(&h1, &tx1, 0, 0);

        // Alice sends 3000 to Bob
        let tx2 = Transaction {
            version: 1,
            inputs: vec![make_tx_input(&h1, 0, &[0xab; 32])],
            outputs: make_tx_outputs(&[3000], &bob),
            ring_size: 1,
        };
        state.spend_transaction_inputs(&tx2).unwrap();
        state.add_transaction_outputs(&tx2.hash(), &tx2, 1, 0);

        assert_eq!(state.get_balance(&alice), 0);
        assert_eq!(state.get_balance(&bob), 3000);
    }
}
