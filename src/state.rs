/// Ewatts Protocol — State Management (UTXO Set)
use std::collections::{HashMap, HashSet};
use crate::block::{Transaction, TxInput, TxOutput, Block};
use crate::constants;

#[derive(Debug, Clone)]
pub struct UtxoEntry {
    pub amount: u64,
    pub public_key: Vec<u8>,
    pub block_height: u64,
    pub tx_index: u32,
    pub output_index: u32,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct UtxoKey {
    pub tx_hash: [u8; 32],
    pub output_index: u32,
}

#[derive(Debug)]
pub struct UtxoSet {
    utxos: HashMap<UtxoKey, UtxoEntry>,
    spent_key_images: HashSet<[u8; 32]>,
    /// Total supply from coinbase transactions only
    total_supply: u64,
}

impl UtxoSet {
    pub fn new() -> Self {
        UtxoSet { utxos: HashMap::new(), spent_key_images: HashSet::new(), total_supply: 0 }
    }

    /// Add UTXOs from a transaction's outputs.
    /// Does NOT increment total_supply — caller decides if it's coinbase.
    pub fn add_transaction_outputs(&mut self, tx_hash: &[u8; 32], tx: &Transaction, block_height: u64, tx_index: u32) {
        for (i, output) in tx.outputs.iter().enumerate() {
            let key = UtxoKey { tx_hash: *tx_hash, output_index: i as u32 };
            self.utxos.insert(key, UtxoEntry {
                amount: output.amount,
                public_key: output.public_key.clone(),
                block_height, tx_index, output_index: i as u32,
            });
        }
    }

    /// Increment total supply (coinbase only).
    pub fn add_coinbase_supply(&mut self, amount: u64) {
        self.total_supply = self.total_supply.checked_add(amount).unwrap_or(self.total_supply);
    }

    /// Spend UTXOs referenced by transaction inputs.
    /// Order: validate all key images FIRST, then remove UTXOs.
    pub fn spend_transaction_inputs(&mut self, tx: &Transaction) -> Result<(), String> {
        // First pass: validate ALL key images before mutating anything
        for input in &tx.inputs {
            if self.spent_key_images.contains(&input.key_image) {
                return Err("Double-spend detected: key image already spent".to_string());
            }
            let key = UtxoKey { tx_hash: input.previous_tx_hash, output_index: input.output_index };
            if !self.utxos.contains_key(&key) {
                return Err(format!("UTXO not found: {:?}/{}", &input.previous_tx_hash[..4], input.output_index));
            }
        }
        // Second pass: mutate (all checks passed)
        for input in &tx.inputs {
            let key = UtxoKey { tx_hash: input.previous_tx_hash, output_index: input.output_index };
            self.utxos.remove(&key);
            self.spent_key_images.insert(input.key_image);
        }
        Ok(())
    }

    pub fn validate_transaction(&self, tx: &Transaction) -> Result<(), String> {
        if tx.inputs.is_empty() && tx.outputs.is_empty() {
            return Err("Empty transaction".to_string());
        }
        let mut input_sum: u64 = 0;
        let mut output_sum: u64 = 0;
        for input in &tx.inputs {
            let key = UtxoKey { tx_hash: input.previous_tx_hash, output_index: input.output_index };
            let utxo = self.utxos.get(&key)
                .ok_or_else(|| format!("Input UTXO not found: {:?}/{}", &input.previous_tx_hash[..4], input.output_index))?;
            input_sum = input_sum.checked_add(utxo.amount).ok_or("Input sum overflow")?;
            if self.spent_key_images.contains(&input.key_image) {
                return Err("Double-spend: key image already used".to_string());
            }
        }
        for output in &tx.outputs {
            output_sum = output_sum.checked_add(output.amount).ok_or("Output sum overflow")?;
        }
        if !tx.inputs.is_empty() && input_sum < output_sum {
            return Err(format!("Creates Ewatts: input {} < output {}", input_sum, output_sum));
        }
        Ok(())
    }

    pub fn apply_block(&mut self, block: &Block, block_height: u64) -> Result<(), String> {
        for (tx_idx, tx) in block.body.transactions.iter().enumerate() {
            let tx_hash = tx.hash();
            if tx_idx == 0 {
                // Coinbase: add outputs + increment supply
                self.add_transaction_outputs(&tx_hash, tx, block_height, tx_idx as u32);
                let coinbase_amount: u64 = tx.outputs.iter().map(|o| o.amount).sum();
                self.add_coinbase_supply(coinbase_amount);
            } else {
                self.spend_transaction_inputs(tx)?;
                self.add_transaction_outputs(&tx_hash, tx, block_height, tx_idx as u32);
            }
        }
        Ok(())
    }

    pub fn get_balance(&self, public_key: &[u8]) -> u64 {
        self.utxos.values()
            .filter(|u| u.public_key.as_slice() == public_key)
            .map(|u| u.amount).sum()
    }

