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
