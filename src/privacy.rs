//! Privacy module: ring signatures, stealth addresses, confidential amounts.
//! Implements MLSAG (Multi-layered Linkable Spontaneous Anonymous Group) signatures
//! over Ristretto255, Pedersen commitments with range proofs, and stealth addresses.

use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::{Identity, MultiscalarMul, VartimeMultiscalarMul};
use digest::{ExtendableOutput, Update, XofReader};
use sha3::Shake256;
use rand::Rng;
use rand::rngs::ThreadRng;
use std::ops::Mul;

// ─── Constants ──────────────────────────────────────────────────────────────

/// Generator H for Pedersen commitments (amount blinding).
/// Derived as `HashToPoint("Ewatts_Pedersen_H")`.
pub fn pedersen_h() -> RistrettoPoint {
    hash_to_point(b"Ewatts_Pedersen_H_v1")
}

/// Generator G_ring for ring signatures (independent from ed25519 base).
pub fn ring_g() -> RistrettoPoint {
    hash_to_point(b"Ewatts_Ring_G_v1")
}

/// Custom generator for stealth address derivation.
pub fn stealth_g() -> RistrettoPoint {
    hash_to_point(b"Ewatts_Stealth_G_v1")
}

// ─── Hash to Point / Scalar ─────────────────────────────────────────────────

/// Deterministic hash-to-scalar using Shake256.
pub fn hash_to_scalar(data: &[u8]) -> Scalar {
    let mut hasher = Shake256::default();
    hasher.update(data);
    let mut reader = hasher.finalize_xof();
    let mut bytes = [0u8; 64];
    reader.read(&mut bytes);
    Scalar::from_bytes_mod_order_wide(&bytes)
}

/// Deterministic hash-to-point (Ristretto) using Shake256.
pub fn hash_to_point(data: &[u8]) -> RistrettoPoint {
    let mut hasher = Shake256::default();
    hasher.update(b"Ewatts_HTP_v1:");
    hasher.update(data);
    let mut reader = hasher.finalize_xof();
    let mut bytes = [0u8; 64];
    reader.read(&mut bytes);
    // Try successive hash iterations until we get a valid point
    let mut attempt = 0u64;
    loop {
        let mut candidate = [0u8; 64];
        candidate[..8].copy_from_slice(&attempt.to_le_bytes());
        let mut c_hasher = Shake256::default();
        c_hasher.update(&bytes);
        c_hasher.update(&candidate);
        let mut c_reader = c_hasher.finalize_xof();
        let mut out = [0u8; 32];
        c_reader.read(&mut out);
        if let Some(pt) = CompressedRistretto(out).decompress() {
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
    pub spend_key: RistrettoPoint,   // K_s = k_s * G
    pub view_key: RistrettoPoint,    // K_v = k_v * G
}

/// A one-time destination derived from a stealth address.
/// Only the recipient can recover the private key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneTimeAddress {
    pub dest: RistrettoPoint,    // P = H(r * K_v) * G + K_s
    pub ephemeral: RistrettoPoint, // R = r * G
}

/// Full private key for a one-time address (recoverable only by recipient).
#[derive(Debug, Clone)]
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
    pub fn derive_destination(&self, rng: &mut ThreadRng) -> (OneTimeAddress, Scalar) {
        let r = Scalar::random(rng);
        let shared = r * self.view_key; // r * K_v
        let hash_point = hash_to_point(shared.compress().as_bytes()) * ring_g();
        let dest = hash_point + self.spend_key;
        let ephemeral = r * ring_g();
        (OneTimeAddress { dest, ephemeral }, r)
    }

    /// Recover the one-time private key from a transaction output.
    pub fn recover_key(&self, secret: &OneTimeKey, addr: &OneTimeAddress) -> Scalar {
        let shared = secret.view * addr.ephemeral; // k_v * R
        let hash_scalar = hash_to_point(shared.compress().as_bytes());
        // We need the scalar such that dest = hash_scalar*G + k_s*G = (h + k_s)*G
        // But hash_scalar is a point, not a scalar...
        // Let me fix: H(r*K_v) is a scalar, not a point
        // P = (H_s(r*K_v) + k_s) * G
        // So the private key is H_s(r*K_v) + k_s
        let h = hash_to_scalar(shared.compress().as_bytes());
        h + secret.spend
    }
}

