use serde::{Serialize, Deserialize};
use curve25519_dalek::ristretto::CompressedRistretto;

use ed25519_dalek::{Verifier, VerifyingKey, Signature, SigningKey};
use std::collections::{HashMap, HashSet};
use rand::RngCore;
use crate::block::{Transaction, TxOutput, Block, UtxoRef};
#[cfg(test)] use crate::block::TxInput;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoEntry {
    pub amount: u64,
    pub public_key: Vec<u8>,
    pub spendable_after: u64,
    pub block_height: u64,
    pub tx_index: u32,
    pub output_index: u32,
    /// Private mode: stealth destination bytes.
    pub stealth_dest: Option<[u8; 32]>,
    /// Private mode: ephemeral key R (for one-time key recovery).
    pub ephemeral: Option<[u8; 32]>,
    /// Private mode: commitment bytes.
    pub commitment_bytes: Option<[u8; 32]>,
}

fn utxo_is_spendable(utxo: &UtxoEntry, current_block: u64) -> bool {
    current_block >= utxo.spendable_after
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

/// Build the message to be signed/hashed for a transaction.
/// Used for both ed25519 legacy signing and MLSAG message.
pub fn tx_msg(tx: &Transaction) -> Vec<u8> {
    let mut msg = Vec::new();
    for i in &tx.inputs {
        msg.extend_from_slice(&i.previous_tx_hash);
        msg.extend_from_slice(&i.output_index.to_le_bytes());
    }
    for o in &tx.outputs {
        msg.extend_from_slice(&o.amount.to_le_bytes());
        if !o.public_key.is_empty() {
            msg.extend_from_slice(&o.public_key);
        }
        if let Some(d) = &o.stealth_dest {
            msg.extend_from_slice(d);
        }
        if let Some(c) = &o.commitment_bytes {
            msg.extend_from_slice(c);
        }
    }
    msg.extend_from_slice(&tx.ring_size.to_le_bytes());
    msg
}

#[allow(dead_code)]
fn make_signing_key() -> SigningKey {
    let mut b = [0u8; 32]; rand::thread_rng().fill_bytes(&mut b);
    SigningKey::from_bytes(&b)
}

/// Verify a transaction signature using ed25519 (legacy public mode).
pub fn verify_tx_signature(tx: &Transaction, pubkey_bytes: &[u8]) -> Result<(), String> {
    if tx.signatures.is_empty() { return Err("sem assinatura".into()); }
    let pk_bytes: [u8; 32] = pubkey_bytes.try_into().map_err(|_| "chave invalida")?;
    let pk = VerifyingKey::from_bytes(&pk_bytes).map_err(|_| "chave publica invalida")?;
    let sig = Signature::from_slice(&tx.signatures[0]).map_err(|_| "assinatura invalida")?;
    pk.verify(&tx_msg(tx), &sig).map_err(|_| "assinatura nao confere".to_string())
}

/// Verify an MLSAG ring signature against a ring of public keys.
/// `ring_pubkeys`: for each layer, the set of pubkeys forming the ring.
fn verify_mlsag(
    mlsag: &crate::block::MlsagData,
    ring_pubkeys: &[Vec<curve25519_dalek::ristretto::RistrettoPoint>],
    msg: &[u8],
) -> bool {
    
    let sig = mlsag.to_sig();
    sig.verify(ring_pubkeys, msg)
}

/// Look up ring member public keys from the UTXO set.
pub fn build_ring_inline(
    utxo_set: &HashMap<UtxoKey, UtxoEntry>,
    members: &[UtxoRef],
) -> Result<Vec<Vec<curve25519_dalek::ristretto::RistrettoPoint>>, String> {
    
    

    let mut ring = Vec::with_capacity(members.len());
    for m in members {
        let key = UtxoKey { tx_hash: m.tx_hash, output_index: m.output_index };
        let entry = utxo_set.get(&key).ok_or_else(|| format!("Ring UTXO not found: {:x}..{}", m.tx_hash[0], m.output_index))?;

        let pk = if let Some(sd) = &entry.stealth_dest {
            // Private mode: stealth dest is the pubkey
            CompressedRistretto(*sd).decompress()
                .ok_or_else(|| "Invalid stealth point in ring".to_string())?
        } else if entry.public_key.len() == 32 {
            // Legacy: convert ed25519 pubkey bytes to Ristretto... 
            // This is a hack — ed25519 keys aren't the same as Ristretto.
            // For now, just hash the public key to a Ristretto point.
            crate::privacy::hash_to_point(&entry.public_key)
        } else {
            // Fallback: hash everything to a point
            let mut data = Vec::new();
            data.extend_from_slice(&entry.amount.to_le_bytes());
            data.extend_from_slice(&entry.public_key);
            crate::privacy::hash_to_point(&data)
        };
        ring.push(vec![pk]);
    }
    Ok(ring)
}

impl UtxoSet {
    pub fn new() -> Self {
        UtxoSet { utxos: HashMap::new(), spent_key_images: HashSet::new(), total_supply: 0 }
    }

    pub fn add_transaction_outputs(&mut self, h: &[u8;32], tx: &Transaction, bh: u64, ti: u32) {
        for (i, o) in tx.outputs.iter().enumerate() {
            self.utxos.insert(
                UtxoKey { tx_hash: *h, output_index: i as u32 },
                UtxoEntry {
                    amount: o.amount,
                    public_key: o.public_key.to_vec(),
                    spendable_after: o.spendable_after,
                    block_height: bh,
                    tx_index: ti,
                    output_index: i as u32,
                    stealth_dest: o.stealth_dest,
                    ephemeral: o.ephemeral,
                    commitment_bytes: o.commitment_bytes,
                }
            );
        }
    }

    pub fn add_coinbase_supply(&mut self, a: u64) {
        self.total_supply = self.total_supply.checked_add(a).unwrap_or(self.total_supply);
    }

    /// Spend transaction inputs, verifying both public (ed25519) and private (MLSAG) sigs.
    pub fn spend_transaction_inputs(&mut self, tx: &Transaction, current_block: u64) -> Result<(), String> {
        // Pre-check: if private mode, verify MLSAG once before spending individual inputs
        if let Some(ref mlsag) = tx.mlsag {
            let ring_members = tx.ring_members.as_ref()
                .ok_or("Private tx without ring members")?;
            let msg = tx_msg(tx);

            // Build ring for each layer (MLSAG combines all inputs into one sig)
            // For single-input txs: one ring of size ring_size
            // For multi-input txs: n_layers = inputs.len()
            let mut all_rings = Vec::new();
            for (_input_idx, members_for_input) in ring_members.iter().enumerate() {
                let ring = build_ring_inline(&self.utxos, members_for_input)?;
                // Take only the first layer from each ring member (n_layers=1 per input)
                let layer_ring: Vec<curve25519_dalek::ristretto::RistrettoPoint> = ring.iter().map(|r| r[0]).collect();
                all_rings.push(layer_ring);
            }

            // Transpose: MLSAG expects ring[ring_pos][layer]
            // We have all_rings[input_idx][ring_pos]
            // We need ring[ring_pos][layer_idx]
            if all_rings.is_empty() {
                return Err("No rings to verify".into());
            }
            let ring_size = all_rings[0].len();
            let n_layers = all_rings.len();
            let mut ring_formatted = vec![Vec::with_capacity(n_layers); ring_size];
            for ring_pos in 0..ring_size {
                for layer in 0..n_layers {
                    ring_formatted[ring_pos].push(all_rings[layer][ring_pos]);
                }
            }

            if !verify_mlsag(mlsag, &ring_formatted, &msg) {
                return Err("MLSAG signature invalid".into());
            }
        }

        // Check individual inputs
        for input in &tx.inputs {
            if self.spent_key_images.contains(&input.key_image) {
                return Err("Double-spend".into());
            }
            let key = UtxoKey { tx_hash: input.previous_tx_hash, output_index: input.output_index };
            let utxo = self.utxos.get(&key).ok_or("UTXO not found")?;
            if !utxo_is_spendable(utxo, current_block) {
                return Err("UTXO time-locked".into());
            }

            // Legacy sig verification (only if not using MLSAG for this tx)
            if tx.mlsag.is_none() {
                verify_tx_signature(tx, &utxo.public_key)?;
            }

            // For private mode, also check amount conservation via commitments (TODO)
            // This requires range proof verification on each output
        }

        // Apply spends
        for input in &tx.inputs {
            let key = UtxoKey { tx_hash: input.previous_tx_hash, output_index: input.output_index };
            self.utxos.remove(&key);
            self.spent_key_images.insert(input.key_image);
        }

        // Verify range proofs on outputs (private mode)
        if tx.mlsag.is_some() {
            for o in &tx.outputs {
                // Range proof verification would go here
                // For now, just check that private outputs have the required fields
                if o.is_private() {
                    if o.commitment_bytes.is_none() {
                        return Err("Private output missing commitment".into());
                    }
                    if o.range_proof_bytes.is_none() {
                        return Err("Private output missing range proof".into());
                    }
                }
            }
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

    /// Access the internal UTXO map (for mempool ring building).
    pub fn utxos_map(&self) -> &std::collections::HashMap<UtxoKey, UtxoEntry> {
        &self.utxos
    }

    pub fn apply_block(&mut self, block: &Block, block_height: u64) -> Result<(), String> {
        for (tx_idx, tx) in block.body.transactions.iter().enumerate() {
            let tx_hash = tx.hash();
            if tx_idx == 0 {
                // Coinbase
                self.add_transaction_outputs(&tx_hash, tx, block_height, tx_idx as u32);
                let coinbase_amount: u64 = tx.outputs.iter().map(|o| o.amount).sum();
                self.add_coinbase_supply(coinbase_amount);
            } else {
                self.spend_transaction_inputs(tx, block_height)?;
                self.add_transaction_outputs(&tx_hash, tx, block_height, tx_idx as u32);
            }
        }
        Ok(())
    }

    pub fn validate_transaction(&self, tx: &Transaction) -> Result<(), String> {
        if tx.inputs.is_empty() && tx.outputs.is_empty() {
            return Err("Empty tx".into());
        }
        let (mut ins, mut outs) = (0u64, 0u64);
        for i in &tx.inputs {
            let key = UtxoKey { tx_hash: i.previous_tx_hash, output_index: i.output_index };
            let u = self.utxos.get(&key).ok_or("UTXO not found")?;
            ins = ins.checked_add(u.amount).ok_or("overflow")?;
            if self.spent_key_images.contains(&i.key_image) {
                return Err("Double-spend".into());
            }
        }
        for o in &tx.outputs { outs = outs.checked_add(o.amount).ok_or("overflow")?; }
        if !tx.inputs.is_empty() && ins < outs { return Err("creates money".into()); }
        Ok(())
    }

    pub fn genesis(a: u64, pk: &[u8]) -> Self {
        let mut s = UtxoSet::new();
        let tx = Transaction {
            version: 1, inputs: vec![], outputs: vec![TxOutput {
                amount: a, public_key: pk.try_into().unwrap(), spendable_after: 0,
                stealth_dest: None, commitment_bytes: None, range_proof_bytes: None, ephemeral: None,
            }], ring_size: 1, signatures: vec![], mlsag: None, ring_members: None,
        };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        s.add_coinbase_supply(a);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    

    fn out(v: &[u64], pk: &[u8]) -> Vec<TxOutput> {
        v.iter().map(|&a| TxOutput {
            amount: a, public_key: pk.try_into().unwrap(), spendable_after: 0,
            stealth_dest: None, commitment_bytes: None, range_proof_bytes: None, ephemeral: None,
        }).collect()
    }

    fn mk_tx(inp: Vec<TxInput>, out: Vec<TxOutput>, sk: &SigningKey) -> Transaction {
        let mut tx = Transaction {
            version: 1, inputs: inp, outputs: out,
            ring_size: 1, signatures: vec![],
            mlsag: None, ring_members: None,
        };
        let sig = sk.sign(&tx_msg(&tx));
        tx.signatures = vec![sig.to_bytes().to_vec()];
        tx
    }

    #[test] fn test_spend() {
        let mut s = UtxoSet::new();
        let sk = make_signing_key(); let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction {
            version: 1, inputs: vec![], outputs: out(&[5000], &pk),
            ring_size: 1, signatures: vec![], mlsag: None, ring_members: None,
        };
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0);
        assert!(s.spend_transaction_inputs(&mk_tx(
            vec![TxInput { previous_tx_hash: h, output_index: 0, key_image: [0xab; 32] }],
            out(&[3000], &pk), &sk), 1000).is_ok());
    }

    #[test] fn test_wrong_sig() {
        let mut s = UtxoSet::new();
        let sk = make_signing_key(); let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction {
            version: 1, inputs: vec![], outputs: out(&[5000], &pk),
            ring_size: 1, signatures: vec![], mlsag: None, ring_members: None,
        };
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0);
        let wrong = make_signing_key();
        assert!(s.spend_transaction_inputs(&mk_tx(
            vec![TxInput { previous_tx_hash: h, output_index: 0, key_image: [0xcd; 32] }],
            out(&[3000], &pk), &wrong), 1000).is_err());
    }

    #[test] fn test_double_spend() {
        let mut s = UtxoSet::new();
        let sk = make_signing_key(); let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction {
            version: 1, inputs: vec![], outputs: out(&[5000], &pk),
            ring_size: 1, signatures: vec![], mlsag: None, ring_members: None,
        };
        let h = tx.hash(); s.add_transaction_outputs(&h, &tx, 0, 0);
        assert!(s.spend_transaction_inputs(&mk_tx(
            vec![TxInput { previous_tx_hash: h, output_index: 0, key_image: [0xab; 32] }],
            out(&[3000], &pk), &sk), 1000).is_ok());
        assert!(s.spend_transaction_inputs(&mk_tx(
            vec![TxInput { previous_tx_hash: h, output_index: 0, key_image: [0xcd; 32] }],
            out(&[3000], &pk), &sk), 1000).is_err());
    }

    #[test] fn test_supply() {
        let s = UtxoSet::genesis(100_000_000_000, &[0; 32]);
        assert_eq!(s.total_supply(), 100_000_000_000);
    }
}
