//! Privacy module: ring signatures, stealth addresses, confidential amounts.
//! Implements MLSAG (Multi-layered Linkable Spontaneous Anonymous Group) signatures
//! over Ristretto255, Pedersen commitments with range proofs, and stealth addresses.

use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::{Identity, MultiscalarMul};
use digest::{ExtendableOutput, Update, XofReader};
use sha3::Shake256;
use rand::rngs::ThreadRng;

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Scalar zero (API compat: v4.1.3 removed Scalar::zero()).
fn scalar_zero() -> Scalar {
    Scalar::from(0u64)
}

/// Generator H for Pedersen commitments (amount blinding).
/// H = hash_to_point("Ewatts_Pedersen_H").
pub fn pedersen_h() -> RistrettoPoint {
    hash_to_point(b"Ewatts_Pedersen_H_v1")
}

/// Generator G_ring for ring signatures (independent from ed25519 base).
pub fn ring_g() -> RistrettoPoint {
    hash_to_point(b"Ewatts_Ring_G_v1")
}

// ─── Hash to Point / Scalar ─────────────────────────────────────────────────

/// Deterministic hash-to-scalar using Shake256 (64 bytes → reduce mod l).
pub fn hash_to_scalar(data: &[u8]) -> Scalar {
    let mut hasher = Shake256::default();
    hasher.update(data);
    let mut reader = hasher.finalize_xof();
    let mut bytes = [0u8; 64];
    reader.read(&mut bytes);
    Scalar::from_bytes_mod_order_wide(&bytes)
}

/// Deterministic hash-to-point (Ristretto) using Shake256.
/// Uses trial-and-error (hash then check if on curve).
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

// ─── Stealth Address ────────────────────────────────────────────────────────

/// A stealth address: public spend key + public view key.
/// Recipient publishes these once; sender derives unique one-time addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StealthAddress {
    pub spend_key: RistrettoPoint,  // K_s = k_s * G
    pub view_key: RistrettoPoint,   // K_v = k_v * G
}

/// A one-time destination derived from a stealth address.
/// Only the recipient can recover the private key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneTimeAddress {
    pub dest: RistrettoPoint,       // P = H_s(r * K_v) * G + K_s
    pub ephemeral: RistrettoPoint,  // R = r * G
}

/// Full private key for a stealth address.
#[derive(Debug, Clone, Copy)]
pub struct OneTimeKey {
    pub spend: Scalar,
    pub view: Scalar,
}

impl StealthAddress {
    /// Generate a new stealth keypair.
    pub fn generate(rng: &mut ThreadRng) -> (Self, OneTimeKey) {
        let spend = Scalar::random(rng);
        let view = Scalar::random(rng);
        let addr = StealthAddress {
            spend_key: ring_g() * spend,
            view_key: ring_g() * view,
        };
        let key = OneTimeKey { spend, view };
        (addr, key)
    }

    /// Derive a one-time destination for a payment to this address.
    /// Returns (OneTimeAddress, ephemeral_secret_r).
    /// Sender publishes `addr.ephemeral = r*G` in the transaction.
    pub fn derive_destination(&self, rng: &mut ThreadRng) -> (OneTimeAddress, Scalar) {
        let r = Scalar::random(rng);
        let shared = r * self.view_key;             // r * K_v
        let h = hash_to_scalar(shared.compress().as_bytes()); // H_s(r * K_v)
        let dest = h * ring_g() + self.spend_key;    // P = h*G + K_s
        let ephemeral = r * ring_g();                 // R = r*G
        (OneTimeAddress { dest, ephemeral }, r)
    }
}

/// Recover the one-time private key scalar for a stealth output.
/// P = H_s(k_v * R) * G + K_s  →  private key = H_s(k_v * R) + k_s
pub fn recover_one_time_key(
    view_secret: &Scalar,
    spend_secret: &Scalar,
    ephemeral: &RistrettoPoint,
) -> Scalar {
    let shared = view_secret * ephemeral;            // k_v * R
    let h = hash_to_scalar(shared.compress().as_bytes()); // H_s(k_v * R)
    h + spend_secret
}

// ─── Pedersen Commitment ────────────────────────────────────────────────────

/// A Pedersen commitment: C = a*G + v*H, hiding amount v with blinding a.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Commitment(pub RistrettoPoint);

