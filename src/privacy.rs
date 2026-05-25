//! Ring signatures (MLSAG), stealth addresses, Pedersen commitments.

use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;
use digest::{ExtendableOutput, Update, XofReader};
use rand::rngs::ThreadRng;
use serde::{Deserialize, Serialize};
use sha3::Shake256;

fn scalar_zero() -> Scalar {
    Scalar::from(0u64)
}

/// Generator G (independent from ed25519 base point)
pub fn ring_g() -> RistrettoPoint {
    hash_to_point(b"Ewatts_Ring_G_v1")
}

/// Generator H for Pedersen commitments (blinding)
pub fn pedersen_h() -> RistrettoPoint {
    hash_to_point(b"Ewatts_Pedersen_H_v1")
}

/// Hash a pubkey to a point (for key images)
pub fn hash_pk(pk: &RistrettoPoint) -> RistrettoPoint {
    hash_to_point(pk.compress().as_bytes())
}

/// Deterministic hash-to-scalar via Shake256
pub fn hash_to_scalar(data: &[u8]) -> Scalar {
    let mut hasher = Shake256::default();
    hasher.update(data);
    let mut reader = hasher.finalize_xof();
    let mut bytes = [0u8; 64];
    reader.read(&mut bytes);
    Scalar::from_bytes_mod_order_wide(&bytes)
}

/// Deterministic hash-to-point via Shake256
pub fn hash_to_point(data: &[u8]) -> RistrettoPoint {
    let mut hasher = Shake256::default();
    hasher.update(b"Ewatts_HTP_v1:");
    hasher.update(data);
    let mut reader = hasher.finalize_xof();
    let mut seed = [0u8; 64];
    reader.read(&mut seed);
    let mut attempt = 0u64;
    loop {
        let mut candidate = [0u8; 32];
        let mut c_hasher = Shake256::default();
        c_hasher.update(&seed);
        c_hasher.update(&attempt.to_le_bytes());
        let mut c_reader = c_hasher.finalize_xof();
        c_reader.read(&mut candidate);
        if let Some(pt) = CompressedRistretto(candidate).decompress() {
            return pt;
        }
        attempt += 1;
    }
}

/// MLSAG challenge: c = H(msg, L, R)
fn mlsag_challenge(msg: &[u8], l: &RistrettoPoint, r: &RistrettoPoint) -> Scalar {
    let mut hasher = Shake256::default();
    hasher.update(b"MLSAG_c:");
    hasher.update(msg);
    hasher.update(l.compress().as_bytes());
    hasher.update(r.compress().as_bytes());
    let mut reader = hasher.finalize_xof();
    let mut c_bytes = [0u8; 64];
    reader.read(&mut c_bytes);
    Scalar::from_bytes_mod_order_wide(&c_bytes)
}

/// Stealth address (spend + view public keys)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StealthAddress {
    pub spend_key: RistrettoPoint,
    pub view_key: RistrettoPoint,
}

/// One-time destination (dest + ephemeral key)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneTimeAddress {
    pub dest: RistrettoPoint,
    pub ephemeral: RistrettoPoint,
}

/// Stealth private key
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OneTimeKey {
    pub spend: Scalar,
    pub view: Scalar,
}

impl StealthAddress {
    pub fn generate(rng: &mut ThreadRng) -> (Self, OneTimeKey) {
        let spend = Scalar::random(rng);
        let view = Scalar::random(rng);
        let addr = StealthAddress {
            spend_key: ring_g() * spend,
            view_key: ring_g() * view,
        };
        (addr, OneTimeKey { spend, view })
    }

    pub fn derive_destination(&self, rng: &mut ThreadRng) -> (OneTimeAddress, Scalar) {
        let r = Scalar::random(rng);
        let shared = r * self.view_key;
        let h = hash_to_scalar(shared.compress().as_bytes());
        let dest = h * ring_g() + self.spend_key;
        let ephemeral = r * ring_g();
        (OneTimeAddress { dest, ephemeral }, r)
    }
}

