//! Ewatts Wallet — stealth key management, UTXO scanning, private tx construction.
//!
//! ## SECURITY NOTE
//! This wallet is a REFERENCE implementation for testnet. It is NOT hardened against:
//! - Side-channel attacks (key material processed in software without isolation)
//! - Persistent state monitoring (keys stored unencrypted on disk)
//! - Malicious RNG (uses ThreadRng, which is not cryptographically audited for production)
//! - Sophisticated chain analysis (ring selection is simple, not optimized for maximum entropy)
//! Production wallet requires HSM integration, encrypted key storage, and constant-time operations.
//!
//! ## Usage
//! ```bash
//! ewatts wallet new          # Generate stealth keypair
//! ewatts wallet balance      # Scan blockchain for owned UTXOs
//! ewatts wallet send <addr> <amount>  # Create and broadcast private tx
//! ewatts wallet list         # List all wallet keys
//! ```

use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::ThreadRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::block::*;
use crate::privacy::*;
use crate::state::{UtxoEntry, UtxoKey, UtxoSet};

const WALLET_DIR: &str = "ewatts_data/wallets";

/// A single stealth keypair in the wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthKeyEntry {
    pub view_secret: [u8; 32],      // Scalar bytes
    pub spend_secret: [u8; 32],     // Scalar bytes
    pub spend_key: [u8; 32],        // Compressed RistrettoPoint
    pub view_key: [u8; 32],         // Compressed RistrettoPoint
    pub legacy_public_key: Vec<u8>, // ed25519 public key (P1-3: legacy UTXO detection)
    pub label: String,
}

impl StealthKeyEntry {
    pub fn address(&self) -> String {
        hex::encode(self.spend_key)
    }

    /// Derive the StealthAddress from stored bytes.
    pub fn stealth_address(&self) -> Result<StealthAddress, String> {
        let s = curve25519_dalek::ristretto::CompressedRistretto(self.spend_key)
            .decompress()
            .ok_or_else(|| "Invalid spend key in wallet".to_string())?;
        let v = curve25519_dalek::ristretto::CompressedRistretto(self.view_key)
            .decompress()
            .ok_or_else(|| "Invalid view key in wallet".to_string())?;
        Ok(StealthAddress {
            spend_key: s,
            view_key: v,
        })
    }

    /// Get the private scalars.
    pub fn secrets(&self) -> (Scalar, Scalar) {
        let view = Scalar::from_bytes_mod_order(self.view_secret);
        let spend = Scalar::from_bytes_mod_order(self.spend_secret);
        (view, spend)
    }

    /// Get ed25519 verifying key for legacy UTXO detection (P1-3 fix).
    pub fn legacy_verifying_key(&self) -> Option<VerifyingKey> {
        if self.legacy_public_key.len() == 32 {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&self.legacy_public_key[..32]);
            VerifyingKey::from_bytes(&bytes).ok()
        } else {
            None
        }
    }
}