    pub fn utxo_count(&self) -> usize { self.utxos.len() }
    pub fn total_supply(&self) -> u64 { self.total_supply }
    pub fn is_key_image_spent(&self, key_image: &[u8; 32]) -> bool {
        self.spent_key_images.contains(key_image)
    }

    pub fn genesis(coinbase_amount: u64, coinbase_public_key: &[u8]) -> Self {
        let mut state = UtxoSet::new();
        let tx = Transaction {
            version: 1, inputs: vec![],
            outputs: vec![TxOutput { amount: coinbase_amount, public_key: coinbase_public_key.to_vec() }],
            ring_size: 1,
        };
        let tx_hash = tx.hash();
        state.add_transaction_outputs(&tx_hash, &tx, 0, 0);
        state.add_coinbase_supply(coinbase_amount);
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{TxInput, TxOutput};

    fn out(amounts: &[u64], pk: &[u8]) -> Vec<TxOutput> {
        amounts.iter().map(|&a| TxOutput { amount: a, public_key: pk.to_vec() }).collect()
    }
    fn inp(tx_hash: &[u8; 32], idx: u32, key_img: &[u8; 32]) -> TxInput {
        TxInput { previous_tx_hash: *tx_hash, output_index: idx, key_image: *key_img }
    }

    #[test] fn test_empty() { let s = UtxoSet::new(); assert_eq!(s.utxo_count(), 0); assert_eq!(s.total_supply(), 0); }

    #[test] fn test_add_and_spend() {
        let mut s = UtxoSet::new(); let pk = vec![0u8; 33];
        let tx = Transaction { version: 1, inputs: vec![], outputs: out(&[1000, 2000], &pk), ring_size: 1 };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        assert_eq!(s.utxo_count(), 2);
        assert_eq!(s.get_balance(&pk), 3000);
    }

    #[test] fn test_spend_utxo() {
        let mut s = UtxoSet::new(); let pk = vec![0u8; 33]; let pk2 = vec![1u8; 33];
        let tx = Transaction { version: 1, inputs: vec![], outputs: out(&[5000], &pk), ring_size: 1 };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        let spend = Transaction { version: 1, inputs: vec![inp(&h, 0, &[0xab; 32])], outputs: out(&[3000], &pk2), ring_size: 1 };
        assert!(s.validate_transaction(&spend).is_ok());
        s.spend_transaction_inputs(&spend).unwrap();
        s.add_transaction_outputs(&spend.hash(), &spend, 1, 0);
        assert_eq!(s.get_balance(&pk), 0);
        assert_eq!(s.get_balance(&pk2), 3000);
    }

    #[test] fn test_double_spend_rejected() {
        let mut s = UtxoSet::new(); let pk = vec![0u8; 33];
        let tx = Transaction { version: 1, inputs: vec![], outputs: out(&[5000], &pk), ring_size: 1 };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        let sp1 = Transaction { version: 1, inputs: vec![inp(&h, 0, &[0xab; 32])], outputs: out(&[3000], &pk), ring_size: 1 };
        s.spend_transaction_inputs(&sp1).unwrap();
        let sp2 = Transaction { version: 1, inputs: vec![inp(&h, 0, &[0xcd; 32])], outputs: out(&[3000], &pk), ring_size: 1 };
        assert!(s.spend_transaction_inputs(&sp2).is_err());
    }

    #[test] fn test_genesis() {
        let s = UtxoSet::genesis(100_000_000_000, &[0u8; 33]);
        assert_eq!(s.utxo_count(), 1);
        assert_eq!(s.total_supply(), 100_000_000_000);
    }

    #[test] fn test_supply_not_inflated_by_change() {
        let mut s = UtxoSet::new(); let pk = vec![0u8; 33]; let pk2 = vec![1u8; 33];
        let tx = Transaction { version: 1, inputs: vec![], outputs: out(&[5000], &pk), ring_size: 1 };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        s.add_coinbase_supply(5000);
        assert_eq!(s.total_supply(), 5000);
        // Regular tx: spend 5000, output 3000 (2000 is fee)
        let sp = Transaction { version: 1, inputs: vec![inp(&h, 0, &[0xab; 32])], outputs: out(&[3000], &pk2), ring_size: 1 };
        s.spend_transaction_inputs(&sp).unwrap();
        s.add_transaction_outputs(&sp.hash(), &sp, 1, 0);
        // Supply should NOT have increased
        assert_eq!(s.total_supply(), 5000);
    }

    #[test] fn test_invalid_tx_rejected() {
        let s = UtxoSet::new(); let pk = vec![0u8; 33];
        let tx = Transaction { version: 1, inputs: vec![inp(&[0;32], 0, &[0xab;32])], outputs: out(&[100], &pk), ring_size: 1 };
        assert!(s.validate_transaction(&tx).is_err());
    }
}