/// Recover the one-time private key scalar.
pub fn recover_one_time_key(
    view_secret: &Scalar,
    spend_secret: &Scalar,
    ephemeral: &RistrettoPoint,
) -> Scalar {
    let shared = view_secret * ephemeral;
    let h = hash_to_scalar(shared.compress().as_bytes());
    h + spend_secret
}

// ─── Pedersen Commitment ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commitment(pub RistrettoPoint);

impl Commitment {
        pub fn new(v: u64, rng: &mut ThreadRng) -> (Self, Scalar) {
        let a = Scalar::random(rng);
        (Commitment::new_with_blinding(v, a), a)
    }

    pub fn new_with_blinding(v: u64, a: Scalar) -> Self {
        let point = a * ring_g() + Scalar::from(v) * pedersen_h();
        Commitment(point)
    }

    pub fn verify(&self, v: u64, a: Scalar) -> bool {
        self.0 == (a * ring_g() + Scalar::from(v) * pedersen_h())
    }

    pub fn add(&self, other: &Commitment) -> Self {
        Commitment(self.0 + other.0)
    }

    pub fn negate(&self) -> Self {
        Commitment(-self.0)
    }

    pub fn zero() -> Self {
        Commitment(RistrettoPoint::identity())
    }

    pub fn is_identity(&self) -> bool {
        self.0 == RistrettoPoint::identity()
    }
}

/// Multi-layered Linkable Spontaneous Anonymous Group signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MLSAGSignature {
    pub ring_size: usize,
    pub n_layers: usize,
    pub key_images: Vec<RistrettoPoint>,
    pub c0: Scalar,
    pub responses: Vec<Vec<Scalar>>,
}

impl MLSAGSignature {
        /// Sign. NOT constant-time w.r.t. real_index (testnet only).
    pub fn sign(
        ring: &[Vec<RistrettoPoint>],
        secret_keys: &[Scalar],
        real_index: usize,
        msg: &[u8],
        rng: &mut ThreadRng,
    ) -> Self {
        let ring_size = ring.len();
        let n_layers = secret_keys.len();

        // Key images
        let mut key_images = Vec::with_capacity(n_layers);
        for j in 0..n_layers {
            key_images.push(secret_keys[j] * hash_pk(&ring[real_index][j]));
        }

        // α for real signer
        let alpha: Vec<Scalar> = (0..n_layers).map(|_| Scalar::random(rng)).collect();

        // Random responses for non-real positions
        let mut responses = vec![vec![scalar_zero(); n_layers]; ring_size];
        for i in 0..ring_size {
            if i == real_index {
                continue;
            }
            for j in 0..n_layers {
                responses[i][j] = Scalar::random(rng);
            }
        }

        // challenges[i] = c_{i+1} (output of position i)
        let mut challenges = vec![scalar_zero(); ring_size];
        let mut c;

        // α step at π: L = Σα_j * G, R = Σα_j * H_p(K_π)
        {
            let mut l = RistrettoPoint::identity();
            let mut r = RistrettoPoint::identity();
            for j in 0..n_layers {
                l = l + alpha[j] * ring_g();
                r = r + alpha[j] * hash_pk(&ring[real_index][j]);
            }
            c = mlsag_challenge(msg, &l, &r);
        }
        challenges[real_index] = c;

        // Walk: π+1, π+2, ..., m-1, 0, 1, ..., π-1
        for step in 1..ring_size {
            let idx = (real_index + step) % ring_size;
            let c_prev = challenges[(idx + ring_size - 1) % ring_size];

            let mut l = RistrettoPoint::identity();
            let mut r = RistrettoPoint::identity();
            for j in 0..n_layers {
                l = l + responses[idx][j] * ring_g() + c_prev * ring[idx][j];
                r = r + responses[idx][j] * hash_pk(&ring[idx][j]) + c_prev * key_images[j];
            }
            c = mlsag_challenge(msg, &l, &r);
            challenges[idx] = c;
        }

        // c_π = c_{π} (output of position π-1) = challenges[(π+ring_size-1)%ring_size]
        let c_pi = challenges[(real_index + ring_size - 1) % ring_size];
        for j in 0..n_layers {
            responses[real_index][j] = alpha[j] - c_pi * secret_keys[j];
        }

        // c0 = c_0 = output of position m-1 (= input to position 0)
        let c0 = challenges[ring_size - 1];

        MLSAGSignature {
            ring_size,
            n_layers,
            key_images,
            c0,
            responses,
        }
    }

