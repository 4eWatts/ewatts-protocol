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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub version: u16,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub ring_size: u16,
    pub signatures: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub previous_tx_hash: [u8; 32],
    pub output_index: u32,
    pub key_image: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub amount: u64,
    pub pubkey_hash: [u8; 20],    // P2PKH: H(public_key) instead of public key
    pub spendable_after: u64,      // Founder time-lock: 0 = immediate
}

impl TxOutput {
    /// Create a new P2PKH output with optional time-lock
    pub fn new(amount: u64, pubkey_hash: [u8; 20]) -> Self {
        TxOutput { amount, pubkey_hash, spendable_after: 0 }
    }

    /// Create a founder time-locked output (only for coinbase during ramp-up)
    pub fn new_locked(amount: u64, pubkey_hash: [u8; 20], block_number: u64) -> Self {
        let lock = if block_number < constants::RAMP_UP_BLOCKS {
            std::cmp::max(constants::FOUNDER_LOCK_BLOCKS, block_number + constants::FOUNDER_LOCK_ADDITIONAL)
        } else { 0 };
        TxOutput { amount, pubkey_hash, spendable_after: lock }
    }

    /// Check if this output is spendable at the current block
    pub fn is_spendable(&self, current_block: u64) -> bool {
        current_block >= self.spendable_after
    }
}

impl BlockHeader {
    pub fn hash(&self) -> [u8; 32] {
        let mut h = Keccak256::new();
        h.update(self.version.to_le_bytes());
        h.update(self.previous_hash);
        h.update(self.merkle_root);
        h.update(self.timestamp.to_le_bytes());
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
        for i in &self.inputs { h.update(i.previous_tx_hash); h.update(i.output_index.to_le_bytes()); h.update(i.key_image); }
        for o in &self.outputs { h.update(o.amount.to_le_bytes()); h.update(&o.pubkey_hash); }
        h.update(self.ring_size.to_le_bytes());
        h.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_header_hash() {
        let h = BlockHeader { version: constants::PROTOCOL_VERSION, previous_hash: [0;32], merkle_root: [0;32], timestamp: 1000,
            epoch: 0, difficulty_target: 1, total_effective_commit: 100., emission_rate: 100., miner_effective_commit: 50.,
            vr_block: 0.001, nonce: 42, elapsed_ms: 5000 };
        assert_eq!(h.hash(), h.hash());
    }
    #[test] fn test_different_nonce() {
        let mut a = BlockHeader { version: constants::PROTOCOL_VERSION, previous_hash: [0;32], merkle_root: [0;32], timestamp: 1000,
            epoch: 0, difficulty_target: 1, total_effective_commit: 100., emission_rate: 100., miner_effective_commit: 50.,
            vr_block: 0.001, nonce: 42, elapsed_ms: 5000 };
        let mut b = a.clone(); b.nonce = 43;
        assert_ne!(a.hash(), b.hash());
    }
    #[test] fn test_tx_hash() {
        let tx = Transaction { version: 1, inputs: vec![TxInput { previous_tx_hash: [0;32], output_index: 0, key_image: [0;32] }],
            outputs: vec![TxOutput { amount: 1000, pubkey_hash: [0u8;20] }], ring_size: 11 };
        assert_eq!(tx.hash(), tx.hash());
    }
}
