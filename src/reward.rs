use crate::constants;
use crate::commitment::{self, Commitment};

/// Convert Ewatt (f64) to base units with rounding.
fn ewatt_to_units(ewatt: f64) -> u64 {
    (ewatt * constants::UNITS_PER_EWATT as f64).round() as u64
}

pub fn compute_emission_rate(total_effective_aops: f64, historical_avg_aops: f64) -> f64 {
    if historical_avg_aops <= 0.0 { return constants::BASE_EMISSION; }
    let rate = constants::BASE_EMISSION * total_effective_aops / historical_avg_aops;
    rate.clamp(constants::BASE_EMISSION * constants::EMISSION_FLOOR_MULTIPLIER,
               constants::BASE_EMISSION * constants::EMISSION_CEILING_MULTIPLIER)
}

/// Apply ramp-up cap: no single miner receives >80% of reward during first 10,000 blocks.
/// Excess goes to coinbase_burn.
/// Input values are in Ewatt (f64), output burn is in Ewatt (f64) for internal chaining.
pub fn apply_ramp_up_cap(block_number: u64, rewards: &mut Vec<(Vec<u8>, f64)>) -> f64 {
    if block_number >= constants::RAMP_UP_BLOCKS {
        return 0.0;
    }
    let total: f64 = rewards.iter().map(|(_, r)| r).sum();
    let mut burned = 0.0;
    for (_, reward) in rewards.iter_mut() {
        let share = *reward / total;
        if share > constants::RAMP_UP_CAP {
            let excess = *reward - (total * constants::RAMP_UP_CAP);
            burned += excess;
            *reward = total * constants::RAMP_UP_CAP;
        }
    }
    burned
}

/// Compute founder time-lock: outputs mined before block 10,000
/// are spendable only after max(50000, current_block + 40000).
pub fn founder_lock_block(block_number: u64) -> u64 {
    if block_number < constants::RAMP_UP_BLOCKS {
        std::cmp::max(constants::FOUNDER_LOCK_BLOCKS, block_number + constants::FOUNDER_LOCK_ADDITIONAL)
    } else {
        0 // no lock after ramp-up
    }
}

pub struct RewardSummary {
    /// Miner rewards in base units (1 Ewatt = 1_000_000 units).
    pub miner_rewards: Vec<(Vec<u8>, u64)>,
    /// Total emission in base units.
    pub total_emission: u64,
    /// Emission rate used in base units per block.
    pub emission_rate_used: u64,
    /// Amount burned in base units.
    pub burned: u64,
}