    /// Verify MLSAG signature.
    /// Walk ring from 0..m-1, each position: c_{i+1} = H(msg, r_i*G + c_i*K_i, r_i*H_p(K_i) + c_i*I)
    /// Check final c == stored c0.
    const MIN_RING_SIZE: usize = 2;

    pub fn verify(&self, ring: &[Vec<RistrettoPoint>], msg: &[u8]) -> bool {
        if ring.len() != self.ring_size {
            return false;
        }
        if self.ring_size < Self::MIN_RING_SIZE || self.n_layers == 0 {
            return false;
        }
        if ring.is_empty() || ring[0].len() != self.n_layers {
            return false;
        }
        if self.responses.len() != self.ring_size {
            return false;
        }
        if self.key_images.len() != self.n_layers {
            return false;
        }
        for r in &self.responses {
            if r.len() != self.n_layers {
                return false;
            }
        }
        for r in ring {
            if r.len() != self.n_layers {
                return false;
            }
        }

        let mut c = self.c0;

        for i in 0..self.ring_size {
            let mut l = RistrettoPoint::identity();
            let mut r = RistrettoPoint::identity();
            for j in 0..self.n_layers {
                l = l + self.responses[i][j] * ring_g() + c * ring[i][j];
                r = r + self.responses[i][j] * hash_pk(&ring[i][j]) + c * self.key_images[j];
            }
            c = mlsag_challenge(msg, &l, &r);
        }

        c == self.c0
    }
}

// ─── Range Proof (bit-decomposition with MLSAG) ────────────────────────────

/// A range proof using bit decomposition: C = sum(2^i * C_i), each C_i ∈ {0, 1}.
/// Each bit is proven with a 1-out-of-2 MLSAG ring signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeProof {
    pub bits: usize,
    pub commitments: Vec<Commitment>,
    pub proofs: Vec<MLSAGSignature>,
}

impl RangeProof {
    /// Prove that a commitment opens to v ∈ [0, 2^bits) with a given total blinding.
    /// The last bit's ai is computed so that Σ(a_i · 2^i) == blinding.
    pub fn prove(v: u64, blinding: Scalar, bits: usize, rng: &mut ThreadRng) -> Self {
        Self::prove_with_exact_blinding(v, blinding, bits, rng)
    }

    /// Prove v ∈ [0, 2^bits) and return the total blinding factor.
    pub fn prove_with_blinding(v: u64, bits: usize, rng: &mut ThreadRng) -> (Self, Scalar) {
        let bits = bits.min(64);
        let mut total_blinding = Scalar::from(0u64);
        let mut commitments = Vec::with_capacity(bits);
        let mut proofs = Vec::with_capacity(bits);

        for i in 0..bits {
            let bit = (v >> i) & 1;
            let a_i = Scalar::random(rng);
            let c_i = Commitment::new_with_blinding(bit, a_i);

            let ring: Vec<Vec<RistrettoPoint>> = (0..2)
                .map(|b| vec![c_i.0 - Scalar::from(b as u64) * pedersen_h()])
                .collect();

            let sig = MLSAGSignature::sign(
                &ring,
                &[a_i],
                bit as usize,
                &format!("bit_{}", i).into_bytes(),
                rng,
            );
            commitments.push(c_i);
            proofs.push(sig);
            total_blinding = total_blinding + a_i * Scalar::from(1u64 << i);
        }

        (
            RangeProof {
                bits,
                commitments,
                proofs,
            },
            total_blinding,
        )
    }

