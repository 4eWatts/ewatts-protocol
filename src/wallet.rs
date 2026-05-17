//! Ewatts Wallet — stealth key management, UTXO scanning, private tx construction.
//!
//! ## Usage
//! ```bash
//! ewatts wallet new          # Generate stealth keypair
//! ewatts wallet balance      # Scan blockchain for owned UTXOs
//! ewatts wallet send <addr> <amount>  # Create and broadcast private tx
//! ewatts wallet list         # List all wallet keys
//! ```


use serde::{Serialize, Deserialize};
use rand::rngs::ThreadRng;
use rand::RngCore;
use std::fs;
use std::path::Path;
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;

use crate::block::*;
use crate::privacy::*;
use crate::state::{UtxoKey, UtxoEntry, UtxoSet};

const WALLET_DIR: &str = "ewatts_data/wallets";

/// A single stealth keypair in the wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthKeyEntry {
    pub view_secret: [u8; 32],   // Scalar bytes
    pub spend_secret: [u8; 32],  // Scalar bytes
    pub spend_key: [u8; 32],     // Compressed RistrettoPoint
    pub view_key: [u8; 32],      // Compressed RistrettoPoint
    pub label: String,
}

impl StealthKeyEntry {
    pub fn address(&self) -> String {
        hex::encode(self.spend_key)
    }

    /// Derive the StealthAddress from stored bytes.
    pub fn stealth_address(&self) -> StealthAddress {
        let s = curve25519_dalek::ristretto::CompressedRistretto(self.spend_key)
            .decompress().unwrap_or_else(RistrettoPoint::identity);
        let v = curve25519_dalek::ristretto::CompressedRistretto(self.view_key)
            .decompress().unwrap_or_else(RistrettoPoint::identity);
        StealthAddress { spend_key: s, view_key: v }
    }

    /// Get the private scalars.
    pub fn secrets(&self) -> (Scalar, Scalar) {
        let view = Scalar::from_bytes_mod_order(self.view_secret);
        let spend = Scalar::from_bytes_mod_order(self.spend_secret);
        (view, spend)
    }
}

/// A UTXO owned by this wallet.
#[derive(Debug, Clone)]
pub struct OwnedUtxo {
    pub key: UtxoKey,
    pub entry: UtxoEntry,
    pub one_time_key: Scalar,     // derived private key for spending
    pub commitment_val: u64,      // amount
}

/// Wallet state: loaded keys.
pub struct Wallet {
    pub keys: Vec<StealthKeyEntry>,
}

