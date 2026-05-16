use ed25519_dalek::{Verifier, VerifyingKey, Signature};
use std::collections::{HashMap, HashSet};
use crate::block::{Transaction, TxInput, TxOutput, Block};

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
    total_supply: u64,
}

/// The message to sign for a transaction: hash of (inputs + outputs + ring_size)
pub fn tx_msg(tx: &Transaction) -> Vec<u8> {
    let mut msg = Vec::new();
    for i in &tx.inputs { msg.extend_from_slice(&i.previous_tx_hash); msg.extend_from_slice(&i.output_index.to_le_bytes()); }
    for o in &tx.outputs { msg.extend_from_slice(&o.amount.to_le_bytes()); msg.extend_from_slice(&o.public_key); }
    msg.extend_from_slice(&tx.ring_size.to_le_bytes());
    msg
}

pub fn verify_tx_signature(tx: &Transaction, pubkey_bytes: &[u8]) -> Result<(), String> {
    if tx.inputs.is_empty() { return Ok(()); } // coinbase
    if tx.signatures.is_empty() { return Err("sem assinatura".to_string()); }
    let pk = VerifyingKey::from_bytes(pubkey_bytes.try_into().map_err(|_| "chave invalida")?)
        .map_err(|_| "chave publica invalida")?;
    let sig = Signature::from_slice(&tx.signatures[0])
        .map_err(|_| "assinatura invalida")?;
    let msg = tx_msg(tx);
    pk.verify(&msg, &sig).map_err(|_| "assinatura nao confere".to_string())
}

impl UtxoSet {
    pub fn new() -> Self { UtxoSet { utxos: HashMap::new(), spent_key_images: HashSet::new(), total_supply: 0 } }

    pub fn add_transaction_outputs(&mut self, tx_hash: &[u8; 32], tx: &Transaction, block_height: u64, tx_index: u32) {
        for (i, output) in tx.outputs.iter().enumerate() {
            let key = UtxoKey { tx_hash: *tx_hash, output_index: i as u32 };
            self.utxos.insert(key, UtxoEntry {
                amount: output.amount, public_key: output.public_key.clone(),
                block_height, tx_index, output_index: i as u32,
            });
        }
    }

    pub fn add_coinbase_supply(&mut self, amount: u64) {
        self.total_supply = self.total_supply.checked_add(amount).unwrap_or(self.total_supply);
    }

    pub fn spend_transaction_inputs(&mut self, tx: &Transaction) -> Result<(), String> {
        // First pass: validate ALL before mutating
        for input in &tx.inputs {
            if self.spent_key_images.contains(&input.key_image) {
                return Err("Double-spend: key image already spent".to_string());
            }
            let key = UtxoKey { tx_hash: input.previous_tx_hash, output_index: input.output_index };
            let utxo = self.utxos.get(&key)
                .ok_or_else(|| format!("UTXO not found: {:?}/{}", &input.previous_tx_hash[..4], input.output_index))?;
            // Verify signature against UTXO owner's public key
            verify_tx_signature(tx, &utxo.public_key)?;
        }
        // Second pass: mutate
        for input in &tx.inputs {
            let key = UtxoKey { tx_hash: input.previous_tx_hash, output_index: input.output_index };
            self.utxos.remove(&key);
            self.spent_key_images.insert(input.key_image);
        }
        Ok(())
    }

    pub fn validate_transaction(&self, tx: &Transaction) -> Result<(), String> {
        if tx.inputs.is_empty() && tx.outputs.is_empty() { return Err("Empty transaction".to_string()); }
        let (mut input_sum, mut output_sum) = (0u64, 0u64);
        for input in &tx.inputs {
            let key = UtxoKey { tx_hash: input.previous_tx_hash, output_index: input.output_index };
            let utxo = self.utxos.get(&key)
                .ok_or_else(|| format!("Input UTXO not found: {:?}/{}", &input.previous_tx_hash[..4], input.output_index))?;
            input_sum = input_sum.checked_add(utxo.amount).ok_or("Input sum overflow")?;
            if self.spent_key_images.contains(&input.key_image) {
                return Err("Double-spend: key image already used".to_string());
            }
        }
        for output in &tx.outputs { output_sum = output_sum.checked_add(output.amount).ok_or("Output sum overflow")?; }
        if !tx.inputs.is_empty() && input_sum < output_sum {
            return Err(format!("Creates Ewatts: input {} < output {}", input_sum, output_sum));
        }
        Ok(())
    }