    /// Prove with an exact total blinding, so Pedersen balance can be verified.
    pub fn prove_with_exact_blinding(v: u64, desired: Scalar, bits: usize, rng: &mut ThreadRng) -> Self {
        let bits = bits.min(64);
        if bits == 0 {
            panic!("Range proof needs >= 1 bit");
        }
        let mut commitments = Vec::with_capacity(bits);
        let mut proofs = Vec::with_capacity(bits);
        let mut partial = Scalar::from(0u64);

        for i in 0..bits - 1 {
            let bit = (v >> i) & 1;
            let a_i = Scalar::random(rng);
            let c_i = Commitment::new_with_blinding(bit, a_i);
            let ring: Vec<Vec<RistrettoPoint>> = (0..2)
                .map(|b| vec![c_i.0 - Scalar::from(b as u64) * pedersen_h()])
                .collect();
            let sig = MLSAGSignature::sign(&ring, &[a_i], bit as usize, &format!("bit_{}", i).into_bytes(), rng);
            commitments.push(c_i);
            proofs.push(sig);
            partial = partial + a_i * Scalar::from(1u64 << i);
        }

        // Last bit: compute a_last so total = desired
        let last_i = bits - 1;
        let last_bit = (v >> last_i) & 1;
        let scale = Scalar::from(1u64 << last_i);
        let a_last = (desired - partial) * scale.invert();
        let c_last = Commitment::new_with_blinding(last_bit, a_last);
        let ring_last: Vec<Vec<RistrettoPoint>> = (0..2)
            .map(|b| vec![c_last.0 - Scalar::from(b as u64) * pedersen_h()])
            .collect();
        let sig_last = MLSAGSignature::sign(&ring_last, &[a_last], last_bit as usize, &format!("bit_{}", last_i).into_bytes(), rng);
        commitments.push(c_last);
        proofs.push(sig_last);

        RangeProof { bits, commitments, proofs }
    }

    /// Verify the range proof against a commitment.
    pub fn verify(&self, commitment: &Commitment) -> bool {
        // Validate proof structure (must mirror prove constraints)
        if self.commitments.len() > 64 {
            return false;
        }
        if self.commitments.len() != self.proofs.len() {
            return false;
        }
        if self.commitments.len() != self.bits {
            return false;
        }

        // Reconstruct commitment from bits
        let mut sum = RistrettoPoint::identity();
        for (i, c_i) in self.commitments.iter().enumerate() {
            sum = sum + Scalar::from(1u64 << i) * c_i.0;
        }
        if sum != commitment.0 {
            return false;
        }

        // Verify each bit proof
        for (i, sig) in self.proofs.iter().enumerate() {
            let ring: Vec<Vec<RistrettoPoint>> = (0..2)
                .map(|b| vec![self.commitments[i].0 - Scalar::from(b as u64) * pedersen_h()])
                .collect();
            if !sig.verify(&ring, &format!("bit_{}", i).into_bytes()) {
                return false;
            }
        }
        true
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rand::thread_rng;

    #[test]
    fn test_stealth_address_roundtrip() {
        let mut rng = thread_rng();
        let (addr, key) = StealthAddress::generate(&mut rng);
        let (dest, _r) = addr.derive_destination(&mut rng);
        let recovered = recover_one_time_key(&key.view, &key.spend, &dest.ephemeral);
        assert_eq!(recovered * ring_g(), dest.dest, "Stealth recovery failed");
    }

    #[test]
    fn test_pedersen_commitment() {
        let mut rng = thread_rng();
        let (comm, blinding) = Commitment::new(42, &mut rng);
        assert!(comm.verify(42, blinding));
        assert!(!comm.verify(43, blinding));
    }

    #[test]
    fn test_pedersen_homomorphic() {
        let mut rng = thread_rng();
        let (c1, a1) = Commitment::new(10, &mut rng);
        let (c2, a2) = Commitment::new(20, &mut rng);
        let c3 = c1.add(&c2);
        assert!(c3.verify(30, a1 + a2));
    }

    #[test]
    fn test_mlsag_roundtrip() {
        let mut rng = thread_rng();
        let ring_size = 11;
        let mut ring = Vec::with_capacity(ring_size);
        let mut secrets = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            let sk = Scalar::random(&mut rng);
            ring.push(vec![sk * ring_g()]);
            secrets.push(sk);
        }
        let sig = MLSAGSignature::sign(&ring, &[secrets[3]], 3, b"test", &mut rng);
        assert!(sig.verify(&ring, b"test"));
    }