impl Wallet {
    /// Load or initialize the wallet from disk.
    pub fn load() -> Self {
        let path = format!("{}/keys.json", WALLET_DIR);
        let keys = if Path::new(&path).exists() {
            let data = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        Wallet { keys }
    }

    /// Save keys to disk.
    pub fn save(&self) {
        let path = format!("{}/keys.json", WALLET_DIR);
        fs::create_dir_all(WALLET_DIR).ok();
        let data = serde_json::to_string_pretty(&self.keys).unwrap();
        fs::write(&path, &data).ok();
        println!("  Wallet saved: {}", path);
    }

    /// Generate a new stealth keypair and add to wallet.
    pub fn new_key(&mut self, label: &str) {
        let mut rng = rand::thread_rng();
        let (addr, key) = StealthAddress::generate(&mut rng);
        let entry = StealthKeyEntry {
            view_secret: key.view.to_bytes(),
            spend_secret: key.spend.to_bytes(),
            spend_key: addr.spend_key.compress().to_bytes(),
            view_key: addr.view_key.compress().to_bytes(),
            label: label.to_string(),
        };
        let addr_hex = entry.address();
        self.keys.push(entry);
        self.save();
        println!("  Generated stealth key: {}", &addr_hex[..16]);
        println!("  Label: {}", label);
    }

    /// Scan the UTXO set for outputs owned by this wallet.
    pub fn scan_utxos(&self, utxo_set: &UtxoSet) -> Vec<OwnedUtxo> {
        let mut owned = Vec::new();
        let map = utxo_set.utxos_map();
        for (key, entry) in map.iter() {
            if let Some(sd) = &entry.stealth_dest {
                let ephem_pt = curve25519_dalek::ristretto::CompressedRistretto(*sd)
                    .decompress();
                if let Some(_ephem) = ephem_pt {
                    // Try to recover with each wallet key
                    for k in &self.keys {
                        let (view, spend) = k.secrets();
                        // We need the ephemeral key (R) to recover
                        // Actually for scanning, we need to check if we can derive a key
                        // For now, try all known keys and see if the pubkey matches
                        let addr = k.stealth_address();
                        // Derive the expected dest: P = Hs(k_v * R) * G + K_s
                        // But we don't have R here (it's not stored in the UTXO entry)
                        // R needs to be stored somewhere for recovery
                        // For now, we use a simplified check
                        let _ = (view, spend, addr);
                    }
                }
            }
            // Also check legacy (ed25519) public keys
            for k in &self.keys {
                if entry.public_key.len() == 32 && k.spend_key == entry.public_key[..32] {
                    owned.push(OwnedUtxo {
                        key: key.clone(),
                        entry: entry.clone(),
                        one_time_key: Scalar::from(0u64), // placeholder
                        commitment_val: entry.amount,
                    });
                }
            }
        }
        owned
    }

    /// List all wallet keys.
    pub fn list(&self) {
        if self.keys.is_empty() {
            println!("  No keys in wallet. Run 'wallet new' first.");
            return;
        }
        for (i, k) in self.keys.iter().enumerate() {
            println!("  [{:02}] {}  ({})", i, k.address(), k.label);
        }
    }
}

/// Create a private transaction using the first wallet key.
/// This is a simplified version — real wallet needs proper ring selection and R storage.
pub fn create_private_tx(
    wallet: &Wallet,
    to_stealth_bytes: &[u8; 32],
    amount: u64,
    utxo_set: &UtxoSet,
    rng: &mut ThreadRng,
) -> Result<Transaction, String> {
    if wallet.keys.is_empty() {
        return Err("No wallet keys".into());
    }
    let key = &wallet.keys[0];
    let addr = key.stealth_address();
    let (view_sec, spend_sec) = key.secrets();

    // Scan for own UTXOs
    let owned = wallet.scan_utxos(utxo_set);
    let mut selected_inputs = Vec::new();
    let mut total = 0u64;

    // Select UTXOs to spend
    let mut total_available = 0u64;
    for o in &owned {
        total_available += o.entry.amount;
    }
    if total_available < amount {
        return Err(format!("Insufficient balance: have {}, need {}", total_available, amount));
    }

    // Simple selection: take first UTXOs until amount is covered
    let mut selected_utxos = Vec::new();
    for o in &owned {
        if total >= amount { break; }
        total += o.entry.amount;
        selected_utxos.push(o.clone());
    }

    // Get all UTXOs for ring member selection
    let all_utxos: Vec<(UtxoKey, UtxoEntry)> = utxo_set.utxos_map()
        .iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    let mut ring_members = Vec::new();

    for utxo in &selected_utxos {
        let input = TxInput {
            previous_tx_hash: utxo.key.tx_hash,
            output_index: utxo.key.output_index,
            key_image: [0u8; 32], // placeholder, set after MLSAG signs
        };
        selected_inputs.push(input);

        // Select ring members: pick 10 random UTXOs + our own
        // For simplicity, use hash-to-point of random UTXOs
        let mut members: Vec<UtxoRef> = Vec::new();
        members.push(UtxoRef {
            tx_hash: utxo.key.tx_hash,
            output_index: utxo.key.output_index,
        });

        // Add 10 random ring members
        let ring_size = 11usize;
        let mut indices: Vec<usize> = (0..all_utxos.len()).collect();
        // Shuffle and pick
        for i in (1..indices.len()).rev() {
            let j = rng.next_u32() as usize % (i + 1);
            indices.swap(i, j);
        }
        for &idx in indices.iter().take(ring_size - 1).filter(|&&i| all_utxos[i].0 != utxo.key) {
            members.push(UtxoRef {
                tx_hash: all_utxos[idx].0.tx_hash,
                output_index: all_utxos[idx].0.output_index,
            });
        }
        ring_members.push(members);
    }

    // Destination: create stealth output for recipient
    let to_addr = StealthAddress {
        spend_key: curve25519_dalek::ristretto::CompressedRistretto(*to_stealth_bytes)
            .decompress().unwrap_or_else(RistrettoPoint::identity),
        view_key: RistrettoPoint::identity(), // not used for one-time dest
    };
    let (dest, _r_ephem) = to_addr.derive_destination(rng);

    // Create outputs
    let mut outputs = Vec::new();

    // Output to recipient
    let (comm_to, blinding_to) = Commitment::new(amount, rng);
    let range_to = RangeProof::prove(amount, blinding_to, 32, rng);
    outputs.push(TxOutput::new_private(
        amount,
        dest.dest.compress().to_bytes(),
        comm_to.0.compress().to_bytes(),
        serde_json::to_vec(&range_to).unwrap_or_default(),
    ));

    // Change output to self (if any)
    if total > amount {
        let change = total - amount;
        let change_dest = StealthAddress {
            spend_key: addr.spend_key,
            view_key: addr.view_key,
        };
        let (c_dest, _) = change_dest.derive_destination(rng);
        let (comm_ch, blinding_ch) = Commitment::new(change, rng);
        let range_ch = RangeProof::prove(change, blinding_ch, 32, rng);
        outputs.push(TxOutput::new_private(
            change,
            c_dest.dest.compress().to_bytes(),
            comm_ch.0.compress().to_bytes(),
            serde_json::to_vec(&range_ch).unwrap_or_default(),
        ));
    }

    // Build the transaction (without MLSAG sig first)
    let mut tx = Transaction {
        version: 1,
        inputs: selected_inputs,
        outputs,
        ring_size: 11,
        signatures: vec![],
        mlsag: None,
        ring_members: Some(ring_members.clone()),
    };

    // Build ring pubkeys for MLSAG signing
    let mut ring_pubkeys: Vec<Vec<RistrettoPoint>> = Vec::with_capacity(ring_members.len());
    for members in &ring_members {
        let mut layer_ring: Vec<RistrettoPoint> = Vec::with_capacity(members.len());
        for m in members {
            // Look up each ring member's pubkey from UTXO set
            let mk = UtxoKey { tx_hash: m.tx_hash, output_index: m.output_index };
            if let Some(entry) = utxo_set.utxos_map().get(&mk) {
                let pk = if let Some(sd) = &entry.stealth_dest {
                    curve25519_dalek::ristretto::CompressedRistretto(*sd)
                        .decompress().unwrap_or_else(RistrettoPoint::identity)
                } else {
                    hash_to_point(&entry.public_key)
                };
                layer_ring.push(pk);
            } else {
                layer_ring.push(RistrettoPoint::identity());
            }
        }
        ring_pubkeys.push(layer_ring);
    }

    // Transpose: MLSAG needs [ring_pos][layer] not [layer][ring_pos]
    if ring_pubkeys.is_empty() {
        return Err("No ring pubkeys".into());
    }
    let n_layers = ring_pubkeys.len();
    let ring_sz = ring_pubkeys[0].len();
    let mut mlsag_ring = vec![Vec::with_capacity(n_layers); ring_sz];
    for pos in 0..ring_sz {
        for layer in 0..n_layers {
            mlsag_ring[pos].push(ring_pubkeys[layer][pos]);
        }
    }

    // Sign with MLSAG (real signer at position 0 for each layer)
    let secret_keys: Vec<Scalar> = selected_utxos.iter().map(|u| u.one_time_key).collect();
    let msg = crate::state::tx_msg(&tx);
    let sig = MLSAGSignature::sign(&mlsag_ring, &secret_keys, 0, &msg, rng);

    // Set key images from MLSAG
    for (i, ki) in sig.key_images.iter().enumerate() {
        if i < tx.inputs.len() {
            tx.inputs[i].key_image = ki.compress().to_bytes();
        }
    }

    // Attach MLSAG data
    tx.mlsag = Some(MlsagData::from_sig(&sig));

    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_wallet_keygen() {
        let mut w = Wallet { keys: vec![] };
        w.new_key("test");
        assert_eq!(w.keys.len(), 1);
        assert!(w.keys[0].address().len() == 64);
    }

    #[test]
    fn test_wallet_scan_empty() {
        let w = Wallet { keys: vec![] };
        let utxo_set = UtxoSet::new();
        let owned = w.scan_utxos(&utxo_set);
        assert!(owned.is_empty());
    }
}