    pub fn apply_block(&mut self, block: &Block, block_height: u64) -> Result<(), String> {
        for (tx_idx, tx) in block.body.transactions.iter().enumerate() {
            let tx_hash = tx.hash();
            if tx_idx == 0 {
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
        self.utxos.values().filter(|u| u.public_key.as_slice() == public_key).map(|u| u.amount).sum()
    }

    pub fn utxo_count(&self) -> usize { self.utxos.len() }
    pub fn total_supply(&self) -> u64 { self.total_supply }
    pub fn is_key_image_spent(&self, ki: &[u8; 32]) -> bool { self.spent_key_images.contains(ki) }

    pub fn genesis(coinbase_amount: u64, coinbase_public_key: &[u8]) -> Self {
        let mut state = UtxoSet::new();
        let tx = Transaction {
            version: 1, inputs: vec![],
            outputs: vec![TxOutput { amount: coinbase_amount, public_key: coinbase_public_key.to_vec() }],
            ring_size: 1, signatures: vec![],
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
    use crate::block::{TxInput, TxOutput, Transaction};
    use ed25519_dalek::Signer;

    fn keypair() -> ed25519_dalek::Keypair {
        ed25519_dalek::Keypair::generate(&mut rand::thread_rng())
    }

    fn out(amounts: &[u64], pk: &[u8]) -> Vec<TxOutput> {
        amounts.iter().map(|&a| TxOutput { amount: a, public_key: pk.to_vec() }).collect()
    }

    fn signed_tx(inputs: Vec<TxInput>, outputs: Vec<TxOutput>, kp: &ed25519_dalek::Keypair) -> Transaction {
        let mut tx = Transaction { version: 1, inputs, outputs, ring_size: 1, signatures: vec![] };
        let msg = tx_msg(&tx);
        let sig = kp.sign(&msg);
        tx.signatures = vec![sig.to_bytes().to_vec()];
        tx
    }

    #[test] fn test_empty() { let s = UtxoSet::new(); assert_eq!(s.utxo_count(), 0); }

    #[test] fn test_spend_with_signature() {
        let mut s = UtxoSet::new();
        let kp = keypair();
        let pk = kp.verifying_key().to_bytes().to_vec();
        let tx = Transaction { version: 1, inputs: vec![], outputs: out(&[5000], &pk), ring_size: 1, signatures: vec![] };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        // Spend with proper signature
        let spend = signed_tx(vec![TxInput { previous_tx_hash: h, output_index: 0, key_image: [0xab;32] }],
            out(&[3000], &kp.verifying_key().to_bytes().to_vec()), &kp);
        assert!(s.spend_transaction_inputs(&spend).is_ok());
        assert_eq!(s.get_balance(&pk), 0);
    }

    #[test] fn test_wrong_signature_rejected() {
        let mut s = UtxoSet::new();
        let kp = keypair();
        let pk = kp.verifying_key().to_bytes().to_vec();
        let tx = Transaction { version: 1, inputs: vec![], outputs: out(&[5000], &pk), ring_size: 1, signatures: vec![] };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        // Spend with WRONG keypair
        let wrong_kp = keypair();
        let spend = signed_tx(vec![TxInput { previous_tx_hash: h, output_index: 0, key_image: [0xcd;32] }],
            out(&[3000], &wrong_kp.verifying_key().to_bytes().to_vec()), &wrong_kp);
        assert!(s.spend_transaction_inputs(&spend).is_err());
    }

    #[test] fn test_double_spend() {
        let mut s = UtxoSet::new();
        let kp = keypair();
        let pk = kp.verifying_key().to_bytes().to_vec();
        let tx = Transaction { version: 1, inputs: vec![], outputs: out(&[5000], &pk), ring_size: 1, signatures: vec![] };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        let sp1 = signed_tx(vec![TxInput { previous_tx_hash: h, output_index: 0, key_image: [0xab;32] }],
            out(&[3000], &pk), &kp);
        s.spend_transaction_inputs(&sp1).unwrap();
        let sp2 = signed_tx(vec![TxInput { previous_tx_hash: h, output_index: 0, key_image: [0xcd;32] }],
            out(&[3000], &pk), &kp);
        assert!(s.spend_transaction_inputs(&sp2).is_err());
    }

    #[test] fn test_genesis() {
        let s = UtxoSet::genesis(100_000_000_000, &[0u8; 32]);
        assert_eq!(s.utxo_count(), 1);
        assert_eq!(s.total_supply(), 100_000_000_000);
    }
}