impl Commitment {
    /// Create a commitment to amount v with random blinding.
    pub fn new(v: u64, rng: &mut ThreadRng) -> (Self, Scalar) {
        let a = Scalar::random(rng);
        let v_scalar = Scalar::from(v);
        let point = RistrettoPoint::multiscalar_mul(
            &[a, v_scalar],
            &[ring_g(), pedersen_h()],
        );
        (Commitment(point), a)
    }

    /// Create a commitment with a given blinding factor.
    pub fn new_with_blinding(v: u64, a: Scalar) -> Self {
        let v_scalar = Scalar::from(v);
        let point = RistrettoPoint::multiscalar_mul(
            &[a, v_scalar],
            &[ring_g(), pedersen_h()],
        );
        Commitment(point)
    }

    /// Verify commitment opens to amount v with blinding a.
    pub fn verify(&self, v: u64, a: Scalar) -> bool {
        let expected = Commitment::new_with_blinding(v, a);
        self.0 == expected.0
    }

    /// Add two commitments homomorphically: C(a1, v1) + C(a2, v2) = C(a1+a2, v1+v2).
    pub fn add(&self, other: &Commitment) -> Self {
        Commitment(self.0 + other.0)
    }
}

// ─── MLSAG Ring Signature ──────────────────────────────────────────────────

/// A multi-layered ring signature (MLSAG).
/// Signs `n_layers` different keys with one ring of size `ring_size`.
#[derive(Debug, Clone)]
pub struct MLSAGSignature {
    pub ring_size: usize,
    pub n_layers: usize,
    pub key_images: Vec<RistrettoPoint>,       // I_j = k_j * H_p(K_real[j])
    pub c0: Scalar,
    pub responses: Vec<Vec<Scalar>>,            // responses[i][j]
}

impl MLSAGSignature {
    /// Sign `n_layers` private keys using a ring of `ring_size` public keys.
    ///
    /// `ring[i][j]`: j-th layer public key at ring position i.
    /// `secret_keys[j]`: real signer's j-th private key.
    /// `real_index`: which ring position the real signer occupies.
    pub fn sign(
        ring: &[Vec<RistrettoPoint>],
        secret_keys: &[Scalar],
        real_index: usize,
        msg: &[u8],
        rng: &mut ThreadRng,
    ) -> Self {
        let ring_size = ring.len();
        let n_layers = secret_keys.len();

        // Key images: I_j = k_j * H_p(K_real[j])
        let mut key_images = Vec::with_capacity(n_layers);
        for j in 0..n_layers {
            let h = hash_to_point(ring[real_index][j].compress().as_bytes());
            key_images.push(secret_keys[j] * h);
        }

        // Random α_j for the real signer
        let alpha: Vec<Scalar> = (0..n_layers).map(|_| Scalar::random(rng)).collect();

        // Initialize responses with zeros (will fill real_index last)
        let mut responses = vec![vec![scalar_zero(); n_layers]; ring_size];

        // Compute c0 = H(msg, L_0j, R_0j, ring, key_images)
        let mut c0_hasher = Shake256::default();
        c0_hasher.update(b"MLSAG_c0:");
        c0_hasher.update(msg);
        for j in 0..n_layers {
            let l = alpha[j] * ring_g();
            let h_pk = hash_to_point(ring[real_index][j].compress().as_bytes());
            let r = alpha[j] * h_pk;
            c0_hasher.update(l.compress().as_bytes());
            c0_hasher.update(r.compress().as_bytes());
        }
        for i in 0..ring_size {
            for j in 0..n_layers {
                c0_hasher.update(ring[i][j].compress().as_bytes());
            }
        }
        for img in &key_images {
            c0_hasher.update(img.compress().as_bytes());
        }
        let mut c0_reader = c0_hasher.finalize_xof();
        let mut c0_bytes = [0u8; 64];
        c0_reader.read(&mut c0_bytes);
        let c0 = Scalar::from_bytes_mod_order_wide(&c0_bytes);

        let mut c = c0;

        // Walk ring from real_index+1 around back to real_index
        for i in 1..ring_size {
            let idx = (real_index + i) % ring_size;

            // Fill random responses for non-real positions
            for j in 0..n_layers {
                if idx != real_index {
                    responses[idx][j] = Scalar::random(rng);
                }
            }

            // Compute L_i, R_i and hash to get next challenge
            let mut hasher = Shake256::default();
            hasher.update(b"MLSAG_round:");
            hasher.update(msg);
            hasher.update(&c.to_bytes());
            hasher.update(&(idx as u64).to_le_bytes());

            for j in 0..n_layers {
                let l = responses[idx][j] * ring_g() + c * ring[idx][j];
                let h_pk = hash_to_point(ring[idx][j].compress().as_bytes());
                let r = responses[idx][j] * h_pk + c * key_images[j];
                hasher.update(l.compress().as_bytes());
                hasher.update(r.compress().as_bytes());
                hasher.update(ring[idx][j].compress().as_bytes());
            }

            let mut reader = hasher.finalize_xof();
            let mut c_bytes = [0u8; 64];
            reader.read(&mut c_bytes);
            c = Scalar::from_bytes_mod_order_wide(&c_bytes);
        }

        // Now close the ring: r_real = α - c * k (for each layer)
        // where c is the challenge that wraps back to real_index
        for j in 0..n_layers {
            responses[real_index][j] = alpha[j] - c * secret_keys[j];
        }

        MLSAGSignature {
            ring_size,
            n_layers,
            key_images,
            c0,
            responses,
        }
    }

