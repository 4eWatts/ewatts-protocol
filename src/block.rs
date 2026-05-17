use sha3::{Digest, Keccak256};
use serde::{Serialize, Deserialize};
use crate::constants;
use crate::commitment::Commitment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: u32,
    pub previous_hash: [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp: u64,
    pub height: u64,
    pub epoch: u64,
    pub difficulty_target: u64,
    pub total_effective_commit: f64,
    pub emission_rate: f64,
    pub miner_effective_commit: f64,
    pub vr_block: f64,
    pub nonce: u64,
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockBody {
    pub transactions: Vec<Transaction>,
    pub commitments: Vec<Commitment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block { pub header: BlockHeader, pub body: BlockBody }

/// A reference to a UTXO: (tx_hash, output_index).
/// Used for ring member references.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct UtxoRef {
    pub tx_hash: [u8; 32],
    pub output_index: u32,
}

/// MLSAG ring signature serialized for blockchain storage.
/// All points stored as compressed bytes ([u8; 32]) for serde compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlsagData {
    pub ring_size: usize,
    pub n_layers: usize,
    pub key_images: Vec<[u8; 32]>,           // compressed RistrettoPoints
    pub c0: [u8; 64],                         // Scalar (64 bytes wide → reduced mod order)
    pub responses: Vec<Vec<[u8; 32]>>,       // [ring_size][n_layers] scalars
}

impl MlsagData {
    /// Create from in-memory MLSAGSignature + ring reference.
    pub fn from_sig(sig: &crate::privacy::MLSAGSignature) -> Self {
        let compress = |pt: &curve25519_dalek::ristretto::RistrettoPoint| pt.compress().to_bytes();
        let scalar_bytes = |s: &curve25519_dalek::scalar::Scalar| {
            let mut out = [0u8; 64];
            out[..32].copy_from_slice(&s.to_bytes());
            out
        };
        MlsagData {
            ring_size: sig.ring_size,
            n_layers: sig.n_layers,
            key_images: sig.key_images.iter().map(compress).collect(),
            c0: scalar_bytes(&sig.c0),
            responses: sig.responses.iter().map(|row| {
                row.iter().map(|s| {
                    let mut b = [0u8; 32];
                    b.copy_from_slice(&s.to_bytes());
                    b
                }).collect()
            }).collect(),
        }
    }

