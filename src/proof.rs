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
    /// Optional Merkle root over the proof trace samples (Opção B).
    /// When Some, verifier can use sampled verification.
    pub merkle_root: Option<[u8; 32]>,
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

// ─── Merkle tree utilities (Opção B) ──────────────────────────────────

/// Build a binary Merkle root from a list of 32-byte leaf hashes.
/// Odd-numbered leaves at any level are paired with themselves (duplicated).
pub fn merkle_root_from_leaves(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        for chunk in level.chunks(2) {
            let mut h = Keccak256::new();
            h.update(chunk[0]);
            if chunk.len() > 1 {
                h.update(chunk[1]);
            } else {
                h.update(chunk[0]); // self-pair
            }
            next.push(h.finalize().into());
        }
        level = next;
    }
    level[0]
}

/// Compute the leaf hash for a single access sample (position + mix_hash).
pub fn sample_leaf_hash(position: u64, mix_hash: &[u8; 64]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(position.to_le_bytes());
    h.update(mix_hash);
    h.finalize().into()
}

/// Generate a random set of sample indices to check (N indices from 0..total).
pub fn sample_indices(n: usize, total: usize, rng: &mut impl rand::Rng) -> Vec<usize> {
    if total == 0 {
        return vec![];
    }
    let n = n.min(total);
    let mut indices: Vec<usize> = (0..total).collect();
    // Fisher-Yates partial shuffle: only first N
    for i in 0..n {
        let j = rng.gen_range(i..total);
        indices.swap(i, j);
    }
    indices[..n].to_vec()
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
            // Build Merkle root from proof trace (if any samples collected)
            let merkle_root = if trace.is_empty() {
                None
            } else {
                let leaf_hashes: Vec<[u8; 32]> = trace
                    .iter()
                    .map(|s| sample_leaf_hash(s.position, &s.mix_hash))
                    .collect();
                Some(merkle_root_from_leaves(&leaf_hashes))
            };
            return Some(Solution {
                nonce,
                proof_trace: trace,
                elapsed_ms,
                walk_length,
                merkle_root,
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

    let sample_interval = std::cmp::max(1, walk_length / 1000);

    // ── Fast path: empty trace, full walk (no samples to verify) ────
    if solution.proof_trace.is_empty() {
        let mut mix = initial_mix(header_hash, solution.nonce);
        for _i in 0..walk_length {
            let element = dag.get(read_u64_le(&mix[..8]) as usize % dag.len());
            for k in 0..64 { mix[k] ^= element[k]; }
            let mut h = Sha512::new();
            h.update(&mix);
            mix.copy_from_slice(&h.finalize());
        }
        let final_hash: [u8; 32] = Keccak256::digest(&mix).into();
        if !meets_difficulty(&final_hash, difficulty) {
            return Err("Difficulty not met".to_string());
        }
        return Ok(());
    }

    // ── Verificação amostrada com Merkle root (Opção B) ─────────────
    //    Usado quando a solução carrega o proof_trace + merkle_root.
    //    Verifica N=30 amostras aleatórias em vez do walk completo.
    if let Some(merkle_root) = solution.merkle_root {
        // 1) Verify the Merkle root commits to the proof trace
        let leaf_hashes: Vec<[u8; 32]> = solution.proof_trace
            .iter()
            .map(|s| sample_leaf_hash(s.position, &s.mix_hash))
            .collect();
        let computed_root = merkle_root_from_leaves(&leaf_hashes);
        if computed_root != merkle_root {
            return Err("Merkle root mismatch".to_string());
        }

        // 2) Check N random sample positions against full recompute
        let total = solution.proof_trace.len();
        let n = 30usize.min(total);
        let mut rng = rand::thread_rng();
        let indices = sample_indices(n, total, &mut rng);

        for &si in &indices {
            let target = &solution.proof_trace[si];
            let mut m = initial_mix(header_hash, solution.nonce);
            for _pos in 0..=target.position {
                let el = dag.get(read_u64_le(&m[..8]) as usize % dag.len());
                for k in 0..64 { m[k] ^= el[k]; }
                let mut h = Sha512::new();
                h.update(&m);
                m.copy_from_slice(&h.finalize());
            }
            if m != target.mix_hash {
                return Err(format!("Sample {} mix hash mismatch", target.position));
            }
        }

        // 3) Walk from last sample to end, check final hash meets difficulty
        let last = solution.proof_trace.last().unwrap();
        let mut mix = last.mix_hash;
        for _pos in (last.position + 1)..walk_length {
            let el = dag.get(read_u64_le(&mix[..8]) as usize % dag.len());
            for k in 0..64 { mix[k] ^= el[k]; }
            let mut h = Sha512::new();
            h.update(&mix);
            mix.copy_from_slice(&h.finalize());
        }
        let final_hash: [u8; 32] = Keccak256::digest(&mix).into();
        if !meets_difficulty(&final_hash, difficulty) {
            return Err("Difficulty not met".to_string());
        }
        return Ok(());
    }

    // ── Fallback: full walk com verificação do proof trace (legacy) ──
    let mut mix = initial_mix(header_hash, solution.nonce);
    let mut last_offset: u64 = 0;
    for i in 0..walk_length {
        let element = dag.get(read_u64_le(&mix[..8]) as usize % dag.len());
        for k in 0..64 { mix[k] ^= element[k]; }
        let mut h = Sha512::new();
        h.update(&mix);
        mix.copy_from_slice(&h.finalize());
        if i % sample_interval == 0 {
            let idx = i / sample_interval;
            if idx >= solution.proof_trace.len() as u64 {
                return Err("Missing sample".to_string());
            }
            let s = &solution.proof_trace[idx as usize];
            if s.position != i { return Err("Position mismatch".to_string()); }
            if s.mix_hash != mix { return Err("Mix hash mismatch".to_string()); }
            if s.elapsed_offset_us < last_offset {
                return Err("Non-monotonic elapsed offset".to_string());
            }
            last_offset = s.elapsed_offset_us;
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
    /// Access Operations Per Second (AOPS) — métrica primária.
    pub aops: f64,
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
        let aops = if s.elapsed_ms > 0 {
            s.walk_length as f64 / (s.elapsed_ms as f64 / 1000.0)
        } else {
            0.0
        };
        WorkReport {
            nonce: s.nonce,
            walk_length: s.walk_length,
            elapsed_ms: s.elapsed_ms,
            gb_processed: gb,
            gbps,
            aops,
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
        // Use a small DAG (64KB) to avoid 8GB generation during tests
        let dag = Dag::generate_with_size(0, 64 * 1024);
        let h = [0xabu8; 32];
        let s = mine(&h, 1, &dag, 100).unwrap();
        assert!(verify(&h, &s, 1, &dag).is_ok());
        // Merkle root should be present when proof_trace is non-empty
        assert!(s.merkle_root.is_some(), "mine should produce merkle root");
    }
    #[test]
    fn test_merkle_root_verify() {
        // Verify that the Merkle root correctly commits to the proof trace
        let dag = Dag::generate_with_size(0, 64 * 1024);
        let h = [0xabu8; 32];
        let s = mine(&h, 1, &dag, 100).unwrap();
        // Recompute root from trace
        let leaf_hashes: Vec<[u8; 32]> = s.proof_trace.iter().map(|s|
            sample_leaf_hash(s.position, &s.mix_hash)
        ).collect();
        let computed = merkle_root_from_leaves(&leaf_hashes);
        assert_eq!(Some(computed), s.merkle_root, "Merkle root should match trace");
        // Tampered leaf should produce different root
        let tampered: Vec<[u8; 32]> = s.proof_trace.iter().map(|s| {
            let mut mix = s.mix_hash;
            mix[0] ^= 0xff;
            sample_leaf_hash(s.position, &mix)
        }).collect();
        assert_ne!(merkle_root_from_leaves(&tampered), computed, "Tampered trace should differ");
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