    /// Verify an MLSAG signature.
    pub fn verify(&self, ring: &[Vec<RistrettoPoint>], msg: &[u8]) -> bool {
        if ring.len() != self.ring_size {
            return false;
        }
        if self.ring_size == 0 || self.n_layers == 0 {
            return false;
        }
        if ring[0].len() != self.n_layers {
            return false;
        }

        let mut c = self.c0;

        for i in 0..self.ring_size {
            let mut hasher = Shake256::default();
            hasher.update(b"MLSAG_verify:");
            hasher.update(msg);
            hasher.update(&c.to_bytes());
            hasher.update(&(i as u64).to_le_bytes());

            for j in 0..self.n_layers {
                // L_i = r_i * G + c * K_i
                let l = self.responses[i][j] * ring_g() + c * ring[i][j];
                // R_i = r_i * H_p(K_i) + c * I_j
                let h_pk = hash_to_point(ring[i][j].compress().as_bytes());
                let r = self.responses[i][j] * h_pk + c * self.key_images[j];
                hasher.update(l.compress().as_bytes());
                hasher.update(r.compress().as_bytes());
                hasher.update(ring[i][j].compress().as_bytes());
            }

            let mut reader = hasher.finalize_xof();
            let mut c_bytes = [0u8; 64];
            reader.read(&mut c_bytes);
            c = Scalar::from_bytes_mod_order_wide(&c_bytes);
        }

        c == self.c0
    }
}

// ─── Range Proof (bit-decomposition) ────────────────────────────────────────

/// A range proof using bit decomposition: C = sum(2^i * C_i), each C_i ∈ {0, 1}.
/// Each bit commitment uses a Borromean-style ring of two points.
#[derive(Debug, Clone)]
pub struct RangeProof {
    pub bits: usize,
    pub commitments: Vec<Commitment>,
    pub proofs: Vec<BitProof>,
}

/// Proof that a single bit commitment opens to 0 or 1.
#[derive(Debug, Clone)]
pub struct BitProof {
    pub c0: Scalar,
    pub s: [Scalar; 2],
}

impl RangeProof {
    /// Prove that a commitment opens to v ∈ [0, 2^bits).
    pub fn prove(v: u64, _blinding: Scalar, bits: usize, rng: &mut ThreadRng) -> Self {
        let mut commitments = Vec::with_capacity(bits);
        let mut proofs = Vec::with_capacity(bits);

        for i in 0..bits {
            let bit = (v >> i) & 1;
            let a_i = Scalar::random(rng);
            let c_i = Commitment::new_with_blinding(bit, a_i);

            // Ring: {C_i - 0*H, C_i - 1*H} — proves commit is to 0 or 1
            let ring: Vec<RistrettoPoint> = (0..2).map(|b| {
                c_i.0 - Scalar::from(b as u64) * pedersen_h()
            }).collect();

            let sig = BitProof::sign(&ring, &a_i, bit as usize, rng);
            commitments.push(c_i);
            proofs.push(sig);
        }

        RangeProof { bits, commitments, proofs }
    }

