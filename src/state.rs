use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::block::TxInput;
use crate::block::{Block, Transaction, TxOutput, UtxoRef};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use std::collections::{HashMap, HashSet};

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

impl UtxoEntry {
    /// Decompress the stealth destination point, if present.
    pub fn stealth_dest_point(&self) -> Option<RistrettoPoint> {
        self.stealth_dest
            .and_then(|sd| curve25519_dalek::ristretto::CompressedRistretto(sd).decompress())
    }
}

fn utxo_is_spendable(utxo: &UtxoEntry, current_block: u64) -> bool {
    current_block >= utxo.spendable_after
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct UtxoKey {
    pub tx_hash: [u8; 32],
    pub output_index: u32,
}

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
        if parts.len() != 2 {
            return Err(serde::de::Error::custom("invalid key"));
        }
        if parts[0].len() != 64 {
            return Err(serde::de::Error::custom("invalid hash length"));
        }
        let mut hash = [0u8; 32];
        for i in 0..32 {
            hash[i] = u8::from_str_radix(&parts[0][i * 2..i * 2 + 2], 16)
                .map_err(serde::de::Error::custom)?;
        }
        Ok(UtxoKey {
            tx_hash: hash,
            output_index: parts[1].parse().map_err(serde::de::Error::custom)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        if let Some(rp) = &o.range_proof_bytes {
            msg.extend_from_slice(rp);
        }
        if let Some(e) = &o.ephemeral {
            msg.extend_from_slice(e);
        }
    }
    msg.extend_from_slice(&tx.ring_size.to_le_bytes());
    msg
}

#[allow(dead_code)]
fn make_signing_key() -> SigningKey {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    SigningKey::from_bytes(&b)
}

/// Verify a transaction signature using ed25519 (legacy public mode).
pub fn verify_tx_signature(tx: &Transaction, pubkey_bytes: &[u8]) -> Result<(), String> {
    if tx.signatures.is_empty() {
        return Err("sem assinatura".into());
    }
    let pk_bytes: [u8; 32] = pubkey_bytes.try_into().map_err(|_| "chave invalida")?;
    let pk = VerifyingKey::from_bytes(&pk_bytes).map_err(|_| "chave publica invalida")?;
    let sig = Signature::from_slice(&tx.signatures[0]).map_err(|_| "assinatura invalida")?;
    pk.verify(&tx_msg(tx), &sig)
        .map_err(|_| "assinatura nao confere".to_string())
}

/// Verify an MLSAG ring signature against a ring of public keys.
/// `ring_pubkeys`: for each layer, the set of pubkeys forming the ring.
fn verify_mlsag(
    mlsag: &crate::block::MlsagData,
    ring_pubkeys: &[Vec<curve25519_dalek::ristretto::RistrettoPoint>],
    msg: &[u8],
) -> bool {
    let sig = match mlsag.to_sig() {
        Ok(s) => s,
        Err(_) => return false,
    };
    sig.verify(ring_pubkeys, msg)
}

/// Look up ring member public keys from the UTXO set.
pub fn build_ring_inline(
    utxo_set: &HashMap<UtxoKey, UtxoEntry>,
    members: &[UtxoRef],
) -> Result<Vec<Vec<curve25519_dalek::ristretto::RistrettoPoint>>, String> {
    let mut ring = Vec::with_capacity(members.len());
    for m in members {
        let key = UtxoKey {
            tx_hash: m.tx_hash,
            output_index: m.output_index,
        };
        let entry = utxo_set.get(&key).ok_or_else(|| {
            format!(
                "Ring UTXO not found: {:x}..{}",
                m.tx_hash[0], m.output_index
            )
        })?;

        let pk = if let Some(sd) = &entry.stealth_dest {
            // Private mode: stealth dest is the pubkey
            CompressedRistretto(*sd)
                .decompress()
                .ok_or_else(|| "Invalid stealth point in ring".to_string())?
        } else {
            return Err(format!(
                "Ring member {:x}..{} is a legacy (non-stealth) UTXO. Only private UTXOs can be ring members.",
                m.tx_hash[0], m.output_index
            ));
        };
        ring.push(vec![pk]);
    }
    Ok(ring)
}

/// Record of changes made by a single block, used for reorg unwinding.
#[derive(Debug, Clone)]
pub struct BlockDiff {
    /// UTXOs that were consumed (key_image -> (key, entry) to restore on unwind)
    pub consumed: std::collections::HashMap<[u8; 32], (UtxoKey, UtxoEntry)>,
    /// Keys of UTXOs that were created (to remove on unwind)
    pub created: Vec<UtxoKey>,
    /// Key images that were spent (to un-mark on unwind)
    pub key_images: Vec<[u8; 32]>,
    /// Supply delta (positive = emission added, negative = burned)
    pub supply_delta: i64,
}

impl BlockDiff {
    pub fn new() -> Self {
        BlockDiff {
            consumed: std::collections::HashMap::new(),
            created: Vec::new(),
            key_images: Vec::new(),
            supply_delta: 0,
        }
    }
}

impl UtxoSet {
    pub fn new() -> Self {
        UtxoSet {
            utxos: HashMap::new(),
            spent_key_images: HashSet::new(),
            total_supply: 0,
        }
    }

    pub fn add_transaction_outputs(&mut self, h: &[u8; 32], tx: &Transaction, bh: u64, ti: u32) {
        for (i, o) in tx.outputs.iter().enumerate() {
            self.utxos.insert(
                UtxoKey {
                    tx_hash: *h,
                    output_index: i as u32,
                },
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
                },
            );
        }
    }

    pub fn add_coinbase_supply(&mut self, a: u64) {
        self.total_supply = self
            .total_supply
            .checked_add(a)
            .unwrap_or(self.total_supply);
    }

    /// Spend transaction inputs, verifying both public (ed25519) and private (MLSAG) sigs.
    pub fn spend_transaction_inputs(
        &mut self,
        tx: &Transaction,
        current_block: u64,
    ) -> Result<(), String> {
        self.spend_transaction_inputs_with_diff(tx, current_block, None)
    }

    /// Same as spend_transaction_inputs but also populates a BlockDiff for each
    /// individual mutation, ensuring that if a later input spend fails, the partial
    /// diff is consistent for rollback (atomicity).
    pub fn spend_transaction_inputs_with_diff(
        &mut self,
        tx: &Transaction,
        current_block: u64,
        mut diff: Option<&mut crate::state::BlockDiff>,
    ) -> Result<(), String> {
        // Pre-check: if private mode, verify MLSAG once before spending individual inputs
        if let Some(ref mlsag) = tx.mlsag {
            let ring_members = tx
                .ring_members
                .as_ref()
                .ok_or("Private tx without ring members")?;
            let msg = tx_msg(tx);

            // Build ring for each layer (MLSAG combines all inputs into one sig)
            // For single-input txs: one ring of size ring_size
            // For multi-input txs: n_layers = inputs.len()
            let mut all_rings = Vec::new();
            for (_input_idx, members_for_input) in ring_members.iter().enumerate() {
                let ring = build_ring_inline(&self.utxos, members_for_input)?;
                // Take only the first layer from each ring member (n_layers=1 per input)
                let layer_ring: Vec<curve25519_dalek::ristretto::RistrettoPoint> =
                    ring.iter().map(|r| r[0]).collect();
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
            let key = UtxoKey {
                tx_hash: input.previous_tx_hash,
                output_index: input.output_index,
            };
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

        // P0-D: verify range proofs on all private outputs
        for o in &tx.outputs {
            if o.is_private() {
                let rp_bytes = o
                    .range_proof_bytes
                    .as_ref()
                    .ok_or("Private output missing range proof")?;
                let cb = o
                    .commitment_bytes
                    .as_ref()
                    .ok_or("Private output missing commitment")?;
                let proof: crate::privacy::RangeProof = serde_json::from_slice(rp_bytes)
                    .map_err(|e| format!("Invalid range proof encoding: {}", e))?;
                let comm_pt = curve25519_dalek::ristretto::CompressedRistretto(*cb)
                    .decompress()
                    .ok_or("Invalid commitment point")?;
                if !proof.verify(&crate::privacy::Commitment(comm_pt)) {
                    return Err("Range proof verification failed on output".into());
                }
            }
        }

        // Plaintext amount conservation check (testnet: range proofs + MLSAG + double-spend
        // prevention provide the security model. A proper Pedersen balance proof with
        // consistent blinding factors can be added when encrypted blinding storage lands.)
        let has_private_outputs = tx.outputs.iter().any(|o| o.is_private());
        // Reject hybrid txs: if any output is private, ALL must be private
        if has_private_outputs {
            for o in &tx.outputs {
                if !o.is_private() {
                    return Err("Hybrid tx: all outputs must be private if any is private".into());
                }
            }
        }
        if has_private_outputs || tx.mlsag.is_some() {
            let mut input_amount = 0u64;
            for i in &tx.inputs {
                let key = UtxoKey {
                    tx_hash: i.previous_tx_hash,
                    output_index: i.output_index,
                };
                let u = self.utxos.get(&key).ok_or("UTXO not found")?;
                input_amount = input_amount.checked_add(u.amount).ok_or("overflow")?;
            }
            let mut output_amount = 0u64;
            for o in &tx.outputs {
                output_amount = output_amount.checked_add(o.amount).ok_or("overflow")?;
            }
            if input_amount < output_amount {
                return Err("Output amount exceeds input amount (inflation attack)".into());
            }
        }

        // Apply spends (atomic tracking: record in diff BEFORE each mutation)
        for input in &tx.inputs {
            let key = UtxoKey {
                tx_hash: input.previous_tx_hash,
                output_index: input.output_index,
            };
            // Track in diff BEFORE removing (so partial rollback knows what to restore)
            if let Some(ref mut d) = diff {
                if !d.consumed.contains_key(&input.key_image) {
                    if let Some(entry) = self.utxos.get(&key) {
                        d.consumed.insert(input.key_image, (key.clone(), entry.clone()));
                    }
                }
                d.key_images.push(input.key_image);
            }
            self.utxos.remove(&key);
            self.spent_key_images.insert(input.key_image);
        }

        Ok(())
    }

    pub fn total_supply(&self) -> u64 {
        self.total_supply
    }

    pub fn get_balance(&self, pk: &[u8]) -> u64 {
        self.utxos
            .values()
            .filter(|u| u.public_key == pk)
            .map(|u| u.amount)
            .sum()
    }

    pub fn utxo_count(&self) -> usize {
        self.utxos.len()
    }

    pub fn utxo_keys_for(&self, pk: &[u8]) -> Vec<UtxoKey> {
        self.utxos
            .iter()
            .filter(|(_, e)| e.public_key.as_slice() == pk)
            .map(|(k, _)| k.clone())
            .collect()
    }

    pub fn get_utxo(&self, key: &UtxoKey) -> Option<&UtxoEntry> {
        self.utxos.get(key)
    }

    /// Access spent key images (for mempool double-spend check).
    pub fn spent_key_images(&self) -> &std::collections::HashSet<[u8; 32]> {
        &self.spent_key_images
    }





    /// Subtract from total supply (for reorg unwind).
    pub fn sub_from_supply(&mut self, amount: u64) {
        self.total_supply = self.total_supply.saturating_sub(amount);
    }

    /// Access the internal UTXO map (for mempool ring building).
    pub fn utxos_map(&self) -> &std::collections::HashMap<UtxoKey, UtxoEntry> {
        &self.utxos
    }

    pub fn apply_block(&mut self, block: &Block, block_height: u64) -> Result<(), String> {
        self.apply_block_inner(block, block_height, None)
    }

    /// Apply a block and return a BlockDiff recording all changes (for reorg).
    pub fn apply_block_and_track(&mut self, block: &Block, block_height: u64) -> Result<BlockDiff, String> {
        let mut diff = BlockDiff::new();
        self.apply_block_inner(block, block_height, Some(&mut diff))?;
        Ok(diff)
    }

    fn apply_block_inner(&mut self, block: &Block, block_height: u64, mut diff: Option<&mut BlockDiff>) -> Result<(), String> {
        for (tx_idx, tx) in block.body.transactions.iter().enumerate() {
            let tx_hash = tx.hash();
            if tx_idx == 0 {
                // P0-B: coinbase must not have inputs (no spending, only creation)
                if !tx.inputs.is_empty() {
                    return Err("Coinbase must have empty inputs".into());
                }
                // P0-C: cap coinbase to maximum reasonable emission
                let coinbase_amount: u64 = tx.outputs.iter().map(|o| o.amount).sum();
                let max_emission = crate::constants::BASE_EMISSION_UNITS * 20;
                if coinbase_amount > max_emission {
                    return Err("Coinbase amount exceeds emission cap".into());
                }
                // P0-2: enforce spendable_after on coinbase outputs
                let expected_lock = crate::reward::founder_lock_block(block_height);
                for o in &tx.outputs {
                    if o.spendable_after != expected_lock {
                        return Err(format!(
                            "Coinbase spendable_after must be {} (got {})",
                            expected_lock, o.spendable_after
                        ));
                    }
                }
                self.add_transaction_outputs(&tx_hash, tx, block_height, tx_idx as u32);
                self.add_coinbase_supply(coinbase_amount);
                if let Some(ref mut d) = diff {
                    d.supply_delta = d.supply_delta.wrapping_add(coinbase_amount as i64);
                    for (i, _) in tx.outputs.iter().enumerate() {
                        d.created.push(UtxoKey { tx_hash, output_index: i as u32 });
                    }
                }
            } else {
                // P0-A: validate inputs >= outputs before spending
                self.validate_transaction(tx)?;
                // Spend inputs with atomic diff tracking inside the function.
                // Each mutation is recorded BEFORE it happens, so partial rollback
                // (via diff) is consistent even if a later input spend fails.
                self.spend_transaction_inputs_with_diff(tx, block_height, diff)?;
                // Track created UTXOs (outputs just added)
                if let Some(ref mut d) = diff {
                    for (i, _) in tx.outputs.iter().enumerate() {
                        d.created.push(UtxoKey { tx_hash, output_index: i as u32 });
                    }
                }
                self.add_transaction_outputs(&tx_hash, tx, block_height, tx_idx as u32);
            }
        }
        Ok(())
    }

    /// Unwind a block's effects using a BlockDiff (the correct way).
    /// Reverses exactly what apply_block_and_track recorded.
    pub fn unwind_with_diff(&mut self, diff: &BlockDiff) -> Result<(), String> {
        // 1. Restore consumed UTXOs (inputs that were spent)
        for (_, (key, entry)) in &diff.consumed {
            self.utxos.insert(key.clone(), entry.clone());
        }
        // 2. Remove created UTXOs (outputs that were created)
        for key in &diff.created {
            self.utxos.remove(key);
        }
        // 3. Un-mark key images
        for ki in &diff.key_images {
            self.spent_key_images.remove(ki);
        }
        // 4. Reverse supply change
        if diff.supply_delta > 0 {
            self.total_supply = self.total_supply.saturating_sub(diff.supply_delta as u64);
        }
        Ok(())
    }

    /// Reverse a block's effects (for reorg unwinding).
    /// Legacy block-level unwind. Does not restore MLSAG-hidden spent UTXOs.
    /// Prefer unwind_with_diff when a BlockDiff is available (P2P path).
    #[deprecated(since = "0.1.0", note = "use unwind_with_diff instead")]
    pub fn unwind_block(&mut self, block: &Block, _block_height: u64) -> Result<(), String> {
        // Process transactions in reverse order
        for (tx_idx, tx) in block.body.transactions.iter().enumerate().rev() {
            let tx_hash = tx.hash();
            if tx_idx == 0 {
                // Coinbase: remove created outputs, subtract supply
                let coinbase_amount: u64 = tx.outputs.iter().map(|o| o.amount).sum();
                for (i, _) in tx.outputs.iter().enumerate() {
                    let key = UtxoKey {
                        tx_hash,
                        output_index: i as u32,
                    };
                    self.utxos.remove(&key);
                }
                self.total_supply = self.total_supply.saturating_sub(coinbase_amount);
            } else {
                // Regular tx: remove created outputs, restore spent inputs
                // 1. Remove outputs created by this tx
                for (i, _) in tx.outputs.iter().enumerate() {
                    let key = UtxoKey {
                        tx_hash,
                        output_index: i as u32,
                    };
                    self.utxos.remove(&key);
                }

                // 2. For private txs: we cannot fully reverse the spent UTXOs here
                // because MLSAG hides which input was actually spent.
                // Instead, we rely on the fact that a reorg means we'll re-apply
                // a different chain, so spent key images will be restored by
                // BlockDiff tracking if we used apply_block_and_track.
                // For basic reorg, we just un-mark key_images as spent.
                for input in &tx.inputs {
                    self.spent_key_images.remove(&input.key_image);
                }
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
            let key = UtxoKey {
                tx_hash: i.previous_tx_hash,
                output_index: i.output_index,
            };
            let u = self.utxos.get(&key).ok_or("UTXO not found")?;
            ins = ins.checked_add(u.amount).ok_or("overflow")?;
            if self.spent_key_images.contains(&i.key_image) {
                return Err("Double-spend".into());
            }
        }
        for o in &tx.outputs {
            outs = outs.checked_add(o.amount).ok_or("overflow")?;
        }
        if !tx.inputs.is_empty() && ins < outs {
            return Err("creates money".into());
        }
        Ok(())
    }

    pub fn genesis(a: u64, pk: &[u8]) -> Self {
        let mut s = UtxoSet::new();
        let tx = Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                amount: a,
                public_key: pk.try_into().unwrap(),
                spendable_after: 0,
                stealth_dest: None,
                commitment_bytes: None,
                range_proof_bytes: None,
                ephemeral: None,
            }],
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
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
        v.iter()
            .map(|&a| TxOutput {
                amount: a,
                public_key: pk.try_into().unwrap(),
                spendable_after: 0,
                stealth_dest: None,
                commitment_bytes: None,
                range_proof_bytes: None,
                ephemeral: None,
            })
            .collect()
    }

    fn mk_tx(inp: Vec<TxInput>, out: Vec<TxOutput>, sk: &SigningKey) -> Transaction {
        let mut tx = Transaction {
            version: 1,
            inputs: inp,
            outputs: out,
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };
        let sig = sk.sign(&tx_msg(&tx));
        tx.signatures = vec![sig.to_bytes().to_vec()];
        tx
    }

    #[test]
    fn test_spend() {
        let mut s = UtxoSet::new();
        let sk = make_signing_key();
        let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction {
            version: 1,
            inputs: vec![],
            outputs: out(&[5000], &pk),
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        assert!(s
            .spend_transaction_inputs(
                &mk_tx(
                    vec![TxInput {
                        previous_tx_hash: h,
                        output_index: 0,
                        key_image: [0xab; 32]
                    }],
                    out(&[3000], &pk),
                    &sk
                ),
                1000
            )
            .is_ok());
    }

    #[test]
    fn test_wrong_sig() {
        let mut s = UtxoSet::new();
        let sk = make_signing_key();
        let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction {
            version: 1,
            inputs: vec![],
            outputs: out(&[5000], &pk),
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        let wrong = make_signing_key();
        assert!(s
            .spend_transaction_inputs(
                &mk_tx(
                    vec![TxInput {
                        previous_tx_hash: h,
                        output_index: 0,
                        key_image: [0xcd; 32]
                    }],
                    out(&[3000], &pk),
                    &wrong
                ),
                1000
            )
            .is_err());
    }

    #[test]
    fn test_double_spend() {
        let mut s = UtxoSet::new();
        let sk = make_signing_key();
        let pk = sk.verifying_key().to_bytes().to_vec();
        let tx = Transaction {
            version: 1,
            inputs: vec![],
            outputs: out(&[5000], &pk),
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };
        let h = tx.hash();
        s.add_transaction_outputs(&h, &tx, 0, 0);
        assert!(s
            .spend_transaction_inputs(
                &mk_tx(
                    vec![TxInput {
                        previous_tx_hash: h,
                        output_index: 0,
                        key_image: [0xab; 32]
                    }],
                    out(&[3000], &pk),
                    &sk
                ),
                1000
            )
            .is_ok());
        assert!(s
            .spend_transaction_inputs(
                &mk_tx(
                    vec![TxInput {
                        previous_tx_hash: h,
                        output_index: 0,
                        key_image: [0xcd; 32]
                    }],
                    out(&[3000], &pk),
                    &sk
                ),
                1000
            )
            .is_err());
    }

    #[test]
    fn test_supply() {
        let s = UtxoSet::genesis(100_000_000, &[0; 32]);
        assert_eq!(s.total_supply(), 100_000_000);
    }
}