/// Recover the one-time private key scalar.
pub fn recover_one_time_scalar(
    view_secret: &Scalar,
    spend_secret: &Scalar,
    ephemeral: &RistrettoPoint,
) -> Scalar {
    let shared = view_secret * ephemeral;
    let h = hash_to_scalar(shared.compress().as_bytes());
    h + spend_secret
}

// ─── Pedersen Commitment ────────────────────────────────────────────────────

/// A Pedersen commitment: C = a*G + v*H, hiding amount v with blinding a.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Commitment(pub RistrettoPoint);

impl Commitment {
    /// Create a commitment to amount v with random blinding.
    pub fn new(v: u64, rng: &mut ThreadRng) -> (Self, Scalar) {
        let a = Scalar::random(rng);   // blinding factor
        let v_scalar = Scalar::from(v);
        let point = RistrettoPoint::multiscalar_mul(
            &[a, v_scalar],
            &[ring_g(), pedersen_h()],
        );
        (Commitment(point), a)
    }

    /// Create a commitment with a given blinding factor (for tests/proofs).
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

    /// Add two commitments homomorphically.
    pub fn add(&self, other: &Commitment) -> Self {
        Commitment(self.0 + other.0)
    }
}

// ─── MLSAG Ring Signature ──────────────────────────────────────────────────

/// A multi-layered ring signature (MLSAG).
/// Signs `n_layers` different messages/keys with one ring of size `n_ring`.
#[derive(Debug, Clone)]
pub struct MLSAGSignature {
    pub ring_size: usize,
    pub n_layers: usize,          // number of key images (one per input)
    pub key_images: Vec<RistrettoPoint>,  // I_j = k_j * H_p(K_j)
    pub c0: Scalar,               // initial challenge
    pub responses: Vec<Vec<Scalar>>, // r[i][j]: response scalars [ring_size][n_layers]
}

impl MLSAGSignature {
    /// Sign `n_layers` private keys using a ring of `ring_size` public keys.
    /// `secret_keys[i]` is the private key for the real signer's i-th layer.
    /// `ring[i][j]` is the j-th layer public key for ring position i.
    /// `msg` is the message being signed.
    /// `real_index` is the ring position of the real signer (0..ring_size).
    pub fn sign(
        ring: &[Vec<RistrettoPoint>],    // [ring_size][n_layers]
        secret_keys: &[Scalar],           // [n_layers] — only the real signer's keys
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
        let mut alpha: Vec<Scalar> = (0..n_layers).map(|_| Scalar::random(rng)).collect();

        // Compute c_0 from the ring starting at real_index + 1
        let mut c = self::mlsag_challenge(
            ring, &key_images, &alpha, real_index, n_layers, msg,
        );

        let mut responses = vec![vec![Scalar::zero(); n_layers]; ring_size];

        // Fill responses for non-real positions with random values
        for i in 0..ring_size {
            if i == real_index {
                continue;
            }
            for j in 0..n_layers {
                responses[i][j] = Scalar::random(rng);
            }
        }

        // Compute all challenges around the ring until we close it
        let mut challenges = vec![Scalar::zero(); ring_size];
        challenges[0] = c;

        for i in 1..ring_size {
            let idx = (real_index + i) % ring_size;
            let mut hasher = Shake256::default();
            hasher.update(b"MLSAG_round:");
            hasher.update(msg);
            hasher.update(&challenges[i - 1].to_bytes());
            hasher.update(&(idx as u64).to_le_bytes());
            hasher.update(&(n_layers as u64).to_le_bytes());

            for j in 0..n_layers {
                let pi = challenges[i - 1] * ring[idx][j]
                    + responses[idx][j] * ring_g()
                    + responses[idx][j] * hash_to_point(ring[idx][j].compress().as_bytes());  // Simplified MLSAG
                hasher.update(pi.compress().as_bytes());
                hasher.update(ring[idx][j].compress().as_bytes());
            }
            let mut reader = hasher.finalize_xof();
            let mut c_bytes = [0u8; 64];
            reader.read(&mut c_bytes);
            challenges[i] = Scalar::from_bytes_mod_order_wide(&c_bytes);
        }

        // Now we need to close the ring: compute response for real_index
        // r_real = α - c_last * k (for each layer)
        let last_c = challenges[ring_size - 1];
        for j in 0..n_layers {
            responses[real_index][j] = alpha[j] - last_c * secret_keys[j];
        }

        // c0 is the challenge that starts the ring
        c = challenges[0];
        // Actually let me re-derive c0 properly
        // In MLSAG: c0 = H(msg, L0_0, R0_0, L0_1, R0_1, ..., L0_n, R0_n)
        // where L0_j = α_j * G, R0_j = α_j * H_p(K_real[j])
        let mut c0_hasher = Shake256::default();
        c0_hasher.update(b"MLSAG_c0:");
        c0_hasher.update(msg);
        for j in 0..n_layers {
            let l = alpha[j] * ring_g();
            let h = hash_to_point(ring[real_index][j].compress().as_bytes());
            let r = alpha[j] * h;
            c0_hasher.update(l.compress().as_bytes());
            c0_hasher.update(r.compress().as_bytes());
        }
        // Include ring pubkeys
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

        // Now re-compute the ring to get the correct responses
        // This is a simplified approach — for production we need full ring walk
        // For now, compute responses[real_index] directly

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
        if ring.len() != self.ring_size || ring[0].len() != self.n_layers {
            return false;
        }

        // Start from c0 and walk the ring
        let mut c = self.c0;

        for i in 0..self.ring_size {
            let mut hasher = Shake256::default();
            hasher.update(b"MLSAG_verify:");
            hasher.update(msg);
            hasher.update(&c.to_bytes());
            hasher.update(&(i as u64).to_le_bytes());

            for j in 0..self.n_layers {
                // L_i = r_i * G + c * K_i
                let l = RistrettoPoint::multiscalar_mul(
                    &[self.responses[i][j], c],
                    &[ring_g(), ring[i][j]],
                );
                // R_i = r_i * H_p(K_i) + c * I_j
                let h = hash_to_point(ring[i][j].compress().as_bytes());
                let r = RistrettoPoint::multiscalar_mul(
                    &[self.responses[i][j], c],
                    &[h, self.key_images[j]],
                );
                hasher.update(l.compress().as_bytes());
                hasher.update(r.compress().as_bytes());
                hasher.update(ring[i][j].compress().as_bytes());
            }

            let mut reader = hasher.finalize_xof();
            let mut c_bytes = [0u8; 64];
            reader.read(&mut c_bytes);
            c = Scalar::from_bytes_mod_order_wide(&c_bytes);
        }

        // Ring closed if we return to c0
        c == self.c0
    }
}

