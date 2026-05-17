use serde::{Serialize, Deserialize};
use serde::Deserializer;
use ed25519_dalek::{Verifier, VerifyingKey, Signature, SigningKey, SecretKey};
use std::collections::{HashMap, HashSet};
use rand::RngCore;
use crate::block::{Transaction, TxInput, TxOutput, Block};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoEntry {
    pub amount: u64, pub public_key: Vec<u8>, pub block_height: u64,
    pub tx_index: u32, pub output_index: u32,
}
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct UtxoKey { pub tx_hash: [u8; 32], pub output_index: u32 }

impl Serialize for UtxoKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let hex_str: String = self.tx_hash.iter().map(|b| format!("{:02x}", b)).collect();
        s.serialize_str(&format!("{}_{}", hex_str, self.output_index))
    }
}
impl<'de> Deserialize<'de> for UtxoKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let parts: Vec<&str> = s.split('_').collect();
        if parts.len() != 2 { return Err(serde::de::Error::custom("invalid key")); }
        let mut hash = [0u8; 32];
        for i in 0..32 {
            hash[i] = u8::from_str_radix(&parts[0][i*2..i*2+2], 16).map_err(serde::de::Error::custom)?;
        }
        Ok(UtxoKey { tx_hash: hash, output_index: parts[1].parse().map_err(serde::de::Error::custom)? })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UtxoSet {
    utxos: HashMap<UtxoKey, UtxoEntry>,
    spent_key_images: HashSet<[u8; 32]>,
    total_supply: u64,
}

pub fn tx_msg(tx: &Transaction) -> Vec<u8> {
    let mut msg = Vec::new();
    for i in &tx.inputs { msg.extend_from_slice(&i.previous_tx_hash); msg.extend_from_slice(&i.output_index.to_le_bytes()); }
    for o in &tx.outputs { msg.extend_from_slice(&o.amount.to_le_bytes()); msg.extend_from_slice(&o.pubkey_hash); }
    msg.extend_from_slice(&tx.ring_size.to_le_bytes());
    msg
}

fn make_signing_key() -> SigningKey {
    let mut b = [0u8; 32]; rand::thread_rng().fill_bytes(&mut b);
    SigningKey::from_bytes(&b)
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
                UtxoEntry{amount:o.amount,public_key:o.pubkey_hash.to_vec(),block_height:bh,tx_index:ti,output_index:i as u32});
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
    pub fn total_supply(&self) -> u64 { self.total_supply }
    pub fn get_balance(&self, pk: &[u8]) -> u64 {
        self.utxos.values().filter(|u| u.public_key == pk).map(|u| u.amount).sum()
    }
    pub fn utxo_count(&self) -> usize { self.utxos.len() }
    pub fn utxo_keys_for(&self, pk: &[u8]) -> Vec<UtxoKey> {
        self.utxos.iter()
            .filter(|(_, e)| e.public_key.as_slice() == pk)
            .map(|(k, _)| k.clone())
            .collect()
    }
    pub fn get_utxo(&self, key: &UtxoKey) -> Option<&UtxoEntry> {
        self.utxos.get(key)
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
    pub fn genesis(a: u64, pk: &[u8]) -> Self {
        let mut s = UtxoSet::new();
        let tx = Transaction{version:1,inputs:vec![],outputs:vec![TxOutput{amount:a,pubkey_hash:pk[..20].try_into().unwrap(),spendable_after:0}],ring_size:1,signatures:vec![]};
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0); s.add_coinbase_supply(a); s
    }
}

#[cfg(test)]
mod tests {
    use super::*; use ed25519_dalek::Signer;
    fn out(v:&[u64],pk:&[u8])->Vec<TxOutput>{v.iter().map(|&a|TxOutput{amount:a,pubkey_hash:pk[..20].try_into().unwrap(),spendable_after:0}).collect()}
    fn mk_tx(inp:Vec<TxInput>,out:Vec<TxOutput>,sk:&SigningKey)->Transaction {
        let mut tx = Transaction{version:1,inputs:inp,outputs:out,ring_size:1,signatures:vec![]};
        let sig = sk.sign(&tx_msg(&tx)); tx.signatures = vec![sig.to_bytes().to_vec()]; tx
    }
    #[test] fn test_spend() {
        let mut s = UtxoSet::new();
        let sk = make_signing_key(); let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction{version:1,inputs:vec![],outputs:out(&[5000],&pk),ring_size:1,signatures:vec![]};
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0);
        assert!(s.spend_transaction_inputs(&mk_tx(vec![TxInput{previous_tx_hash:h,output_index:0,key_image:[0xab;32]}],out(&[3000],&pk),&sk)).is_ok());
    }
    #[test] fn test_wrong_sig() {
        let mut s = UtxoSet::new();
        let sk = make_signing_key(); let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction{version:1,inputs:vec![],outputs:out(&[5000],&pk),ring_size:1,signatures:vec![]};
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0);
        let wrong = make_signing_key();
        assert!(s.spend_transaction_inputs(&mk_tx(vec![TxInput{previous_tx_hash:h,output_index:0,key_image:[0xcd;32]}],out(&[3000],&pk),&wrong)).is_err());
    }
    #[test] fn test_double_spend() {
        let mut s = UtxoSet::new();
        let sk = make_signing_key(); let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction{version:1,inputs:vec![],outputs:out(&[5000],&pk),ring_size:1,signatures:vec![]};
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0);
        assert!(s.spend_transaction_inputs(&mk_tx(vec![TxInput{previous_tx_hash:h,output_index:0,key_image:[0xab;32]}],out(&[3000],&pk),&sk)).is_ok());
        assert!(s.spend_transaction_inputs(&mk_tx(vec![TxInput{previous_tx_hash:h,output_index:0,key_image:[0xcd;32]}],out(&[3000],&pk),&sk)).is_err());
    }
    #[test] fn test_supply() {
        let s = UtxoSet::genesis(100_000_000_000, &[0;32]);
        assert_eq!(s.total_supply(), 100_000_000_000);
    }
}