    /// Deserialize to in-memory MLSAGSignature (without ring — add during verification).
    pub fn to_sig(&self) -> crate::privacy::MLSAGSignature {
        use curve25519_dalek::ristretto::CompressedRistretto;
        use curve25519_dalek::scalar::Scalar;
        let decompress = |b: &[u8; 32]| -> curve25519_dalek::ristretto::RistrettoPoint {
            CompressedRistretto(*b).decompress().unwrap_or(curve25519_dalek::traits::Identity::identity())
        };
        let scalar_from = |b: &[u8; 64]| -> Scalar {
            Scalar::from_bytes_mod_order_wide(b)
        };
        let scalar32 = |b: &[u8; 32]| -> Scalar {
            Scalar::from_bytes_mod_order(*b)
        };
        crate::privacy::MLSAGSignature {
            ring_size: self.ring_size,
            n_layers: self.n_layers,
            key_images: self.key_images.iter().map(decompress).collect(),
            c0: scalar_from(&self.c0),
            responses: self.responses.iter().map(|row| {
                row.iter().map(scalar32).collect()
            }).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub version: u16,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub ring_size: u16,
    pub signatures: Vec<Vec<u8>>,
    /// MLSAG signature (private mode).
    pub mlsag: Option<MlsagData>,
    /// For each input, the ring of UtxoRefs forming the anonymity set.
    pub ring_members: Option<Vec<Vec<UtxoRef>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub previous_tx_hash: [u8; 32],
    pub output_index: u32,
    pub key_image: [u8; 32],   // 32 bytes = compressed RistrettoPoint for MLSAG
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    /// Legacy: amount in plaintext (public mode / coinbase).
    pub amount: u64,
    /// Legacy: P2PKH public key (public mode).
    pub public_key: Vec<u8>,
    /// Founder time-lock: 0 = immediate.
    pub spendable_after: u64,
    /// Private mode: one-time stealth destination (compressed RistrettoPoint).
    pub stealth_dest: Option<[u8; 32]>,
    /// Private mode: Pedersen commitment (compressed RistrettoPoint).
    pub commitment_bytes: Option<[u8; 32]>,
    /// Private mode: serialized RangeProof.
    pub range_proof_bytes: Option<Vec<u8>>,
}

impl TxOutput {
    /// Create a public P2PKH output (coinbase / legacy).
    pub fn new(amount: u64, public_key: Vec<u8>) -> Self {
        TxOutput {
            amount, public_key, spendable_after: 0,
            stealth_dest: None, commitment_bytes: None, range_proof_bytes: None,
        }
    }

    /// Create a private stealth output.
    pub fn new_private(
        amount: u64,
        dest: [u8; 32],
        commitment: [u8; 32],
        range_proof: Vec<u8>,
    ) -> Self {
        TxOutput {
            amount,  // kept for supply tracking
            public_key: vec![],
            spendable_after: 0,
            stealth_dest: Some(dest),
            commitment_bytes: Some(commitment),
            range_proof_bytes: Some(range_proof),
        }
    }

    /// Create a founder time-locked output.
    pub fn new_locked(amount: u64, public_key: Vec<u8>, block_number: u64) -> Self {
        let lock = if block_number < constants::RAMP_UP_BLOCKS {
            std::cmp::max(constants::FOUNDER_LOCK_BLOCKS, block_number + constants::FOUNDER_LOCK_ADDITIONAL)
        } else { 0 };
        TxOutput {
            amount, public_key, spendable_after: lock,
            stealth_dest: None, commitment_bytes: None, range_proof_bytes: None,
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
        h.update(self.nonce.to_le_bytes());
        h.update(self.elapsed_ms.to_le_bytes());
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
            if !o.public_key.is_empty() {
                h.update(&o.public_key);
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
    #[test] fn test_header_hash() {
        let h = BlockHeader { version: constants::PROTOCOL_VERSION, previous_hash: [0;32], merkle_root: [0;32], timestamp: 1000,
            height: 0, epoch: 0, difficulty_target: 1, total_effective_commit: 100., emission_rate: 100., miner_effective_commit: 50.,
            vr_block: 0.001, nonce: 42, elapsed_ms: 5000 };
        assert_eq!(h.hash(), h.hash());
    }
    #[test] fn test_different_nonce() {
        let mut a = BlockHeader { version: constants::PROTOCOL_VERSION, previous_hash: [0;32], merkle_root: [0;32], timestamp: 1000,
            height: 0, epoch: 0, difficulty_target: 1, total_effective_commit: 100., emission_rate: 100., miner_effective_commit: 50.,
            vr_block: 0.001, nonce: 42, elapsed_ms: 5000 };
        let mut b = a.clone(); b.nonce = 43;
        assert_ne!(a.hash(), b.hash());
    }
    #[test] fn test_tx_hash() {
        let tx = Transaction { version: 1, inputs: vec![TxInput { previous_tx_hash: [0;32], output_index: 0, key_image: [0;32] }],
            outputs: vec![TxOutput { amount: 1000, public_key: vec![0u8;32], spendable_after: 0,
                stealth_dest: None, commitment_bytes: None, range_proof_bytes: None }],
            ring_size: 11, signatures: vec![], mlsag: None, ring_members: None };
        assert_eq!(tx.hash(), tx.hash());
    }
    #[test] fn test_private_tx_hash() {
        let tx = Transaction { version: 1,
            inputs: vec![TxInput { previous_tx_hash: [0;32], output_index: 0, key_image: [1u8;32] }],
            outputs: vec![TxOutput {
                amount: 0, public_key: vec![], spendable_after: 0,
                stealth_dest: Some([2u8;32]), commitment_bytes: Some([3u8;32]),
                range_proof_bytes: Some(vec![4u8; 64]),
            }],
            ring_size: 11, signatures: vec![],
            mlsag: Some(MlsagData {
                ring_size: 11, n_layers: 1,
                key_images: vec![[5u8;32]],
                c0: [6u8;64],
                responses: vec![vec![[7u8;32]; 11]],
            }),
            ring_members: Some(vec![
                (0..11).map(|i| UtxoRef { tx_hash: [i as u8; 32], output_index: i }).collect()
            ]),
        };
        assert_eq!(tx.hash(), tx.hash());
        assert_ne!(tx.hash(), Transaction {
            version: 1, inputs: vec![TxInput { previous_tx_hash: [0;32], output_index: 0, key_image: [1u8;32] }],
            outputs: vec![TxOutput { amount: 0, public_key: vec![], spendable_after: 0,
                stealth_dest: Some([9u8;32]), commitment_bytes: Some([3u8;32]),
                range_proof_bytes: Some(vec![4u8; 64]) }],
            ring_size: 11, signatures: vec![],
            mlsag: Some(MlsagData { ring_size: 11, n_layers: 1, key_images: vec![[5u8;32]],
                c0: [6u8;64], responses: vec![vec![[7u8;32]; 11]] }),
            ring_members: Some(vec![
                (0..11).map(|i| UtxoRef { tx_hash: [i as u8; 32], output_index: i }).collect()
            ]),
        }.hash());
    }
}