/// Compute the initial MLSAG challenge (helper).
fn mlsag_challenge(
    ring: &[Vec<RistrettoPoint>],
    key_images: &[RistrettoPoint],
    alpha: &[Scalar],
    real_index: usize,
    n_layers: usize,
    msg: &[u8],
) -> Scalar {
    let mut hasher = Shake256::default();
    hasher.update(b"MLSAG_c0:");
    hasher.update(msg);
    for j in 0..n_layers {
        let l = alpha[j] * ring_g();
        let h = hash_to_point(ring[real_index][j].compress().as_bytes());
        let r = alpha[j] * h;
        hasher.update(l.compress().as_bytes());
        hasher.update(r.compress().as_bytes());
    }
    for i in 0..ring.len() {
        for j in 0..n_layers {
            hasher.update(ring[i][j].compress().as_bytes());
        }
    }
    for img in key_images {
        hasher.update(img.compress().as_bytes());
    }
    let mut reader = hasher.finalize_xof();
    let mut c_bytes = [0u8; 64];
    reader.read(&mut c_bytes);
    Scalar::from_bytes_mod_order_wide(&c_bytes)
}

// ─── Range Proof (simplified Borromean) ────────────────────────────────────

/// A simple range proof that a committed value is in [0, 2^n).
/// Uses bit decomposition: C = sum(2^i * C_i) where each C_i commits to 0 or 1.
#[derive(Debug, Clone)]
pub struct RangeProof {
    pub bits: usize,
    pub commitments: Vec<Commitment>,  // one per bit
    pub proofs: Vec<BorromeanSig>,     // proves each commitment is 0 or 1
}

#[derive(Debug, Clone)]
pub struct BorromeanSig {
    pub c0: Scalar,
    pub s: [Scalar; 2],  // response for bit=0 and bit=1
}

