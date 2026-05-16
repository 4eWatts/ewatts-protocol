use ed25519_dalek::{Verifier, VerifyingKey, Signature};
use std::collections::{HashMap, HashSet};
use crate::block::{Transaction, TxInput, TxOutput, Block};

#[derive(Debug, Clone)]
pub struct UtxoEntry {
    pub amount: u64, pub public_key: Vec<u8>, pub block_height: u64,
    pub tx_index: u32, pub output_index: u32,
}
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct UtxoKey { pub tx_hash: [u8; 32], pub output_index: u32 }

#[derive(Debug)]
pub struct UtxoSet {
    utxos: HashMap<UtxoKey, UtxoEntry>,
    spent_key_images: HashSet<[u8; 32]>,
    total_supply: u64,
}

pub fn tx_msg(tx: &Transaction) -> Vec<u8> {
    let mut msg = Vec::new();
    for i in &tx.inputs { msg.extend_from_slice(&i.previous_tx_hash); msg.extend_from_slice(&i.output_index.to_le_bytes()); }
    for o in &tx.outputs { msg.extend_from_slice(&o.amount.to_le_bytes()); msg.extend_from_slice(&o.public_key); }
    msg.extend_from_slice(&tx.ring_size.to_le_bytes());
    msg
}

pub fn verify_tx_signature(tx: &Transaction, pubkey_bytes: &[u8]) -> Result<(), String> {
    if tx.inputs.is_empty() { return Ok(()); }
    if tx.signatures.is_empty() { return Err("sem assinatura".into()); }
    let pk_bytes: [u8; 32] = pubkey_bytes.try_into().map_err(|_| "chave invalida")?;
    let pk = VerifyingKey::from_bytes(&pk_bytes).map_err(|_| "chave publica invalida")?;
    let sig = Signature::from_slice(&tx.signatures[0]).map_err(|_| "assinatura invalida")?;
    pk.verify(&tx_msg(tx), &sig).map_err(|_| "assinatura nao confere".to_string())
}

impl UtxoSet {
    pub fn new() -> Self { UtxoSet { utxos: HashMap::new(), spent_key_images: HashSet::new(), total_supply: 0 } }
    pub fn add_transaction_outputs(&mut self, h: &[u8;32], tx: &Transaction, bh: u64, ti: u32) {
        for (i,o) in tx.outputs.iter().enumerate() {
            self.utxos.insert(UtxoKey{tx_hash:*h,output_index:i as u32},
                UtxoEntry{amount:o.amount,public_key:o.public_key.clone(),block_height:bh,tx_index:ti,output_index:i as u32});
        }
    }
    pub fn add_coinbase_supply(&mut self, a: u64) { self.total_supply = self.total_supply.checked_add(a).unwrap_or(self.total_supply); }
    pub fn spend_transaction_inputs(&mut self, tx: &Transaction) -> Result<(), String> {
        for input in &tx.inputs {
            if self.spent_key_images.contains(&input.key_image) { return Err("Double-spend".into()); }
            let key = UtxoKey{tx_hash:input.previous_tx_hash,output_index:input.output_index};
            let utxo = self.utxos.get(&key).ok_or("UTXO not found")?;
            verify_tx_signature(tx, &utxo.public_key)?;
        }
        for input in &tx.inputs {
            let key = UtxoKey{tx_hash:input.previous_tx_hash,output_index:input.output_index};
            self.utxos.remove(&key); self.spent_key_images.insert(input.key_image);
        }
        Ok(())
    }
    pub fn validate_transaction(&self, tx: &Transaction) -> Result<(), String> {
        if tx.inputs.is_empty() && tx.outputs.is_empty() { return Err("Empty tx".into()); }
        let (mut ins, mut outs) = (0u64, 0u64);
        for i in &tx.inputs {
            let key = UtxoKey{tx_hash:i.previous_tx_hash,output_index:i.output_index};
            let u = self.utxos.get(&key).ok_or("UTXO not found")?;
            ins = ins.checked_add(u.amount).ok_or("overflow")?;
            if self.spent_key_images.contains(&i.key_image) { return Err("Double-spend".into()); }
        }
        for o in &tx.outputs { outs = outs.checked_add(o.amount).ok_or("overflow")?; }
        if !tx.inputs.is_empty() && ins < outs { return Err("creates money".into()); }
        Ok(())
    }
    pub fn apply_block(&mut self, block: &Block, bh: u64) -> Result<(), String> {
        for (ti, tx) in block.body.transactions.iter().enumerate() {
            let h = tx.hash();
            if ti == 0 { self.add_transaction_outputs(&h, tx, bh, ti as u32);
                self.add_coinbase_supply(tx.outputs.iter().map(|o|o.amount).sum()); }
            else { self.spend_transaction_inputs(tx)?; self.add_transaction_outputs(&h, tx, bh, ti as u32); }
        }
        Ok(())
    }
    pub fn get_balance(&self, pk: &[u8]) -> u64 { self.utxos.values().filter(|u| u.public_key == pk).map(|u| u.amount).sum() }
    pub fn utxo_count(&self) -> usize { self.utxos.len() }
    pub fn total_supply(&self) -> u64 { self.total_supply }
    pub fn genesis(a: u64, pk: &[u8]) -> Self {
        let mut s = UtxoSet::new();
        let tx = Transaction{version:1,inputs:vec![],outputs:vec![TxOutput{amount:a,public_key:pk.to_vec()}],ring_size:1,signatures:vec![]};
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0); s.add_coinbase_supply(a); s
    }
}

