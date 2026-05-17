use crate::constants;
use crate::commitment::{self, Commitment};

pub fn compute_emission_rate(total_effective_gbps: f64, historical_avg_gbps: f64) -> f64 {
    if historical_avg_gbps <= 0.0 { return constants::BASE_EMISSION; }
    let rate = constants::BASE_EMISSION * total_effective_gbps / historical_avg_gbps;
    rate.clamp(constants::BASE_EMISSION * constants::EMISSION_FLOOR_MULTIPLIER,
               constants::BASE_EMISSION * constants::EMISSION_CEILING_MULTIPLIER)
}

/// Apply ramp-up cap: no single miner receives >80% of reward during first 10,000 blocks.
/// Excess goes to coinbase_burn.
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
    pub miner_rewards: Vec<(Vec<u8>, f64)>,
    pub total_emission: f64,
    pub emission_rate_used: f64,
    pub burned: f64,
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
        total_emission: rewards.iter().map(|(_,r)| r).sum(),
        emission_rate_used: emission,
        miner_rewards: rewards,
        burned,
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
    #[test] fn test_reward_proportional() {
        // Two miners with same effective commitment should get equal rewards
        use crate::commitment::Commitment;
        let c1 = Commitment { miner_id: [1;32], bandwidth_gbps: 100., block_number: 0, work_gb: 100., time_seconds: 1., signature: vec![] };
        let c2 = Commitment { miner_id: [2;32], bandwidth_gbps: 100., block_number: 0, work_gb: 100., time_seconds: 1., signature: vec![] };
        let prev = vec![50., 100., 100., 100.];
        let r = compute_block_rewards(0, &[c1, c2], &prev, 100.);
        assert!((r.miner_rewards[0].1 - r.miner_rewards[1].1).abs() < 1e-6);
        assert!(r.miner_rewards[0].1 > 0.);
    }
    #[test] fn test_reward_honest_more() {
        // Honest miner (eff=1.0) should get more than under-declarer (eff=0.5 after cap)
        use crate::commitment::Commitment;
        let honest = Commitment { miner_id: [1;32], bandwidth_gbps: 100., block_number: 0, work_gb: 100., time_seconds: 1., signature: vec![] };
        let under = Commitment { miner_id: [2;32], bandwidth_gbps: 10., block_number: 0, work_gb: 100., time_seconds: 1., signature: vec![] };
        let prev = vec![50., 100., 100., 100.];
        let r = compute_block_rewards(0, &[honest, under], &prev, 100.);
        // honest c_eff=100, under c_eff=13 (capped at 1.3×): honest should get ~88.5%
        assert!(r.miner_rewards[0].1 > r.miner_rewards[1].1);
        assert!((r.miner_rewards[0].1 / (r.miner_rewards[0].1 + r.miner_rewards[1].1) - 0.885).abs() < 0.01);
    }
}
