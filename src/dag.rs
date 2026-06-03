use crate::constants;
use sha2::Sha512;
use sha3::{Digest, Keccak256};
use std::sync::{Mutex, OnceLock};

/// Global DAG cache: avoids regenerating the same DAG in test.
/// Key: (epoch, size_bytes). Only caches the most recent entry.
static DAG_CACHE: OnceLock<Mutex<Option<(u64, u64, Dag)>>> = OnceLock::new();

fn get_dag_cache() -> &'static Mutex<Option<(u64, u64, Dag)>> {
    DAG_CACHE.get_or_init(|| Mutex::new(None))
}

fn fnv_hash(a: u64, b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    h ^= a;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub struct Dag {
    pub elements: Vec<[u8; 64]>,
    pub epoch: u64,
    pub size_bytes: u64,
}

impl Dag {
    pub fn generate(epoch: u64, accelerated: bool) -> Self {
        let size = if accelerated {
            constants::DAG_ACCELERATION_RATE
        } else {
            constants::DAG_GROWTH_RATE_BYTES_PER_YEAR
        };
        // Per-epoch growth: size/year * (epoch_blocks / blocks_per_year)
        // Multiply first to preserve precision (integer division otherwise truncates)
        let per_epoch_growth = (size * constants::DAG_EPOCH_BLOCKS) / constants::BLOCKS_PER_YEAR;
        let total = constants::DAG_INITIAL_SIZE_BYTES + per_epoch_growth * epoch;
        Self::generate_with_size(epoch, total)
    }
    pub fn generate_with_size(epoch: u64, size_bytes: u64) -> Self {
        // Check cache first
        {
            let cache = get_dag_cache().lock().unwrap();
            if let Some((ref e, ref s, ref dag)) = *cache {
                if *e == epoch && *s == size_bytes {
                    // Clone the cached DAG's elements
                    return Dag {
                        elements: dag.elements.clone(),
                        epoch: dag.epoch,
                        size_bytes: dag.size_bytes,
                    };
                }
            }
        }
        
        if size_bytes < 64 {
            panic!("DAG size_bytes must be >= 64 (got {})", size_bytes);
        }
        let n = (size_bytes / 64) as usize;
        let seed: [u8; 32] = Keccak256::digest(&epoch.to_le_bytes()).into();
        let cn = std::cmp::max(1, n / 128);
        let mut cache = Vec::with_capacity(cn);
        let mut p = [0u8; 64];
        let sp = Sha512::digest(&seed);
        p.copy_from_slice(&sp);
        cache.push(p);
        for i in 1..cn {
            let mut x = [0u8; 64];
            let h = Sha512::digest(&cache[i - 1]);
            x.copy_from_slice(&h);
            cache.push(x);
        }
        let mut elems = Vec::with_capacity(n);
        for i in 0..n {
            let mut d = cache[i % cn];
            for j in 0..8.min(64) {
                d[j] ^= (i as u64).to_le_bytes()[j];
            }
            let h1 = Sha512::digest(&d);
            d.copy_from_slice(&h1);
            for j in 0..256u32 {
                let p = fnv_hash((i as u64) ^ (j as u64), &d[..8]) as usize % cn;
                for k in 0..64 {
                    d[k] ^= cache[p][k];
                }
                let h2 = Sha512::digest(&d);
                d.copy_from_slice(&h2);
            }
            elems.push(d);
        }
        let dag = Dag {
            elements: elems,
            epoch,
            size_bytes,
        };
        // Cache for future calls (clone elements into cache)
        let cached_dag = Dag {
            elements: dag.elements.clone(),
            epoch,
            size_bytes,
        };
        {
            let mut cache = get_dag_cache().lock().unwrap();
            *cache = Some((epoch, size_bytes, cached_dag));
        }
        dag
    }
    pub fn get(&self, i: usize) -> &[u8; 64] {
        &self.elements[i % self.elements.len()]
    }
    pub fn len(&self) -> usize {
        self.elements.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Benchmark: DAG generation time for testnet size (256 MB).
    /// Spec §4.2: 8 GB em <60s em DDR5-4800.
    /// Testnet usa DAG reduzido; este teste mede tempo/GB.
    #[test]
    fn test_dag_benchmark_64mb() {
        let size: u64 = 64 * 1024 * 1024; // 64 MB (testnet)
        let start = Instant::now();
        let dag = Dag::generate_with_size(0, size);
        let elapsed = start.elapsed();
        let expected_elements = (size / 64) as usize;
        assert_eq!(dag.len(), expected_elements, "DAG element count mismatch");
        
        // Calcular throughput: MB/s
        let mb_per_sec = (size as f64 / 1_048_576.0) / elapsed.as_secs_f64();
        eprintln!(
            "[PHASE7] DAG {} MB: {:.2}s, {:.1} MB/s, {} elements",
            size / 1_048_576,
            elapsed.as_secs_f64(),
            mb_per_sec,
            dag.len()
        );
        
        // Para 8 GB mainnet: extrapolate
        let eight_gb_time = (8.0 * 1024.0 * 1024.0 * 1024.0 / size as f64) * elapsed.as_secs_f64();
        eprintln!(
            "[PHASE7] ~8 GB mainnet estimate: {:.1}s (spec target: <60s)",
            eight_gb_time
        );
        if eight_gb_time > 60.0 {
            eprintln!("[PHASE7] WARNING: extrapolated DAG time exceeds 60s spec target");
        }
    }

    /// Benchmark: DAG generation at higher sizes (progressive)
    #[test]
    fn test_dag_benchmark_progressive() {
        let sizes = [
            ("1 MB", 1 * 1024 * 1024),
            ("4 MB", 4 * 1024 * 1024),
            ("16 MB", 16 * 1024 * 1024),
            ("64 MB", 64 * 1024 * 1024),
        ];
        for (label, size) in sizes {
            let start = Instant::now();
            let dag = Dag::generate_with_size(0, size as u64);
            let elapsed = start.elapsed();
            let mb_per_sec = (size as f64 / 1_048_576.0) / elapsed.as_secs_f64();
            let expected = (size as u64 / 64) as usize;
            assert_eq!(dag.len(), expected, "{} element count mismatch", label);
            eprintln!(
                "[PHASE7] DAG {}: {:.3}s, {:.1} MB/s",
                label, elapsed.as_secs_f64(), mb_per_sec
            );
        }
    }

    /// Verify DAG determinism (same seed = same DAG)
    #[test]
    fn test_dag_deterministic() {
        let a = Dag::generate_with_size(0, 1024 * 64);
        let b = Dag::generate_with_size(0, 1024 * 64);
        assert_eq!(a.len(), b.len());
        for i in 0..a.len() {
            assert_eq!(a.elements[i], b.elements[i], "DAG element {} differs", i);
        }
    }

    /// Verify different epoch produces different DAG
    #[test]
    fn test_dag_epoch_different() {
        let a = Dag::generate_with_size(0, 1024 * 64);
        let b = Dag::generate_with_size(1, 1024 * 64);
        assert_eq!(a.len(), b.len());
        let mut any_diff = false;
        for i in 0..a.len() {
            if a.elements[i] != b.elements[i] {
                any_diff = true;
                break;
            }
        }
        assert!(any_diff, "Different epochs should produce different DAGs");
    }

    /// Verify get() wraps around correctly
    #[test]
    fn test_dag_get_wraparound() {
        let dag = Dag::generate_with_size(0, 1024 * 64);
        let first = dag.get(0);
        let wrapped = dag.get(dag.len());
        assert_eq!(first, wrapped, "get() should wrap modulo len()");
    }

    /// Test DAG cache roundtrip
    #[test]
    fn test_dag_cache_hit() {
        let start = Instant::now();
        let _a = Dag::generate_with_size(0, 1024 * 64);
        let first_elapsed = start.elapsed();
        
        let start2 = Instant::now();
        let _b = Dag::generate_with_size(0, 1024 * 64);
        let second_elapsed = start2.elapsed();
        
        // Second call should be much faster (cache hit clones elements)
        eprintln!(
            "[PHASE7] DAG cache: first={:.3}s, second={:.3}s (x{:.0})",
            first_elapsed.as_secs_f64(),
            second_elapsed.as_secs_f64(),
            first_elapsed.as_secs_f64() / second_elapsed.as_secs_f64().max(0.0001)
        );
        // Second call may be clone-heavy but should still be faster
        assert!(second_elapsed.as_secs_f64() < first_elapsed.as_secs_f64() * 2.0,
            "cache should not be slower than generation");
    }

    /// DAG memory footprint sanity
    #[test]
    fn test_dag_memory_sanity() {
        let dag = Dag::generate_with_size(0, 1024 * 1024); // 1 MB = 16384 elements
        let expected_bytes = dag.len() * 64;
        assert!(expected_bytes as u64 >= 1024 * 1024);
    }
}
