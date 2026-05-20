use crate::constants;
use crate::commitment::{self, Commitment};

/// Convert Ewatt (f64) to base units with rounding.
fn ewatt_to_units(ewatt: f64) -> u64 {
    (ewatt * constants::UNITS_PER_EWATT as f64).round() as u64
}

pub fn compute_emission_rate(total_effective_gbps: f64, historical_avg_gbps: f64) -> f64 {
    if historical_avg_gbps <= 0.0 { return constants::BASE_EMISSION; }
    let rate = constants::BASE_EMISSION * total_effective_gbps / historical_avg_gbps;
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

pub fn compute_block_rewards(block_number: u64, commitments: &[Commitment], previous_commitments: &[f64], historical_avg_gbps: f64) -> RewardSummary {
    let mut effective = Vec::new();
    let mut total_eff = 0.0;
    for c in commitments {
        if commitment::validate_commitment(c, previous_commitments).is_err() { continue; }
        let eff = commitment::compute_efficiency(c.work_gb, c.bandwidth_gbps, c.time_seconds);
        let c_eff = commitment::effective_commitment(c.bandwidth_gbps, eff);
        effective.push((c_eff, c.miner_id));
        total_eff += c_eff;
    }
    let emission = compute_emission_rate(total_eff, historical_avg_gbps);
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
    #[test] fn test_emission_stable() { assert!((compute_emission_rate(100.,100.) - constants::BASE_EMISSION).abs() < 1e-6); }
    #[test] fn test_emission_double() { assert!((compute_emission_rate(200.,100.) - constants::BASE_EMISSION * 2.).abs() < 1e-6); }
    #[test] fn test_emission_floor() { assert!((compute_emission_rate(1.,100.) - constants::BASE_EMISSION * 0.05).abs() < 1e-6); }
    #[test] fn test_emission_ceiling() { assert!((compute_emission_rate(2000.,100.) - constants::BASE_EMISSION * 20.).abs() < 1e-6); }
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
        // Two miners with same effective commitment should get equal rewards
        use crate::commitment::Commitment;
        use ed25519_dalek::Signer;
        fn signed_commit(pk: [u8;32], bw: f64, sk: &ed25519_dalek::SigningKey) -> Commitment {
            let mut c = Commitment { miner_id: pk, bandwidth_gbps: bw, block_number: 0, work_gb: bw, time_seconds: 1., signature: vec![] };
            let msg = crate::commitment::commit_msg(&c);
            c.signature = sk.sign(&msg).to_bytes().to_vec();
            c
        }
        let sk1 = ed25519_dalek::SigningKey::from_bytes(&[1u8;32]);
        let pk1 = sk1.verifying_key().to_bytes();
        let sk2 = ed25519_dalek::SigningKey::from_bytes(&[2u8;32]);
        let pk2 = sk2.verifying_key().to_bytes();
        let c1 = signed_commit(pk1, 100., &sk1);
        let c2 = signed_commit(pk2, 100., &sk2);
        let prev = vec![50., 100., 100., 100.];
        let r = compute_block_rewards(20000, &[c1, c2], &prev, 100.);
        assert_eq!(r.miner_rewards[0].1, r.miner_rewards[1].1);
        assert!(r.miner_rewards[0].1 > 0);
    }
    #[test] fn test_reward_honest_more() {
        // Honest miner (eff=1.0) should get more than under-declarer (eff=0.5 after cap)
        use crate::commitment::Commitment;
        use ed25519_dalek::Signer;
        fn signed_commit(pk: [u8;32], bw: f64, w: f64, sk: &ed25519_dalek::SigningKey) -> Commitment {
            let mut c = Commitment { miner_id: pk, bandwidth_gbps: bw, block_number: 0, work_gb: w, time_seconds: 1., signature: vec![] };
            let msg = crate::commitment::commit_msg(&c);
            c.signature = sk.sign(&msg).to_bytes().to_vec();
            c
        }
        let sk1 = ed25519_dalek::SigningKey::from_bytes(&[1u8;32]);
        let pk1 = sk1.verifying_key().to_bytes();
        let sk2 = ed25519_dalek::SigningKey::from_bytes(&[2u8;32]);
        let pk2 = sk2.verifying_key().to_bytes();
        let honest = signed_commit(pk1, 100., 100., &sk1);
        let under = signed_commit(pk2, 10., 100., &sk2);
        let prev = vec![50., 100., 100., 100.];
        let r = compute_block_rewards(20000, &[honest, under], &prev, 100.);
        // honest c_eff=100, under c_eff=13 (capped at 1.3×): honest should get ~88.5%
        assert!(r.miner_rewards[0].1 > r.miner_rewards[1].1);
        let ratio = r.miner_rewards[0].1 as f64 / (r.miner_rewards[0].1 + r.miner_rewards[1].1) as f64;
        assert!((ratio - 0.885).abs() < 0.01);
    }
    #[test] fn test_total_emission_matches() {
        // Verify that sum of miner rewards + burned == total_emission
        use crate::commitment::Commitment;
        use ed25519_dalek::Signer;
        fn signed_commit(pk: [u8;32], bw: f64, w: f64, sk: &ed25519_dalek::SigningKey) -> Commitment {
            let mut c = Commitment { miner_id: pk, bandwidth_gbps: bw, block_number: 0, work_gb: w, time_seconds: 1., signature: vec![] };
            let msg = crate::commitment::commit_msg(&c);
            c.signature = sk.sign(&msg).to_bytes().to_vec();
            c
        }
        let sk = ed25519_dalek::SigningKey::from_bytes(&[1u8;32]);
        let pk = sk.verifying_key().to_bytes();
        let c = signed_commit(pk, 100., 100., &sk);
        let prev = vec![50., 100., 100., 100.];
        let r = compute_block_rewards(5000, &[c], &prev, 100.);
        let sum_miners: u64 = r.miner_rewards.iter().map(|(_, amt)| amt).sum();
        assert_eq!(sum_miners + r.burned, r.total_emission);
    }
}
