use crate::constants;
use crate::dag::Dag;
use rand::Rng;
use sha2::Sha512;
use sha3::{Digest, Keccak256};

pub struct AccessSample {
    pub position: u64,
    pub dag_index: u64,
    pub mix_hash: [u8; 64],
    pub elapsed_offset_us: u64,
}

pub struct Solution {
    pub nonce: u64,
    pub proof_trace: Vec<AccessSample>,
    pub elapsed_ms: u64,
    pub walk_length: u64,
}

fn read_u64_le(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    let len = bytes.len().min(8);
    buf[..len].copy_from_slice(&bytes[..len]);
    u64::from_le_bytes(buf)
}

pub fn meets_difficulty(hash: &[u8; 32], difficulty: u64) -> bool {
    let target = u64::MAX / difficulty.max(1);
    read_u64_le(&hash[..8]) <= target
}

pub fn difficulty_to_accesses(difficulty: u64) -> u64 {
    constants::BASE_ACCESSES * difficulty / 1_000_000_000
}

fn initial_mix(header_hash: &[u8; 32], nonce: u64) -> [u8; 64] {
    let mut hasher = Keccak256::new();
    hasher.update(header_hash);
    hasher.update(nonce.to_le_bytes());
    let r: [u8; 32] = hasher.finalize().into();
    let mut mix = [0u8; 64];
    mix[..32].copy_from_slice(&r);
    let mut hasher2 = Sha512::new();
    hasher2.update(&r);
    hasher2.update(nonce.to_le_bytes());
    let r2: [u8; 64] = hasher2.finalize().into();
    mix[32..].copy_from_slice(&r2[..32]);
    mix
}

pub fn mine(
    header_hash: &[u8; 32],
    difficulty: u64,
    dag: &Dag,
    nonce_limit: u64,
) -> Option<Solution> {
    let walk_length = difficulty_to_accesses(difficulty);
    // Integer math: VERIFICATION_SAMPLE_RATE = 0.001 = 1/1000
    let sample_interval = std::cmp::max(1, walk_length / 1000);
    let mut rng = rand::thread_rng();
    for _attempt in 0..nonce_limit {
        let nonce: u64 = rng.gen();
        let mut mix = initial_mix(header_hash, nonce);
        let start = std::time::Instant::now();
        let mut trace = Vec::new();
        for i in 0..walk_length {
            let index = read_u64_le(&mix[..8]) % dag.len() as u64;
            let element = dag.get(index as usize);
            for k in 0..64 {
                mix[k] ^= element[k];
            }
            let mut h = Sha512::new();
            h.update(&mix);
            mix.copy_from_slice(&h.finalize());
            if i % sample_interval == 0 {
                trace.push(AccessSample {
                    position: i,
                    dag_index: index,
                    mix_hash: mix,
                    elapsed_offset_us: start.elapsed().as_micros() as u64,
                });
            }
        }
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let final_hash: [u8; 32] = Keccak256::digest(&mix).into();
        if meets_difficulty(&final_hash, difficulty) {
            return Some(Solution {
                nonce,
                proof_trace: trace,
                elapsed_ms,
                walk_length,
            });
        }
    }
    None
}

pub fn verify(
    header_hash: &[u8; 32],
    solution: &Solution,
    difficulty: u64,
    dag: &Dag,
) -> Result<(), String> {
    let walk_length = difficulty_to_accesses(difficulty);
    if solution.walk_length != walk_length {
        return Err(format!(
            "Walk length mismatch: {} vs {}",
            walk_length, solution.walk_length
        ));
    }
    // Sample interval for proof trace verification.
    // If proof_trace is empty (e.g., testnet blocks without trace),
    // we skip sample checks and only verify the final hash.
    let mut mix = initial_mix(header_hash, solution.nonce);

    if solution.proof_trace.is_empty() {
        // Fast verification: full walk, no sample checks (testnet / lightweight).
        // Used when the solution has no trace data (blocks not mined with sampling).
        for i in 0..walk_length {
            let element = dag.get(read_u64_le(&mix[..8]) as usize % dag.len());
            for k in 0..64 {
                mix[k] ^= element[k];
            }
            let mut h = Sha512::new();
            h.update(&mix);
            mix.copy_from_slice(&h.finalize());
        }
    } else {
        // Full verification with proof trace sampling.
        let sample_interval = std::cmp::max(1, walk_length / 1000);
        let mut last_offset: u64 = 0;
        for i in 0..walk_length {
            let element = dag.get(read_u64_le(&mix[..8]) as usize % dag.len());
            for k in 0..64 {
                mix[k] ^= element[k];
            }
            let mut h = Sha512::new();
            h.update(&mix);
            mix.copy_from_slice(&h.finalize());
            if i % sample_interval == 0 {
                let idx = i / sample_interval;
                if idx >= solution.proof_trace.len() as u64 {
                    return Err("Missing sample".to_string());
                }
                let s = &solution.proof_trace[idx as usize];
                if s.position != i {
                    return Err("Position mismatch".to_string());
                }
                if s.mix_hash != mix {
                    return Err("Mix hash mismatch".to_string());
                }
                // Verify elapsed offset is monotonic (detects gross timing manipulation)
                if s.elapsed_offset_us < last_offset {
                    return Err("Non-monotonic elapsed offset".to_string());
                }
                last_offset = s.elapsed_offset_us;
            }
        }
    }
    let final_hash: [u8; 32] = Keccak256::digest(&mix).into();
    if !meets_difficulty(&final_hash, difficulty) {
        return Err("Difficulty not met".to_string());
    }
    Ok(())
}

pub struct WorkReport {
    pub nonce: u64,
    pub walk_length: u64,
    pub elapsed_ms: u64,
    pub gb_processed: f64,
    pub gbps: f64,
}

impl WorkReport {
    pub fn from_solution(s: &Solution) -> Self {
        let bytes = s.walk_length * constants::DAG_ELEMENT_SIZE as u64;
        let gb = bytes as f64 / (1073741824.0);
        let gbps = if s.elapsed_ms > 0 {
            gb / (s.elapsed_ms as f64 / 1000.0)
        } else {
            0.0
        };
        WorkReport {
            nonce: s.nonce,
            walk_length: s.walk_length,
            elapsed_ms: s.elapsed_ms,
            gb_processed: gb,
            gbps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::Dag;
    #[test]
    fn test_initial_mix_deterministic() {
        assert_eq!(initial_mix(&[0u8; 32], 42), initial_mix(&[0u8; 32], 42));
    }
    #[test]
    fn test_mine_and_verify() {
        let dag = Dag::generate(0, false);
        let h = [0xabu8; 32];
        let s = mine(&h, 1, &dag, 100).unwrap();
        assert!(verify(&h, &s, 1, &dag).is_ok());
    }
    #[test]
    fn test_walk_length() {
        let accesses = difficulty_to_accesses(1);
        assert!(accesses > 0);
    }
    #[test]
    fn test_meets_difficulty_basic() {
        let max_hash = [0xffu8; 32];
        assert!(meets_difficulty(&max_hash, 1));
        let zero_hash = [0x00u8; 32];
        // zero_hash = 0 in [0..7], 0 <= u64::MAX / 1 = u64::MAX, so it meets
        assert!(meets_difficulty(&zero_hash, 1));
    }
}
