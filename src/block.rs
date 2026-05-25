use crate::commitment::Commitment;
use crate::constants;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: u32,
    pub previous_hash: [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp: u64,
    pub height: u64,
    pub epoch: u64,
    pub difficulty_target: u64,
    pub total_effective_commit: u64,
    pub emission_rate: u64,
    pub miner_effective_commit: u64,
    pub vr_block: u64,
    pub coinbase_burn: u64,
    pub nonce: u64,
    pub elapsed_ms: u32,
    pub proof_merkle_root: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockBody {
    pub transactions: Vec<Transaction>,
    pub commitments: Vec<Commitment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub body: BlockBody,
    /// Hash of the header fields used for the PoW proof (excludes nonce/proof fields).
    /// Set during mining; used by verifiers to validate against the same header hash
    /// that the miner solved, even after post-mine fields are filled.
    pub proof_hash: [u8; 32],
}

/// UTXO reference: (tx_hash, output_index)
#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct UtxoRef {
    pub tx_hash: [u8; 32],
    pub output_index: u32,
}

/// Serialized MLSAG (compressed points for serde)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlsagData {
    pub ring_size: usize,
    pub n_layers: usize,
    pub key_images: Vec<[u8; 32]>,     // compressed RistrettoPoints
    pub c0: [u8; 32],                  // Scalar bytes (32 bytes)
    pub responses: Vec<Vec<[u8; 32]>>, // [ring_size][n_layers] scalars
}