    #[test]
    fn test_mlsag_wrong_msg_fails() {
        let mut rng = thread_rng();
        let ring_size = 11;
        let mut ring = Vec::with_capacity(ring_size);
        let mut secrets = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            let sk = Scalar::random(&mut rng);
            ring.push(vec![sk * ring_g()]);
            secrets.push(sk);
        }
        let sig = MLSAGSignature::sign(&ring, &[secrets[3]], 3, b"msg1", &mut rng);
        assert!(!sig.verify(&ring, b"msg2"));
    }

    #[test]
    fn test_mlsag_multi_layer() {
        let mut rng = thread_rng();
        let ring_size = 7;
        let n_layers = 2;
        let mut ring = Vec::with_capacity(ring_size);
        let mut all_sec = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            let mut keys = Vec::with_capacity(n_layers);
            let mut secs = Vec::with_capacity(n_layers);
            for _ in 0..n_layers {
                let sk = Scalar::random(&mut rng);
                keys.push(sk * ring_g());
                secs.push(sk);
            }
            ring.push(keys);
            all_sec.push(secs);
        }
        let sig = MLSAGSignature::sign(&ring, &all_sec[3], 3, b"multi-layer", &mut rng);
        assert!(sig.verify(&ring, b"multi-layer"));
    }

    #[test]
    fn test_range_proof() {
        let mut rng = thread_rng();
        let v = 7u64;
        let bits = 8;
        let (proof, total_blinding) = RangeProof::prove_with_blinding(v, bits, &mut rng);
        let mut sum = RistrettoPoint::identity();
        for (i, c_i) in proof.commitments.iter().enumerate() {
            sum = sum + Scalar::from(1u64 << i) * c_i.0;
        }
        assert!(proof.verify(&Commitment(sum)));
        // The commitment opens to v with total_blinding
        assert!(Commitment(sum).verify(v, total_blinding));
    }

    #[test]
    fn test_range_proof_verify_oversized_commitments() {
        let mut rng = thread_rng();
        let (mut proof, _) = RangeProof::prove_with_blinding(42, 8, &mut rng);
        // Pad commitments to 100 entries (simulating malicious proof)
        // This should not panic; verify should return false
        let dummy = Commitment::new(0, &mut rng).0;
        while proof.commitments.len() < 100 {
            proof.commitments.push(dummy);
            proof.proofs.push(proof.proofs[0].clone());
        }
        let (comm, _) = Commitment::new(42, &mut rng);
        assert!(!proof.verify(&comm), "oversized proof should be rejected");
    }

    #[test]
    fn test_range_proof_large_bits_clamped() {
        let mut rng = thread_rng();
        let v = 42u64;
        let (proof, _) = RangeProof::prove_with_blinding(v, 999, &mut rng);
        assert!(proof.bits <= 64, "bits should be clamped to 64");
        let mut sum = RistrettoPoint::identity();
        for (i, c_i) in proof.commitments.iter().enumerate() {
            sum = sum + Scalar::from(1u64 << i) * c_i.0;
        }
        assert!(proof.verify(&Commitment(sum)));
    }

    #[test]
    fn test_mlsag_min_ring_size() {
        let mut rng = thread_rng();
        // ring of 1 should be rejected
        let ring = vec![vec![Scalar::random(&mut rng) * ring_g()]];
        let sig = MLSAGSignature::sign(&ring, &[Scalar::random(&mut rng)], 0, b"test", &mut rng);
        assert!(
            !sig.verify(&ring, b"test"),
            "ring size 1 should be rejected"
        );
    }

    #[test]
    fn test_mlsag_tampered_c0() {
        let mut rng = thread_rng();
        let ring: Vec<Vec<RistrettoPoint>> = (0..11)
            .map(|_| vec![Scalar::random(&mut rng) * ring_g()])
            .collect();
        let secrets: Vec<Scalar> = (0..11).map(|_| Scalar::random(&mut rng)).collect();
        let mut sig = MLSAGSignature::sign(&ring, &[secrets[3]], 3, b"test", &mut rng);
        sig.c0 = sig.c0 + Scalar::from(1u64); // tamper
        assert!(!sig.verify(&ring, b"test"), "tampered c0 should fail");
    }

    #[test]
    fn test_mlsag_tampered_response() {
        let mut rng = thread_rng();
        let ring: Vec<Vec<RistrettoPoint>> = (0..11)
            .map(|_| vec![Scalar::random(&mut rng) * ring_g()])
            .collect();
        let secrets: Vec<Scalar> = (0..11).map(|_| Scalar::random(&mut rng)).collect();
        let mut sig = MLSAGSignature::sign(&ring, &[secrets[3]], 3, b"test", &mut rng);
        if !sig.responses.is_empty() && !sig.responses[0].is_empty() {
            sig.responses[0][0] = sig.responses[0][0] + Scalar::from(1u64); // tamper
        }
        assert!(!sig.verify(&ring, b"test"), "tampered response should fail");
    }

    #[test]
    fn test_mlsag_malformed_sizes() {
        let mut rng = thread_rng();
        let ring: Vec<Vec<RistrettoPoint>> = (0..11)
            .map(|_| vec![Scalar::random(&mut rng) * ring_g()])
            .collect();
        let secrets: Vec<Scalar> = (0..11).map(|_| Scalar::random(&mut rng)).collect();
        let sig = MLSAGSignature::sign(&ring, &[secrets[3]], 3, b"test", &mut rng);
        // Wrong ring size should be rejected
        let short_ring: Vec<Vec<RistrettoPoint>> = (0..5)
            .map(|_| vec![Scalar::random(&mut rng) * ring_g()])
            .collect();
        assert!(!sig.verify(&short_ring, b"test"), "short ring should fail");
        // Wrong layer count should be rejected
        let wrong_layers: Vec<Vec<RistrettoPoint>> = (0..11)
            .map(|_| {
                vec![
                    Scalar::random(&mut rng) * ring_g(),
                    Scalar::random(&mut rng) * ring_g(),
                ]
            })
            .collect();
        assert!(
            !sig.verify(&wrong_layers, b"test"),
            "wrong layers should fail"
        );
    }

    #[test]
    fn test_mlsag_wrong_ring_fails() {
        let mut rng = thread_rng();
        let rs = 11;
        let ring: Vec<Vec<RistrettoPoint>> = (0..rs)
            .map(|_| vec![Scalar::random(&mut rng) * ring_g()])
            .collect();
        let secrets: Vec<Scalar> = (0..rs).map(|_| Scalar::random(&mut rng)).collect();
        let sig = MLSAGSignature::sign(&ring, &[secrets[2]], 2, b"test", &mut rng);
        let wrong: Vec<Vec<RistrettoPoint>> = (0..rs)
            .map(|_| vec![Scalar::random(&mut rng) * ring_g()])
            .collect();
        assert!(!sig.verify(&wrong, b"test"));
    }

    #[test]
    fn test_hash_to_scalar_deterministic() {
        let h1 = hash_to_scalar(b"hello");
        let h2 = hash_to_scalar(b"hello");
        assert_eq!(h1, h2);
        assert_ne!(h1, hash_to_scalar(b"world"));
    }

    #[test]
    fn test_hash_to_point_valid() {
        let p = hash_to_point(b"test");
        assert!(p.compress().decompress().is_some());
    }
}
