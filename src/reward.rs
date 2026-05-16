use crate::commitment::{self, Commitment};
use crate::constants;

pub fn compute_emission_rate(total: f64, avg: f64) -> f64 {
    if avg <= 0.0 {
        return constants::BASE_EMISSION;
    }
    (constants::BASE_EMISSION * total / avg).clamp(
        constants::BASE_EMISSION * 0.1,
        constants::BASE_EMISSION * 10.0,
    )
}

pub struct RewardSummary {
    pub miner_rewards: Vec<(Vec<u8>, f64)>,
    pub total_emission: f64,
    pub emission_rate_used: f64,
}

pub fn compute_block_rewards(commits: &[Commitment], prev: &[f64], avg: f64) -> RewardSummary {
    let mut eff = Vec::new();
    for c in commits {
        if commitment::validate_commitment(c, prev).is_err() {
            continue;
        }
        let e = commitment::compute_efficiency(c.work_gb, c.bandwidth_gbps, c.time_seconds);
        let ce = commitment::effective_commitment(c.bandwidth_gbps, e);
        eff.push((ce, c.miner_id));
    }
    let total_eff: f64 = eff.iter().map(|(c, _)| c).sum();
    let em = compute_emission_rate(total_eff, avg);
    if total_eff <= 0.0 {
        return RewardSummary {
            miner_rewards: vec![],
            total_emission: 0.0,
            emission_rate_used: em,
        };
    }
    let mut rw = Vec::new();
    let mut total_emission = 0.0;
    for (ce, mid) in &eff {
        let share = (*ce / total_eff) * em;
        total_emission += share;
        rw.push((mid.to_vec(), share));
    }
    RewardSummary {
        miner_rewards: rw,
        total_emission,
        emission_rate_used: em,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commitment;
    use crate::commitment::Commitment;
    use ed25519_dalek::Signer;
    use rand::RngCore;

    fn signed_commit(gbps: f64, wk: f64) -> Commitment {
        let mut seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        let pk = sk.verifying_key().to_bytes();
        let mut c = Commitment {
            miner_id: pk,
            bandwidth_gbps: gbps,
            block_number: 1,
            work_gb: wk,
            time_seconds: 1.0,
            signature: vec![],
        };
        let msg = commitment::commit_msg(&c);
        c.signature = sk.sign(&msg).to_bytes().to_vec();
        c
    }

    #[test]
    fn test_emission_stable() {
        assert!((compute_emission_rate(100., 100.) - 100.).abs() < 1e-6);
    }
    #[test]
    fn test_emission_double() {
        assert!((compute_emission_rate(200., 100.) - 200.).abs() < 1e-6);
    }
    #[test]
    fn test_emission_floor() {
        assert!((compute_emission_rate(1., 100.) - 10.).abs() < 1e-6);
    }
    #[test]
    fn test_emission_ceiling() {
        assert!((compute_emission_rate(2000., 100.) - 1000.).abs() < 1e-6);
    }
    #[test]
    fn test_rewards_sum_to_emission() {
        let c = vec![signed_commit(100., 100.), signed_commit(80., 80.)];
        let r = compute_block_rewards(&c, &[], 100.0);
        assert!((r.total_emission - r.emission_rate_used).abs() < 1e-6);
    }
    #[test]
    fn test_rewards_basic() {
        let c = vec![signed_commit(100., 100.)];
        let r = compute_block_rewards(&c, &[], 100.0);
        assert!(r.total_emission > 0.0);
        assert!(r.emission_rate_used > 0.0);
    }
    #[test]
    fn test_two_equal() {
        let c = vec![signed_commit(100., 100.), signed_commit(100., 100.)];
        let r = compute_block_rewards(&c, &[], 100.0);
        assert_eq!(r.miner_rewards.len(), 2);
        assert!((r.miner_rewards[0].1 - r.miner_rewards[1].1).abs() < 1e-6);
    }
}