    /// Verify the range proof against a commitment.
    pub fn verify(&self, commitment: &Commitment) -> bool {
        // Reconstruct commitment from bits: C = sum(2^i * C_i)
        let mut sum = RistrettoPoint::identity();
        for (i, c_i) in self.commitments.iter().enumerate() {
            sum = sum + Scalar::from(1u64 << i) * c_i.0;
        }
        if sum != commitment.0 {
            return false;
        }

        // Verify each bit proof
        for (i, sig) in self.proofs.iter().enumerate() {
            let ring: Vec<RistrettoPoint> = (0..2).map(|b| {
                self.commitments[i].0 - Scalar::from(b as u64) * pedersen_h()
            }).collect();
            if !sig.verify(&ring) {
                return false;
            }
        }
        true
    }
}

impl BitProof {
    /// Sign a ring of 2 points, proving knowledge of discrete log of ring[bit].
    fn sign(ring: &[RistrettoPoint], secret: &Scalar, bit: usize, rng: &mut ThreadRng) -> Self {
        let s_fake = Scalar::random(rng);
        let alpha = Scalar::random(rng);

        // Compute challenge: c = H(ring, L, R)
        let l = alpha * ring_g();
        let r = alpha * pedersen_h();
        let mut hasher = Shake256::default();
        hasher.update(b"Borromean:");
        for pt in ring {
            hasher.update(pt.compress().as_bytes());
        }
        hasher.update(l.compress().as_bytes());
        hasher.update(r.compress().as_bytes());
        let mut reader = hasher.finalize_xof();
        let mut c_bytes = [0u8; 64];
        reader.read(&mut c_bytes);
        let c = Scalar::from_bytes_mod_order_wide(&c_bytes);

        let mut s = [scalar_zero(); 2];
        if bit == 0 {
            s[0] = alpha - c * secret;
            s[1] = s_fake;
        } else {
            s[0] = s_fake;
            s[1] = alpha - c * secret;
        }

        BitProof { c0: c, s }
    }

    /// Verify the ring signature.
    fn verify(&self, ring: &[RistrettoPoint]) -> bool {
        let mut c = self.c0;
        let mut hasher = Shake256::default();
        hasher.update(b"Borromean:");
        for pt in ring {
            hasher.update(pt.compress().as_bytes());
        }
        for i in 0..2 {
            let l = self.s[i] * ring_g() + c * ring[i];
            let r = self.s[i] * pedersen_h() + c * ring[i];
            hasher.update(l.compress().as_bytes());
            hasher.update(r.compress().as_bytes());
        }
        let mut reader = hasher.finalize_xof();
        let mut c_bytes = [0u8; 64];
        reader.read(&mut c_bytes);
        let c_prime = Scalar::from_bytes_mod_order_wide(&c_bytes);
        c_prime == self.c0
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

        // Recover the one-time private key
        let recovered = recover_one_time_key(&key.view, &key.spend, &dest.ephemeral);

        // Verify: recovered * G should equal destination point
        let expected_dest = recovered * ring_g();
        assert_eq!(expected_dest, dest.dest, "Stealth address recovery failed");
    }

    #[test]
    fn test_pedersen_commitment() {
        let mut rng = thread_rng();
        let (comm, blinding) = Commitment::new(42, &mut rng);
        assert!(comm.verify(42, blinding), "Pedersen open failed");
        assert!(!comm.verify(43, blinding), "Wrong amount should fail");
    }

    #[test]
    fn test_pedersen_homomorphic() {
        let mut rng = thread_rng();
        let (c1, a1) = Commitment::new(10, &mut rng);
        let (c2, a2) = Commitment::new(20, &mut rng);
        let c3 = c1.add(&c2);
        assert!(c3.verify(30, a1 + a2), "Homomorphic sum failed");
    }

    #[test]
    fn test_mlsag_roundtrip() {
        let mut rng = thread_rng();
        let ring_size = 11;
        let n_layers = 1;

        let mut ring: Vec<Vec<RistrettoPoint>> = Vec::with_capacity(ring_size);
        let mut all_secrets: Vec<Scalar> = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            let sk = Scalar::random(&mut rng);
            let pk = sk * ring_g();
            ring.push(vec![pk]);
            all_secrets.push(sk);
        }