#[cfg(test)]
mod tests {
    use super::*; use ed25519_dalek::Signer;
    fn out(v:&[u64],pk:&[u8])->Vec<TxOutput>{v.iter().map(|&a|TxOutput{amount:a,public_key:pk.to_vec()}).collect()}
    fn mk_tx(inp:Vec<TxInput>,out:Vec<TxOutput>,sk:&ed25519_dalek::SigningKey)->Transaction {
        let mut tx = Transaction{version:1,inputs:inp,outputs:out,ring_size:1,signatures:vec![]};
        let sig = sk.sign(&tx_msg(&tx)); tx.signatures = vec![sig.to_bytes().to_vec()]; tx
    }
    #[test] fn test_spend() {
        let mut s = UtxoSet::new();
        let sk = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction{version:1,inputs:vec![],outputs:out(&[5000],&pk),ring_size:1,signatures:vec![]};
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0);
        let sp = mk_tx(vec![TxInput{previous_tx_hash:h,output_index:0,key_image:[0xab;32]}], out(&[3000],&pk), &sk);
        assert!(s.spend_transaction_inputs(&sp).is_ok());
    }
    #[test] fn test_wrong_sig() {
        let mut s = UtxoSet::new();
        let sk = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction{version:1,inputs:vec![],outputs:out(&[5000],&pk),ring_size:1,signatures:vec![]};
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0);
        let wrong_sk = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let sp = mk_tx(vec![TxInput{previous_tx_hash:h,output_index:0,key_image:[0xcd;32]}], out(&[3000],&pk), &wrong_sk);
        assert!(s.spend_transaction_inputs(&sp).is_err());
    }
    #[test] fn test_double_spend() {
        let mut s = UtxoSet::new();
        let sk = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction{version:1,inputs:vec![],outputs:out(&[5000],&pk),ring_size:1,signatures:vec![]};
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0);
        let sp = mk_tx(vec![TxInput{previous_tx_hash:h,output_index:0,key_image:[0xab;32]}],out(&[3000],&pk),&sk);
        s.spend_transaction_inputs(&sp).unwrap();
        let sp2 = mk_tx(vec![TxInput{previous_tx_hash:h,output_index:0,key_image:[0xcd;32]}],out(&[3000],&pk),&sk);
        assert!(s.spend_transaction_inputs(&sp2).is_err());
    }
    #[test] fn test_total_supply() {
        let s = UtxoSet::genesis(100_000_000_000, &[0;32]);
        assert_eq!(s.total_supply(), 100_000_000_000);
    }
}