impl MlsagData {
    pub fn from_sig(sig: &crate::privacy::MLSAGSignature) -> Self {
        let compress = |pt: &curve25519_dalek::ristretto::RistrettoPoint| pt.compress().to_bytes();
        MlsagData {
            ring_size: sig.ring_size,
            n_layers: sig.n_layers,
            key_images: sig.key_images.iter().map(compress).collect(),
            c0: sig.c0.to_bytes(),
            responses: sig
                .responses
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|s| {
                            let mut b = [0u8; 32];
                            b.copy_from_slice(&s.to_bytes());
                            b
                        })
                        .collect()
                })
                .collect(),
        }
    }

    /// Deserialize to in-memory MLSAGSignature (ring excluded)
    pub fn to_sig(&self) -> Result<crate::privacy::MLSAGSignature, String> {
        use curve25519_dalek::ristretto::CompressedRistretto;
        use curve25519_dalek::scalar::Scalar;
        let decompress =
            |b: &[u8; 32]| -> Result<curve25519_dalek::ristretto::RistrettoPoint, String> {
                CompressedRistretto(*b)
                    .decompress()
                    .ok_or_else(|| "Invalid compressed point in MlsagData".to_string())
            };
        let scalar32 = |b: &[u8; 32]| -> Scalar { Scalar::from_bytes_mod_order(*b) };
        let mut key_images = Vec::with_capacity(self.key_images.len());
        for ki in &self.key_images {
            key_images.push(decompress(ki)?);
        }
        Ok(crate::privacy::MLSAGSignature {
            ring_size: self.ring_size,
            n_layers: self.n_layers,
            key_images,
            c0: Scalar::from_bytes_mod_order(self.c0),
            responses: self
                .responses
                .iter()
                .map(|row| row.iter().map(scalar32).collect())
                .collect(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub version: u16,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub ring_size: u16,
    pub signatures: Vec<Vec<u8>>,
    pub mlsag: Option<MlsagData>,
    pub ring_members: Option<Vec<Vec<UtxoRef>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub previous_tx_hash: [u8; 32],
    pub output_index: u32,
    pub key_image: [u8; 32],
    pub revealed_pubkey: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub amount: u64,
    pub pubkey_hash: [u8; 20],
    pub spendable_after: u64,
    pub stealth_dest: Option<[u8; 32]>,
    pub commitment_bytes: Option<[u8; 32]>,
    pub range_proof_bytes: Option<Vec<u8>>,
    pub ephemeral: Option<[u8; 32]>,
}

impl TxOutput {
    /// Hash a public key into 20 bytes (SHA256, truncated).
    pub fn hash_pubkey(pk: &[u8]) -> [u8; 20] {
        use sha3::Digest;
        let mut h = sha3::Keccak256::new();
        h.update(pk);
        let full = h.finalize();
        let mut out = [0u8; 20];
        out.copy_from_slice(&full[..20]);
        out
    }

    /// Create a public P2PKH output (coinbase).
    pub fn new(amount: u64, pubkey: Vec<u8>) -> Self {
        let ph = Self::hash_pubkey(&pubkey);
        TxOutput {
            amount,
            pubkey_hash: ph,
            spendable_after: 0,
            stealth_dest: None,
            commitment_bytes: None,
            range_proof_bytes: None,
            ephemeral: None,
        }
    }

    pub fn new_private(
        amount: u64,
        dest: [u8; 32],
        commitment: [u8; 32],
        range_proof: Vec<u8>,
    ) -> Self {
        TxOutput {
            amount,
            pubkey_hash: [0u8; 20],
            spendable_after: 0,
            stealth_dest: Some(dest),
            commitment_bytes: Some(commitment),
            range_proof_bytes: Some(range_proof),
            ephemeral: None,
        }
    }

    pub fn new_locked(amount: u64, pubkey: Vec<u8>, block_number: u64) -> Self {
        let ph = Self::hash_pubkey(&pubkey);
        let lock = if block_number < constants::RAMP_UP_BLOCKS {
            std::cmp::max(
                constants::FOUNDER_LOCK_BLOCKS,
                block_number + constants::FOUNDER_LOCK_ADDITIONAL,
            )
        } else {
            0
        };
        TxOutput {
            amount,
            pubkey_hash: ph,
            spendable_after: lock,
            stealth_dest: None,
            commitment_bytes: None,
            range_proof_bytes: None,
            ephemeral: None,
        }
    }

    pub fn is_spendable(&self, current_block: u64) -> bool {
        current_block >= self.spendable_after
    }

    /// Returns true if this output uses private (stealth) mode.
    pub fn is_private(&self) -> bool {
        self.stealth_dest.is_some()
    }
}

impl BlockHeader {
    pub fn hash(&self) -> [u8; 32] {
        let mut h = Keccak256::new();
        h.update(self.version.to_le_bytes());
        h.update(self.previous_hash);
        h.update(self.merkle_root);
        h.update(self.timestamp.to_le_bytes());
        h.update(self.height.to_le_bytes());
        h.update(self.epoch.to_le_bytes());
        h.update(self.difficulty_target.to_le_bytes());
        h.update(self.total_effective_commit.to_le_bytes());
        h.update(self.emission_rate.to_le_bytes());
        h.update(self.miner_effective_commit.to_le_bytes());
        h.update(self.vr_block.to_le_bytes());
        h.update(self.coinbase_burn.to_le_bytes());
        h.update(self.nonce.to_le_bytes());
        h.update(self.elapsed_ms.to_le_bytes());
        if let Some(root) = self.proof_merkle_root {
            h.update(root);
        }
        h.finalize().into()
    }

    /// Hash used for PoW proof: excludes nonce and proof-related fields.
    /// Returns the SAME value before and after mining, ensuring verifiers
    /// validate against the hash that the miner actually solved.
    pub fn proof_hash(&self) -> [u8; 32] {
        let mut h = Keccak256::new();
        h.update(self.version.to_le_bytes());
        h.update(self.previous_hash);
        h.update(self.merkle_root);
        h.update(self.timestamp.to_le_bytes());
        h.update(self.height.to_le_bytes());
        h.update(self.epoch.to_le_bytes());
        h.update(self.difficulty_target.to_le_bytes());
        h.update(self.total_effective_commit.to_le_bytes());
        h.update(self.emission_rate.to_le_bytes());
        h.update(self.miner_effective_commit.to_le_bytes());
        h.update(self.vr_block.to_le_bytes());
        h.update(self.coinbase_burn.to_le_bytes());
        h.finalize().into()
    }
}

impl Transaction {
    pub fn hash(&self) -> [u8; 32] {
        let mut h = Keccak256::new();
        h.update(self.version.to_le_bytes());
        for i in &self.inputs {
            h.update(i.previous_tx_hash);
            h.update(i.output_index.to_le_bytes());
            h.update(i.key_image);
        }
        for o in &self.outputs {
            h.update(o.amount.to_le_bytes());
            if o.pubkey_hash != [0u8; 20] {
                h.update(&o.pubkey_hash);
            }
            if let Some(d) = &o.stealth_dest {
                h.update(d);
            }
            if let Some(c) = &o.commitment_bytes {
                h.update(c);
            }
        }
        h.update(self.ring_size.to_le_bytes());
        h.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_header_hash() {
        let h = BlockHeader {
            version: constants::PROTOCOL_VERSION,
            previous_hash: [0; 32],
            merkle_root: [0; 32],
            timestamp: 1000,
            height: 0,
            epoch: 0,
            difficulty_target: 1,
            total_effective_commit: 100_000_000_000,
            emission_rate: 100_000_000,
            miner_effective_commit: 50_000_000_000,
            vr_block: 1_000,
            coinbase_burn: 0,
            nonce: 42,
            elapsed_ms: 5000,
            proof_merkle_root: None,
        };
        assert_eq!(h.hash(), h.hash());
    }
    #[test]
    fn test_different_nonce() {
        let a = BlockHeader {
            version: constants::PROTOCOL_VERSION,
            previous_hash: [0; 32],
            merkle_root: [0; 32],
            timestamp: 1000,
            height: 0,
            epoch: 0,
            difficulty_target: 1,
            total_effective_commit: 100_000_000_000,
            emission_rate: 100_000_000,
            miner_effective_commit: 50_000_000_000,
            vr_block: 1_000,
            coinbase_burn: 0,
            nonce: 42,
            elapsed_ms: 5000,
            proof_merkle_root: None,
        };
        let mut b = a.clone();
        b.nonce = 43;
        assert_ne!(a.hash(), b.hash());
    }
    #[test]
    fn test_tx_hash() {
        let tx = Transaction {
            version: 1,
            inputs: vec![TxInput {
                previous_tx_hash: [0; 32],
                output_index: 0,
                key_image: [0; 32],
            revealed_pubkey: vec![],
            }],
            outputs: vec![TxOutput {
                amount: 1000,
                pubkey_hash: TxOutput::hash_pubkey(&[0u8; 32]),
                spendable_after: 0,
                stealth_dest: None,
                commitment_bytes: None,
                range_proof_bytes: None,
                ephemeral: None,
            }],
            ring_size: 11,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };
        assert_eq!(tx.hash(), tx.hash());
    }
    #[test]
    fn test_private_tx_hash() {
        let tx = Transaction {
            version: 1,
            inputs: vec![TxInput {
                previous_tx_hash: [0; 32],
                output_index: 0,
                key_image: [1u8; 32],
            revealed_pubkey: vec![],
            }],
            outputs: vec![TxOutput {
                amount: 0,
                pubkey_hash: [0u8; 20],
                spendable_after: 0,
                stealth_dest: Some([2u8; 32]),
                commitment_bytes: Some([3u8; 32]),
                range_proof_bytes: Some(vec![4u8; 64]),
                ephemeral: None,
            }],
            ring_size: 11,
            signatures: vec![],
            mlsag: Some(MlsagData {
                ring_size: 11,
                n_layers: 1,
                key_images: vec![[5u8; 32]],
                c0: [6u8; 32],
                responses: vec![vec![[7u8; 32]; 11]],
            }),
            ring_members: Some(vec![(0..11)
                .map(|i| UtxoRef {
                    tx_hash: [i as u8; 32],
                    output_index: i,
                })
                .collect()]),
        };
        assert_eq!(tx.hash(), tx.hash());
        assert_ne!(
            tx.hash(),
            Transaction {
                version: 1,
                inputs: vec![TxInput {
                    previous_tx_hash: [0; 32],
                    output_index: 0,
                    key_image: [1u8; 32], revealed_pubkey: vec![] }],
                outputs: vec![TxOutput {
                    amount: 0,
                    pubkey_hash: [0u8; 20],
                    spendable_after: 0,
                    stealth_dest: Some([9u8; 32]),
                    commitment_bytes: Some([3u8; 32]),
                    range_proof_bytes: Some(vec![4u8; 64]),
                    ephemeral: None
                }],
                ring_size: 11,
                signatures: vec![],
                mlsag: Some(MlsagData {
                    ring_size: 11,
                    n_layers: 1,
                    key_images: vec![[5u8; 32]],
                    c0: [6u8; 32],
                    responses: vec![vec![[7u8; 32]; 11]]
                }),
                ring_members: Some(vec![(0..11)
                    .map(|i| UtxoRef {
                        tx_hash: [i as u8; 32],
                        output_index: i
                    })
                    .collect()]),
            }
            .hash()
        );
    }
}