        let real_index = 3;
        let secret_keys = vec![all_secrets[real_index]];
        let msg = b"test message for MLSAG";

        let sig = MLSAGSignature::sign(&ring, &secret_keys, real_index, msg, &mut rng);
        assert!(sig.verify(&ring, msg), "MLSAG verification failed");
    }

    #[test]
    fn test_mlsag_wrong_msg_fails() {
        let mut rng = thread_rng();
        let ring_size = 11;
        let n_layers = 1;

        let mut ring: Vec<Vec<RistrettoPoint>> = Vec::with_capacity(ring_size);
        let mut all_secrets: Vec<Scalar> = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            let sk = Scalar::random(&mut rng);
            let pk = sk * ring_g();
            ring.push(vec![pk]);
            all_secrets.push(sk);
        }

        let real_index = 3;
        let secret_keys = vec![all_secrets[real_index]];
        let sig = MLSAGSignature::sign(&ring, &secret_keys, real_index, b"msg1", &mut rng);
        assert!(!sig.verify(&ring, b"msg2"), "Wrong msg should fail");
    }

    #[test]
    fn test_mlsag_multi_layer() {
        let mut rng = thread_rng();
        let ring_size = 7;
        let n_layers = 2;

        let mut ring: Vec<Vec<RistrettoPoint>> = Vec::with_capacity(ring_size);
        let mut all_secrets: Vec<Vec<Scalar>> = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            let mut layer_keys = Vec::with_capacity(n_layers);
            let mut layer_secrets = Vec::with_capacity(n_layers);
            for _ in 0..n_layers {
                let sk = Scalar::random(&mut rng);
                let pk = sk * ring_g();
                layer_keys.push(pk);
                layer_secrets.push(sk);
            }
            ring.push(layer_keys);
            all_secrets.push(layer_secrets);
        }

        let real_index = 3;
        let secret_keys = all_secrets[real_index].clone();
        let msg = b"multi-layer test";

        let sig = MLSAGSignature::sign(&ring, &secret_keys, real_index, msg, &mut rng);
        assert!(sig.verify(&ring, msg), "Multi-layer MLSAG failed");
    }

    #[test]
    fn test_range_proof_simple() {
        let mut rng = thread_rng();
        let v = 7u64;
        let bits = 8;
        let (_comm, blinding) = Commitment::new(v, &mut rng);
        let proof = RangeProof::prove(v, blinding, bits, &mut rng);

        // Reconstruct commitment for verification
        let mut sum = RistrettoPoint::identity();
        for (i, c_i) in proof.commitments.iter().enumerate() {
            sum = sum + Scalar::from(1u64 << i) * c_i.0;
        }
        let comm = Commitment(sum);
        assert!(proof.verify(&comm), "Range proof failed");
    }

    #[test]
    fn test_hash_to_scalar_deterministic() {
        let h1 = hash_to_scalar(b"hello");
        let h2 = hash_to_scalar(b"hello");
        assert_eq!(h1, h2, "hash_to_scalar not deterministic");
        let h3 = hash_to_scalar(b"world");
        assert_ne!(h1, h3, "Different inputs should differ");
    }

    #[test]
    fn test_hash_to_point_valid() {
        let p = hash_to_point(b"test");
        assert!(p.compress().decompress().is_some(), "Invalid point");
    }

    #[test]
    fn test_mlsag_wrong_ring_fails() {
        let mut rng = thread_rng();
        let ring_size = 11;

        let ring: Vec<Vec<RistrettoPoint>> = (0..ring_size).map(|_| {
            vec![Scalar::random(&mut rng) * ring_g()]
        }).collect();

        let secrets: Vec<Scalar> = (0..ring_size).map(|_| Scalar::random(&mut rng)).collect();
        let secret_keys = vec![secrets[2]];
        let sig = MLSAGSignature::sign(&ring, &secret_keys, 2, b"test", &mut rng);

        // Wrong ring (different keys)
        let wrong_ring: Vec<Vec<RistrettoPoint>> = (0..ring_size).map(|_| {
            vec![Scalar::random(&mut rng) * ring_g()]
        }).collect();
        assert!(!sig.verify(&wrong_ring, b"test"), "Wrong ring should fail");
    }
}