/// A UTXO owned by this wallet.
#[derive(Debug, Clone)]
pub struct OwnedUtxo {
    pub key: UtxoKey,
    pub entry: UtxoEntry,
    pub one_time_key: Scalar, // derived private key for spending
    pub commitment_val: u64,  // amount
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
        // Also generate ed25519 key for legacy coinbase UTXOs (P1-3 fix)
        let ed_secret = SigningKey::generate(&mut rng);
        let ed_public = ed_secret.verifying_key().to_bytes().to_vec();
        let entry = StealthKeyEntry {
            view_secret: key.view.to_bytes(),
            spend_secret: key.spend.to_bytes(),
            spend_key: addr.spend_key.compress().to_bytes(),
            view_key: addr.view_key.compress().to_bytes(),
            legacy_public_key: ed_public,
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
                if let Some(eph) = &entry.ephemeral {
                    let ephem_point =
                        match curve25519_dalek::ristretto::CompressedRistretto(*eph).decompress() {
                            Some(p) => p,
                            None => continue, // malformed ephemeral, skip
                        };
                    for k in &self.keys {
                        let (view, spend) = k.secrets();
                        let derived =
                            crate::privacy::recover_one_time_key(&view, &spend, &ephem_point);
                        let expected_dest = derived * ring_g();
                        let actual_dest = match curve25519_dalek::ristretto::CompressedRistretto(
                            *sd,
                        )
                        .decompress()
                        {
                            Some(p) => p,
                            None => continue,
                        };
                        if expected_dest == actual_dest {
                            owned.push(OwnedUtxo {
                                key: key.clone(),
                                entry: entry.clone(),
                                one_time_key: derived,
                                commitment_val: entry.amount,
                            });
                            break;
                        }
                    }
                }
            }
            // Also check legacy (ed25519) public keys (P1-3 fixed: use matching key type)
            for k in &self.keys {
                if let Some(vk) = k.legacy_verifying_key() {
                    if entry.public_key.len() == 32 && vk.to_bytes() == entry.public_key[..32] {
                        owned.push(OwnedUtxo {
                            key: key.clone(),
                            entry: entry.clone(),
                            one_time_key: Scalar::from(0u64), // placeholder, legacy mode
                            commitment_val: entry.amount,
                        });
                        break;
                    }
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
    if amount == 0 {
        return Err("Cannot send zero amount".into()); // P2-1
    }
    let key = &wallet.keys[0];
    let addr = key
        .stealth_address()
        .map_err(|e| format!("Invalid wallet address: {}", e))?;
    let (_view_sec, _spend_sec) = key.secrets();

    // Scan for own UTXOs
    let owned = wallet.scan_utxos(utxo_set);

    // Select UTXOs to spend
    let mut total_available = 0u64;
    for o in &owned {
        total_available += o.entry.amount;
    }
    if total_available < amount {
        return Err(format!(
            "Insufficient balance: have {}, need {}",
            total_available, amount
        ));
    }

    let mut total = 0u64;
    let mut selected_utxos = Vec::new();
    for o in &owned {
        if total >= amount {
            break;
        }
        total += o.entry.amount;
        selected_utxos.push(o.clone());
    }

    // Get all UTXOs for ring member selection (P1-2: filter to stealth-only)
    let all_utxos: Vec<(UtxoKey, UtxoEntry)> = utxo_set
        .utxos_map()
        .iter()
        .filter(|(_, v)| v.stealth_dest.is_some())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let ring_size = 11usize;
    let mut ring_members = Vec::with_capacity(selected_utxos.len());
    let mut ring_pubkeys: Vec<Vec<RistrettoPoint>> = Vec::with_capacity(selected_utxos.len());
    let mut selected_inputs = Vec::with_capacity(selected_utxos.len());
    let mut secret_keys = Vec::with_capacity(selected_utxos.len());

    // Decide real_index ONCE: all inputs share the same position in their rings
    let real_index = rng.next_u32() as usize % ring_size;

    // First pass: build rings, compute key_images BEFORE signing (P0-2 fix)
    for utxo in &selected_utxos {
        // Select ring members: filter FIRST, then take (P0-4 fix)
        let mut members: Vec<UtxoRef> = Vec::with_capacity(ring_size);
        let mut indices: Vec<usize> = (0..all_utxos.len()).collect();
        for i in (1..indices.len()).rev() {
            let j = rng.next_u32() as usize % (i + 1);
            indices.swap(i, j);
        }
        // Pick decoys (filter out own UTXO)
        for &idx in indices
            .iter()
            .filter(|&&i| all_utxos[i].0 != utxo.key)
            .take(ring_size - 1)
        {
            members.push(UtxoRef {
                tx_hash: all_utxos[idx].0.tx_hash,
                output_index: all_utxos[idx].0.output_index,
            });
        }
        // Insert own UTXO at the FIXED real_index (shared across all layers)
        members.insert(
            real_index,
            UtxoRef {
                tx_hash: utxo.key.tx_hash,
                output_index: utxo.key.output_index,
            },
        );

        // Build ring pubkeys for this layer
        let mut layer_ring: Vec<RistrettoPoint> = Vec::with_capacity(members.len());
        for m in &members {
            if let Some(entry) = utxo_set.utxos_map().get(&UtxoKey {
                tx_hash: m.tx_hash,
                output_index: m.output_index,
            }) {
                let pk = entry
                    .stealth_dest_point()
                    .ok_or_else(|| "Ring member missing stealth dest".to_string())?;
                layer_ring.push(pk);
            } else {
                return Err(format!("Ring member UTXO not found: {:?}", m));
            }
        }
        ring_pubkeys.push(layer_ring);

        // Compute key_image deterministically (P0-2 fix: was placeholder, set after sign)
        let key_pubkey = ring_pubkeys.last().unwrap()[real_index];
        let key_image = utxo.one_time_key * hash_pk(&key_pubkey);

        selected_inputs.push(TxInput {
            previous_tx_hash: utxo.key.tx_hash,
            output_index: utxo.key.output_index,
            key_image: key_image.compress().to_bytes(),
        });
        secret_keys.push(utxo.one_time_key);
        ring_members.push(members);
    }

    // Destination: create stealth output for recipient
    let to_addr = StealthAddress {
        spend_key: curve25519_dalek::ristretto::CompressedRistretto(*to_stealth_bytes)
            .decompress()
            .ok_or_else(|| "Invalid recipient stealth address".to_string())?,
        view_key: RistrettoPoint::identity(),
    };
    let (dest, _r_ephem) = to_addr.derive_destination(rng);

    // Output to recipient — use prove_with_blinding (P0-1 fix)
    let mut outputs = Vec::new();
    let (range_to, total_blinding_to) = RangeProof::prove_with_blinding(amount, 32, rng);
    let comm_to = Commitment::new_with_blinding(amount, total_blinding_to);
    outputs.push(TxOutput::new_private(
        amount,
        dest.dest.compress().to_bytes(),
        comm_to.0.compress().to_bytes(),
        serde_json::to_vec(&range_to).unwrap_or_default(),
    ));
    if let Some(o) = outputs.last_mut() {
        o.ephemeral = Some(dest.ephemeral.compress().to_bytes());
    }

    // Change output to self
    if total > amount {
        let change = total - amount;
        let change_dest = StealthAddress {
            spend_key: addr.spend_key,
            view_key: addr.view_key,
        };
        let (c_dest, _) = change_dest.derive_destination(rng);
        let (range_ch, tot_blinding_ch) = RangeProof::prove_with_blinding(change, 32, rng);
        let comm_ch = Commitment::new_with_blinding(change, tot_blinding_ch);
        outputs.push(TxOutput::new_private(
            change,
            c_dest.dest.compress().to_bytes(),
            comm_ch.0.compress().to_bytes(),
            serde_json::to_vec(&range_ch).unwrap_or_default(),
        ));
        if let Some(o) = outputs.last_mut() {
            o.ephemeral = Some(c_dest.ephemeral.compress().to_bytes());
        }
    }

    // Transpose ring_pubkeys: MLSAG needs [ring_pos][layer]
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

    // Build transaction with finalized inputs (key_images already set, P0-2)
    let tx = Transaction {
        version: 1,
        inputs: selected_inputs,
        outputs,
        ring_size: ring_size as u16,
        signatures: vec![],
        mlsag: None,
        ring_members: Some(ring_members),
    };

    // Sign over finalized tx_msg (includes key_images in hash, P0-2)
    // Using the same real_index that was used for ring construction
    let msg = crate::state::tx_msg(&tx);
    let sig = MLSAGSignature::sign(&mlsag_ring, &secret_keys, real_index, &msg, rng);

    let mut tx = tx;
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
        // legacy key should be generated (P1-3)
        assert_eq!(w.keys[0].legacy_public_key.len(), 32);
    }

    #[test]
    fn test_wallet_scan_empty() {
        let w = Wallet { keys: vec![] };
        let utxo_set = UtxoSet::new();
        let owned = w.scan_utxos(&utxo_set);
        assert!(owned.is_empty());
    }

    #[test]
    fn test_create_tx_zero_amount_rejected() {
        let mut w = Wallet { keys: vec![] };
        w.new_key("test");
        let dummy_addr = [0u8; 32];
        let utxo_set = UtxoSet::new();
        let result = create_private_tx(&w, &dummy_addr, 0, &utxo_set, &mut rand::thread_rng());
        assert!(result.is_err(), "zero amount tx should be rejected");
    }

    /// Create stealth UTXOs in the state for testing ring membership.
    fn seed_stealth_utxo(utxo_set: &mut UtxoSet, pk: [u8; 32], sd: [u8; 32], amount: u64) {
        use crate::privacy::pedersen_h;
        use curve25519_dalek::scalar::Scalar;
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        let blinding = Scalar::random(&mut rng);
        let comm = crate::privacy::Commitment::new_with_blinding(amount, blinding);
        let mut tx_hash = [0u8; 32];
        rng.fill_bytes(&mut tx_hash);
        utxo_set.add_transaction_outputs(&tx_hash, &Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                amount,
                public_key: pk.to_vec(),
                spendable_after: 0,
                stealth_dest: Some(sd),
                commitment_bytes: Some(comm.0.compress().to_bytes()),
                range_proof_bytes: Some(vec![]),
                ephemeral: None,
            }],
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        }, 0, 0);
    }

    #[test]
    fn test_create_private_tx_roundtrip() {
        use crate::state::UtxoSet;
        use rand::RngCore;

        let mut rng = rand::thread_rng();

        // Create Alice and Bob wallets
        let mut alice_w = Wallet { keys: vec![] };
        alice_w.new_key("alice");
        let mut bob_w = Wallet { keys: vec![] };
        bob_w.new_key("bob");

        let alice_addr = alice_w.keys[0].stealth_address().unwrap();
        let bob_spend = bob_w.keys[0].spend_key;

        // Create UTXO set and seed 11 stealth UTXOs for ring members
        let mut utxo_set = UtxoSet::new();
        let mut dummy_pk = [0u8; 32];
        rng.fill_bytes(&mut dummy_pk);

        // Alice's UTXO (the one she'll spend)
        let alice_sd = alice_addr.spend_key.compress().to_bytes();
        seed_stealth_utxo(&mut utxo_set, dummy_pk, alice_sd, 500);

        // 10 decoy UTXOs for ring membership
        for _ in 0..10 {
            let mut sd = [0u8; 32];
            rng.fill_bytes(&mut sd);
            seed_stealth_utxo(&mut utxo_set, dummy_pk, sd, 100);
        }

        // Alice creates a private tx to Bob
        let tx = create_private_tx(&alice_w, &bob_spend, 100, &utxo_set, &mut rng)
            .expect("Private tx creation should succeed");

        // Verify state accepts the tx (MLSAG + Pedersen + range proof)
        utxo_set.spend_transaction_inputs(&tx, 1)
            .expect("State should accept the private tx");

        // Verify: Alice's balance decreased
        let alice_owned = alice_w.scan_utxos(&utxo_set);
        let alice_balance: u64 = alice_owned.iter().map(|o| o.entry.amount).sum();
        // Alice spent 500, got ~400 back as change (100 sent, rest change)
        assert!(alice_balance > 0, "Alice should have change");

        // Verify: Bob can see the new UTXO
        let bob_owned = bob_w.scan_utxos(&utxo_set);
        let bob_balance: u64 = bob_owned.iter().map(|o| o.entry.amount).sum();
        assert_eq!(bob_balance, 100, "Bob should receive 100");
    }
}