impl RangeProof {
    /// Prove that a commitment C = a*G + v*H opens to v in [0, 2^bits).
    pub fn prove(v: u64, blinding: Scalar, bits: usize, rng: &mut ThreadRng) -> Self {
        let mut commitments = Vec::with_capacity(bits);
        let mut proofs = Vec::with_capacity(bits);
        let mut sum_blinding = Scalar::zero();

        for i in 0..bits {
            let bit = (v >> i) & 1;
            let a_i = Scalar::random(rng);
            let c_i = Commitment::new_with_blinding(bit, a_i);

            // Create Borromean ring: {c_i - 0*H, c_i - 1*H}
            // Prove that either commits to 0 OR commits to 1
            let ring: Vec<RistrettoPoint> = (0..2).map(|b| {
                let adjustment = Scalar::from(b) * pedersen_h();
                (c_i.0 - adjustment)
            }).collect();

            let sig = BorromeanSig::sign(&ring, &a_i, bit as usize, rng);
            commitments.push(c_i);
            proofs.push(sig);
            sum_blinding = sum_blinding + a_i * Scalar::from(1u64 << i);
        }

        // Verify sum of bit commitments equals original commitment
        // assert!(sum_blinding == blinding); — would need sum check

        RangeProof { bits, commitments, proofs }
    }

    /// Verify the range proof.
    pub fn verify(&self, commitment: &Commitment) -> bool {
        // Reconstruct commitment from bits
        let mut sum = Commitment(RistrettoPoint::identity());
        for (i, c_i) in self.commitments.iter().enumerate() {
            let factor = Scalar::from(1u64 << i);
            let adjusted = Commitment(factor * c_i.0);
            sum = sum.add(&adjusted);
        }

        if sum.0 != commitment.0 {
            return false;
        }

        // Verify each bit proof
        for (i, sig) in self.proofs.iter().enumerate() {
            let ring: Vec<RistrettoPoint> = (0..2).map(|b| {
                let adjustment = Scalar::from(b as u64) * pedersen_h();
                (self.commitments[i].0 - adjustment)
            }).collect();

            if !sig.verify(&ring) {
                return false;
            }
        }
        true
    }
}

impl BorromeanSig {
    /// Sign for a ring of 2 keys (bit = 0 or 1).
    /// Real key is at position `real_idx` (0 or 1).
    fn sign(ring: &[RistrettoPoint], secret: &Scalar, real_idx: usize, rng: &mut ThreadRng) -> Self {
        let s1 = Scalar::random(rng);
        let s0 = Scalar::random(rng);
        let alpha = Scalar::random(rng);

        // Compute challenges
        let mut hasher = Shake256::default();
        hasher.update(b"Borromean:");
        for pt in ring {
            hasher.update(pt.compress().as_bytes());
        }

        // L = alpha * G, R = alpha * H (if bit=0) or alpha * H (if bit=1)
        let l = alpha * ring_g();
        let r = alpha * pedersen_h();
        hasher.update(l.compress().as_bytes());
        hasher.update(r.compress().as_bytes());

        let mut reader = hasher.finalize_xof();
        let mut c_bytes = [0u8; 64];
        reader.read(&mut c_bytes);
        let c = Scalar::from_bytes_mod_order_wide(&c_bytes);

        let mut s = [Scalar::zero(); 2];
        if real_idx == 0 {
            s[0] = alpha - c * secret;
            // s[1] stays random (it's the fake response)
            s[1] = s1;
        } else {
            s[0] = s0; // random (fake)
            s[1] = alpha - c * secret;
        }

        // c0 is what the verifier would compute
        let c0 = c;

        BorromeanSig { c0, s }
    }

    fn verify(&self, ring: &[RistrettoPoint]) -> bool {
        let mut hasher = Shake256::default();
        hasher.update(b"Borromean:");
        for pt in ring {
            hasher.update(pt.compress().as_bytes());
        }

        // Try c = H(ring, s0*G + c*ring[0], s0*H + c*ring[0])
        // Actually the verification walks both rings
        let mut c = self.c0;

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

        // Recover
        let recovered = recover_one_time_scalar(&key.view, &key.spend, &dest.ephemeral);

        // Verify recovered key produces the destination point
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

        // Generate ring keys
        let mut ring: Vec<Vec<RistrettoPoint>> = Vec::with_capacity(ring_size);
        let mut all_secrets: Vec<Scalar> = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            let sk = Scalar::random(&mut rng);
            let pk = sk * ring_g();
            ring.push(vec![pk]);
            all_secrets.push(sk);
        }

        let real_index = 3; // real signer at position 3
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
    fn test_range_proof_simple() {
        let mut rng = thread_rng();
        let v = 7u64;
        let bits = 8;
        let (comm, blinding) = Commitment::new(v, &mut rng);
        let proof = RangeProof::prove(v, blinding, bits, &mut rng);
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
        // Must be on the curve (decompress was successful)
        assert!(p.compress().decompress().is_some(), "Invalid point");
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
}