pub fn compute_block_rewards(block_number: u64, commitments: &[Commitment], previous_commitments: &[f64], historical_avg_aops: f64) -> RewardSummary {
    let mut effective = Vec::new();
    let mut total_eff = 0.0;
    for c in commitments {
        if commitment::validate_commitment(c, previous_commitments).is_err() { continue; }
        // Use AOPS: efficiency = total_access_ops / (declared_ops_per_sec × time)
        let eff = commitment::compute_efficiency_aops(c.total_access_ops, c.access_ops_per_sec, c.time_seconds);
        let c_eff = commitment::effective_commitment(c.access_ops_per_sec, eff);
        effective.push((c_eff, c.miner_id));
        total_eff += c_eff;
    }
    let emission = compute_emission_rate(total_eff, historical_avg_aops);
    let mut rewards = Vec::new();
    for (c_eff, mid) in &effective {
        let r = if total_eff > 0.0 { (*c_eff / total_eff) * emission } else { 0.0 };
        rewards.push((mid.to_vec(), r));
    }
    let burned = apply_ramp_up_cap(block_number, &mut rewards);
    RewardSummary {
        miner_rewards: rewards.iter().map(|(pk, r)| (pk.clone(), ewatt_to_units(*r))).collect(),
        total_emission: ewatt_to_units(emission),  // pre-cap total (includes burned)
        emission_rate_used: ewatt_to_units(emission),
        burned: ewatt_to_units(burned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_emission_stable() { assert!((compute_emission_rate(25_000_000.,25_000_000.) - constants::BASE_EMISSION).abs() < 1e-6); }
    #[test] fn test_emission_double() { assert!((compute_emission_rate(50_000_000.,25_000_000.) - constants::BASE_EMISSION * 2.).abs() < 1e-6); }
    #[test] fn test_emission_floor() { assert!((compute_emission_rate(1.,25_000_000.) - constants::BASE_EMISSION * 0.05).abs() < 1e-6); }
    #[test] fn test_emission_ceiling() { assert!((compute_emission_rate(500_000_000.,25_000_000.) - constants::BASE_EMISSION * 20.).abs() < 1e-6); }
    #[test] fn test_ramp_up_cap() {
        let mut rewards = vec![(vec![1u8;32], 100.0), (vec![2u8;32], 0.0)];
        let burned = apply_ramp_up_cap(5000, &mut rewards);
        assert!(burned > 0.0);
        assert!(rewards[0].1 < 100.0);
    }
    #[test] fn test_ramp_up_no_cap_after() {
        let mut rewards = vec![(vec![1u8;32], 100.0)];
        let burned = apply_ramp_up_cap(10001, &mut rewards);
        assert_eq!(burned, 0.0);
        assert_eq!(rewards[0].1, 100.0);
    }
    #[test] fn test_founder_lock() {
        assert!(founder_lock_block(500) >= 50000);
        assert_eq!(founder_lock_block(15000), 0);
    }
    #[test] fn test_ewatt_to_units() {
        assert_eq!(ewatt_to_units(100.0), 100_000_000);
        assert_eq!(ewatt_to_units(0.0), 0);
        assert_eq!(ewatt_to_units(0.000001), 1);
        assert_eq!(ewatt_to_units(99.999999), 99_999_999);
        assert_eq!(ewatt_to_units(0.0000005), 1);  // rounding up
    }
    #[test] fn test_reward_proportional() {
        use crate::commitment::Commitment;
        use ed25519_dalek::Signer;
        fn signed_commit_aops(pk: [u8;32], aops: f64, sk: &ed25519_dalek::SigningKey) -> Commitment {
            let mut c = Commitment { miner_id: pk, access_ops_per_sec: aops, block_number: 0, total_access_ops: aops, time_seconds: 1., signature: vec![] };
            let msg = crate::commitment::commit_msg(&c);
            c.signature = sk.sign(&msg).to_bytes().to_vec();
            c
        }
        let sk1 = ed25519_dalek::SigningKey::from_bytes(&[1u8;32]);
        let pk1 = sk1.verifying_key().to_bytes();
        let sk2 = ed25519_dalek::SigningKey::from_bytes(&[2u8;32]);
        let pk2 = sk2.verifying_key().to_bytes();
        let c1 = signed_commit_aops(pk1, 25_000_000., &sk1);
        let c2 = signed_commit_aops(pk2, 25_000_000., &sk2);
        let prev = vec![20_000_000., 25_000_000., 25_000_000., 25_000_000.];
        let r = compute_block_rewards(20000, &[c1, c2], &prev, 25_000_000.);
        assert_eq!(r.miner_rewards[0].1, r.miner_rewards[1].1);
        assert!(r.miner_rewards[0].1 > 0);
    }
    #[test] fn test_reward_honest_more() {
        use crate::commitment::Commitment;
        use ed25519_dalek::Signer;
        fn signed_commit_aops(pk: [u8;32], aops: f64, ops: f64, sk: &ed25519_dalek::SigningKey) -> Commitment {
            let mut c = Commitment { miner_id: pk, access_ops_per_sec: aops, block_number: 0, total_access_ops: ops, time_seconds: 1., signature: vec![] };
            let msg = crate::commitment::commit_msg(&c);
            c.signature = sk.sign(&msg).to_bytes().to_vec();
            c
        }
        let sk1 = ed25519_dalek::SigningKey::from_bytes(&[1u8;32]);
        let pk1 = sk1.verifying_key().to_bytes();
        let sk2 = ed25519_dalek::SigningKey::from_bytes(&[2u8;32]);
        let pk2 = sk2.verifying_key().to_bytes();
        // Ambos declaram 25M ops/s (acima de MIN_COMMIT_AOPS = 20M)
        // Honest: entrega 25M ops = eff 1.0 → c_eff 25M
        // Under: entrega 3.25M ops = eff 0.13 → c_eff 3.25M (penalizado)
        // ratio esperado: 25 / 28.25 ≈ 0.885
        let honest = signed_commit_aops(pk1, 25_000_000., 25_000_000., &sk1);
        let under = signed_commit_aops(pk2, 25_000_000., 3_250_000., &sk2);
        let prev = vec![20_000_000., 25_000_000., 25_000_000., 25_000_000.];
        let r = compute_block_rewards(20000, &[honest, under], &prev, 25_000_000.);
        assert_eq!(r.miner_rewards.len(), 2, "both miners should be in rewards");
        assert!(r.miner_rewards[0].1 > r.miner_rewards[1].1);
        let total = r.miner_rewards[0].1 as f64 + r.miner_rewards[1].1 as f64;
        let ratio = r.miner_rewards[0].1 as f64 / total;
        assert!((ratio - 0.885).abs() < 0.02, "expected ~0.885, got {}", ratio);
    }
    #[test] fn test_total_emission_matches() {
        use crate::commitment::Commitment;
        use ed25519_dalek::Signer;
        fn signed_commit_aops(pk: [u8;32], aops: f64, ops: f64, sk: &ed25519_dalek::SigningKey) -> Commitment {
            let mut c = Commitment { miner_id: pk, access_ops_per_sec: aops, block_number: 0, total_access_ops: ops, time_seconds: 1., signature: vec![] };
            let msg = crate::commitment::commit_msg(&c);
            c.signature = sk.sign(&msg).to_bytes().to_vec();
            c
        }
        let sk = ed25519_dalek::SigningKey::from_bytes(&[1u8;32]);
        let pk = sk.verifying_key().to_bytes();
        let c = signed_commit_aops(pk, 25_000_000., 25_000_000., &sk);
        let prev = vec![20_000_000., 25_000_000., 25_000_000., 25_000_000.];
        let r = compute_block_rewards(5000, &[c], &prev, 25_000_000.);
        let sum_miners: u64 = r.miner_rewards.iter().map(|(_, amt)| amt).sum();
        assert_eq!(sum_miners + r.burned, r.total_emission);
    }
}
