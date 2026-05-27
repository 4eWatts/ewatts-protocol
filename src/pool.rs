use crate::block::Block;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

/// A share submitted by a pool miner to prove work
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    pub miner_id: [u8; 32],
    pub block_height: u64,
    pub nonce: u64,
    pub hash: [u8; 32],      // hash of the candidate block header
    pub difficulty: u64,      // the difficulty this share meets
    pub timestamp: u64,
}

/// Pool miner registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolMiner {
    pub id: [u8; 32],
    pub address: Vec<u8>,     // payout address
    pub shares: u64,          // total valid shares
    pub last_share_time: u64,
}

/// Mining pool state
pub struct MiningPool {
    pub miners: Vec<PoolMiner>,
    pub total_shares: u64,
    pub current_block_template: Option<Block>,
    pub pool_address: Vec<u8>,
    pub payout_threshold: u64,
}

impl MiningPool {
    pub fn new(pool_address: Vec<u8>) -> Self {
        MiningPool {
            miners: Vec::new(),
            total_shares: 0,
            current_block_template: None,
            pool_address,
            payout_threshold: 1_000_000, // 1 Ewatt minimum payout
        }
    }

    /// Register or update a miner
    pub fn register_miner(&mut self, id: [u8; 32], address: Vec<u8>) {
        if let Some(miner) = self.miners.iter_mut().find(|m| m.id == id) {
            miner.address = address;
        } else {
            self.miners.push(PoolMiner {
                id,
                address,
                shares: 0,
                last_share_time: 0,
            });
        }
    }

    /// Submit a share and return true if it's a valid block solution
    pub fn submit_share(&mut self, share: Share) -> bool {
        // Find or register miner
        if let Some(miner) = self.miners.iter_mut().find(|m| m.id == share.miner_id) {
            miner.shares += 1;
            miner.last_share_time = share.timestamp;
        }
        self.total_shares += 1;

        // Check if this share meets the block difficulty
        // A share that meets the network difficulty IS a block
        let target = share.difficulty;
        let hash_val = u64::from_be_bytes([share.hash[0], share.hash[1], share.hash[2], share.hash[3],
                                            share.hash[4], share.hash[5], share.hash[6], share.hash[7]]);
        hash_val < target
    }

    /// Calculate each miner's reward share
    pub fn calculate_rewards(&self, total_reward: u64) -> Vec<(Vec<u8>, u64)> {
        if self.total_shares == 0 {
            return vec![];
        }
        let pool_fee = 1u64; // 1% pool fee (scaled)
        let net_reward = total_reward.saturating_sub(total_reward / 100); // 1% fee

        self.miners.iter()
            .filter(|m| m.shares > 0)
            .map(|m| {
                let reward = net_reward.saturating_mul(m.shares) / self.total_shares;
                (m.address.clone(), reward)
            })
            .collect()
    }

    /// Get miner count
    pub fn miner_count(&self) -> usize {
        self.miners.len()
    }
}

/// Pool client state (run by each miner)
pub struct PoolClient {
    pub pool_url: String,
    pub miner_id: [u8; 32],
    pub worker_name: String,
}

impl PoolClient {
    pub fn new(pool_url: &str, miner_id: [u8; 32]) -> Self {
        PoolClient {
            pool_url: pool_url.to_string(),
            miner_id,
            worker_name: "default".to_string(),
        }
    }
}

/// Global pool state (thread-safe)
static GLOBAL_POOL: OnceLock<Mutex<Option<MiningPool>>> = OnceLock::new();

fn global_pool() -> &'static Mutex<Option<MiningPool>> {
    GLOBAL_POOL.get_or_init(|| Mutex::new(None))
}

/// Initialize the global pool
pub fn init_global_pool(pool_address: Vec<u8>) {
    let mut pool = global_pool().lock().unwrap();
    *pool = Some(MiningPool::new(pool_address));
}

/// Submit a share to the global pool
pub fn submit_share_to_pool(share: Share) -> bool {
    let mut pool = global_pool().lock().unwrap();
    if let Some(ref mut p) = *pool {
        p.submit_share(share)
    } else {
        false
    }
}

/// Register a miner in the global pool
pub fn register_in_pool(id: [u8; 32], address: Vec<u8>) {
    let mut pool = global_pool().lock().unwrap();
    if let Some(ref mut p) = *pool {
        p.register_miner(id, address);
    }
}

/// Get pool stats
pub fn pool_stats() -> serde_json::Value {
    let pool = global_pool().lock().unwrap();
    if let Some(ref p) = *pool {
        serde_json::json!({
            "miners": p.miner_count(),
            "total_shares": p.total_shares,
            "has_template": p.current_block_template.is_some(),
        })
    } else {
        serde_json::json!({"miners": 0, "total_shares": 0, "has_template": false})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_basic() {
        let mut pool = MiningPool::new(vec![0u8; 32]);
        assert_eq!(pool.miner_count(), 0);

        pool.register_miner([1u8; 32], vec![1u8; 32]);
        pool.register_miner([2u8; 32], vec![2u8; 32]);
        assert_eq!(pool.miner_count(), 2);

        let share = Share {
            miner_id: [1u8; 32],
            block_height: 1,
            nonce: 0,
            hash: [0xFF; 32],  // all 0xFF = max hash value, won't meet target
            difficulty: u64::MAX,
            timestamp: 1000,
        };
        assert!(!pool.submit_share(share), "High hash does not meet difficulty");

        assert_eq!(pool.total_shares, 1);
    }

    #[test]
    fn test_pool_rewards() {
        let mut pool = MiningPool::new(vec![0u8; 32]);
        pool.register_miner([1u8; 32], vec![1u8; 32]);
        pool.register_miner([2u8; 32], vec![2u8; 32]);

        // Submit 3 shares for miner 1, 1 share for miner 2
        for _ in 0..3 {
            pool.submit_share(Share {
                miner_id: [1u8; 32],
                block_height: 1, nonce: 0,
                hash: [0xFF; 32], difficulty: u64::MAX,
                timestamp: 1000,
            });
        }
        pool.submit_share(Share {
            miner_id: [2u8; 32],
            block_height: 1, nonce: 0,
            hash: [0xFF; 32], difficulty: u64::MAX,
            timestamp: 1000,
        });

        assert_eq!(pool.total_shares, 4);
        let rewards = pool.calculate_rewards(1000); // 1000 units reward
        // Miner 1: 3/4 of 99% of 1000 ≈ 742
        // Miner 2: 1/4 of 99% of 1000 ≈ 247
        assert!(rewards.len() >= 1);
        let miner1_reward = rewards.iter().find(|(addr, _)| addr[0] == 1).map(|(_, r)| *r).unwrap_or(0);
        let miner2_reward = rewards.iter().find(|(addr, _)| addr[0] == 2).map(|(_, r)| *r).unwrap_or(0);
        assert!(miner1_reward > miner2_reward, "Miner with more shares gets more reward");
        assert!(miner1_reward + miner2_reward <= 1000, "Total reward <= block reward");
    }

    #[test]
    fn test_share_detects_block() {
        let mut pool = MiningPool::new(vec![0u8; 32]);
        pool.register_miner([1u8; 32], vec![1u8; 32]);

        // A share with hash < difficulty → valid block!
        let share = Share {
            miner_id: [1u8; 32],
            block_height: 1, nonce: 0,
            hash: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                   0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            difficulty: 1000,
            timestamp: 1000,
        };
        assert!(pool.submit_share(share), "Share meeting difficulty must be a block");
    }
}
