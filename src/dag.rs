use crate::constants;
use sha2::Sha512;
use sha3::{Digest, Keccak256};

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
        let total = constants::DAG_INITIAL_SIZE_BYTES
            + size * epoch / constants::BLOCKS_PER_YEAR * constants::DAG_EPOCH_BLOCKS;
        Self::generate_with_size(epoch, total)
    }
    pub fn generate_with_size(epoch: u64, size_bytes: u64) -> Self {
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
        Dag {
            elements: elems,
            epoch,
            size_bytes,
        }
    }
    pub fn get(&self, i: usize) -> &[u8; 64] {
        &self.elements[i % self.elements.len()]
    }
    pub fn len(&self) -> usize {
        self.elements.len()
    }
}
