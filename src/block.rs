use crate::commitment::Commitment;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

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
pub struct Block {
    pub header: BlockHeader,
    pub body: BlockBody,
}
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
    pub public_key: Vec<u8>,
}

pub fn merkle_root(txns: &[Transaction]) -> [u8; 32] {
    if txns.is_empty() {
        return Keccak256::digest(&[]).into();
    }
    let mut hashes: Vec<[u8; 32]> = txns.iter().map(|t| t.hash()).collect();
    while hashes.len() > 1 {
        if hashes.len() % 2 == 1 {
            hashes.push(hashes.last().unwrap().clone());
        }
        let mut next = Vec::with_capacity(hashes.len() / 2);
        for chunk in hashes.chunks(2) {
            let mut h = Keccak256::new();
            h.update(&chunk[0]);
            h.update(&chunk[1]);
            next.push(h.finalize().into());
        }
        hashes = next;
    }
    hashes[0]
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
        for i in &self.inputs {
            h.update(i.previous_tx_hash);
            h.update(i.output_index.to_le_bytes());
            h.update(i.key_image);
        }
        for o in &self.outputs {
            h.update(o.amount.to_le_bytes());
            h.update(&o.public_key);
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
            version: 3,
            previous_hash: [0; 32],
            merkle_root: [0; 32],
            timestamp: 1000,
            epoch: 0,
            difficulty_target: 1,
            total_effective_commit: 100.,
            emission_rate: 100.,
            miner_effective_commit: 50.,
            vr_block: 0.001,
            nonce: 42,
            elapsed_ms: 5000,
        };
        assert_eq!(h.hash(), h.hash());
    }
}
