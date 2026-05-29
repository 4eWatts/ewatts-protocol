use crate::constants;

/// v3: Compute bootstrap multiplier M(S) in EMISSION_PRECISION units.
///
/// M(S) = max(1, 100,000 × exp(-ln(100,000) × S / 10B))
///
/// Uses f64 for the exponential (deterministic via IEEE 754, once per block).
/// Total supply is in base units (UNITS_PER_EWATT = 1e6 per eWatt).
pub fn bootstrap_multiplier(total_supply_units: u64) -> u64 {
    let s_threshold_units = constants::S_THRESHOLD_UNITS;
    if total_supply_units >= s_threshold_units {
        return constants::EMISSION_PRECISION; // 1.0 × precision
    }
    // M(S) = M_MAX × exp(-k × S / S_threshold)
    // In f64 for simplicity; computed once per block
    let s_ratio = total_supply_units as f64 / s_threshold_units as f64;
    let exponent = -(constants::LN_M_MAX_PRECISION as f64 / 1_000_000.0) * s_ratio;
    let mult_f = (constants::M_MAX as f64) * exponent.exp();
    // Clamp to [1.0, M_MAX] and convert to EMISSION_PRECISION
    let mult_clamped = mult_f.max(1.0).min(constants::M_MAX as f64);
    (mult_clamped * constants::EMISSION_PRECISION as f64).round() as u64
}

/// v3: Compute emission rate in EMISSION_PRECISION units.
///
/// Formula: E_block = n_active × C_node × M(S) / P_target
///
/// C_node = 75W × 600s / 3,600,000 × $0.165/kWh = $0.0020625
/// In EMISSION_PRECISION (1e9) units: 0.0020625 × 1e9 = 2,062,500
pub fn compute_emission_rate_v3(total_supply_units: u64, n_active: u64) -> u64 {
    let mult_prec = bootstrap_multiplier(total_supply_units);
    // C_NODE_PRECISION = round(0.0020625 × 1e9) = 2,062,500
    let c_node_prec = 2_062_500u64;
    // E = n × C_node × M / P_target
    // All in precision units; P_target = 1.0 = EMISSION_PRECISION in precision
    // Final: E_prec = n × 2,062,500 × mult_prec / EMISSION_PRECISION
    let numerator = (n_active as u128)
        .saturating_mul(c_node_prec as u128)
        .saturating_mul(mult_prec as u128);
    let denominator = constants::EMISSION_PRECISION as u128;
    if denominator == 0 { return 0; }
    (numerator / denominator) as u64
}

/// Convert emission rate (EMISSION_PRECISION units) to base units (1/1e6 eWatt)
pub fn emission_prec_to_units(emission_prec: u64) -> u64 {
    emission_prec.saturating_mul(constants::UNITS_PER_EWATT) / constants::EMISSION_PRECISION
}

// ═══════════════════════════════════════════════════════════════════════
// Deprecated (v27) emission — kept for reference, removed when v3 is live
// ═══════════════════════════════════════════════════════════════════════

/// Emission rate = BASE × max(EFF_REF / total_eff, total_eff / EFF_REF)
/// Dual-mode formula (DEPRECATED in v3 — kept for migration window)
pub fn compute_emission_rate_int(total_eff: u64, _hist_avg: u64) -> u64 {
    use crate::constants::{BASE_EMISSION_INT, EFF_REF_INT};
    if total_eff == 0 { return BASE_EMISSION_INT; }
    let rate = if total_eff < EFF_REF_INT {
        BASE_EMISSION_INT.saturating_mul(EFF_REF_INT) / total_eff
    } else {
        BASE_EMISSION_INT.saturating_mul(total_eff) / EFF_REF_INT
    };
    rate
}

/// Cap individual miner share during ramp-up, returning excess burned
pub fn apply_ramp_up_cap_int(block_number: u64, rewards: &mut Vec<(Vec<u8>, u64)>) -> u64 {
    if block_number >= crate::constants::RAMP_UP_BLOCKS {
        return 0;
    }
    let total: u64 = rewards.iter().map(|(_, r)| r).sum();
    if total == 0 { return 0; }
    let mut burned = 0u64;
    for (_, reward) in rewards.iter_mut() {
        let share_exceeds = reward.saturating_mul(crate::constants::CAP_PRECISION)
            > total.saturating_mul(crate::constants::RAMP_UP_CAP_INT);
        if share_exceeds {
            let max_reward = total.saturating_mul(crate::constants::RAMP_UP_CAP_INT)
                / crate::constants::CAP_PRECISION;
            let excess = reward.saturating_sub(max_reward);
            burned = burned.saturating_add(excess);
            *reward = max_reward;
        }
    }
    burned
}

/// Compute per-miner rewards proportional to effective commitment, in base units
pub fn compute_block_rewards_int(
    block_number: u64,
    commitments: &[(u64, [u8; 32])],
    emission_rate_int: u64,
) -> Vec<(Vec<u8>, u64)> {
    let total_eff: u64 = commitments.iter().map(|(c, _)| c).sum();
    if total_eff == 0 { return vec![]; }
    let mut rewards: Vec<(Vec<u8>, u64)> = commitments.iter().map(|(c, mid)| {
        let r = if total_eff > 0 {
            let num = (*c as u128).saturating_mul(emission_rate_int as u128);
            (num / total_eff as u128) as u64
        } else {
            0
        };
        (mid.to_vec(), r)
    }).collect();
    let _burned = apply_ramp_up_cap_int(block_number, &mut rewards);
    rewards.iter().map(|(pk, r)| {
        let base_units = r.saturating_mul(crate::constants::UNITS_PER_EWATT)
            / crate::constants::EMISSION_PRECISION;
        (pk.clone(), base_units)
    }).collect()
}

/// Founder outputs locked until max(50000, block + 40000)
pub fn founder_lock_block(block_number: u64) -> u64 {
    if block_number < constants::RAMP_UP_BLOCKS {
        std::cmp::max(constants::FOUNDER_LOCK_BLOCKS, block_number + constants::FOUNDER_LOCK_ADDITIONAL)
    } else {
        0 // no lock after ramp-up
    }
}

#[cfg(test)]
mod tests_v3 {
    use super::*;

    #[test]
    fn test_bootstrap_multiplier_at_genesis() {
        // At S=0 (block 1): M(S) should be M_MAX × exp(0) = M_MAX
        let m = bootstrap_multiplier(0);
        let expected = constants::M_MAX as u64 * constants::EMISSION_PRECISION;
        // Allow small rounding error (f64 → integer)
        let ratio = m as f64 / expected as f64;
        assert!(ratio > 0.999 && ratio < 1.001,
            "M(0) should be ~M_MAX: got {}, expected ~{}", m, expected);
    }

    #[test]
    fn test_bootstrap_multiplier_at_threshold() {
        // At S = S_threshold: M(S) = 1.0
        let m = bootstrap_multiplier(constants::S_THRESHOLD_UNITS);
        assert_eq!(m, constants::EMISSION_PRECISION,
            "M(S_threshold) should be 1.0× precision: got {}", m);

        // At S > S_threshold: M(S) = 1.0
        let m2 = bootstrap_multiplier(constants::S_THRESHOLD_UNITS + 1);
        assert_eq!(m2, constants::EMISSION_PRECISION,
            "M(>threshold) should be 1.0× precision: got {}", m2);
    }

    #[test]
    fn test_emission_v3_solo_miner() {
        // Solver miner at genesis: n=1, S=0
        let em = compute_emission_rate_v3(0, 1);
        assert!(em > 0, "Emission must be positive at genesis, got {}", em);
        // At M=M_MAX (~100k): ~0.002063 × 100k = ~206.3 eWatt/block in precision
        let expected_min = 200u64 * constants::EMISSION_PRECISION / constants::UNITS_PER_EWATT;
        assert!(em > expected_min,
            "Solo miner at genesis should get ~206 eW, got {} prec (expected ~{} prec)",
            em, expected_min);
    }

    #[test]
    fn test_emission_v3_mature() {
        // At maturity (S >= 10B): M=1, n=100k
        // E = 100000 × 0.002063 × 1.0 / 1.0 = 206.3 eWatt/block
        let em = compute_emission_rate_v3(constants::S_THRESHOLD_UNITS, 100_000);
        let base_units = emission_prec_to_units(em);
        let expected = 206_300_000u64; // ~206.3 eWatt in base units
        let ratio = base_units as f64 / expected as f64;
        assert!(ratio > 0.95 && ratio < 1.05,
            "Mature emission (N=100k): expected ~206.3 eW, got {} eW (ratio: {})",
            base_units as f64 / constants::UNITS_PER_EWATT as f64, ratio);
    }

    #[test]
    fn test_emission_v3_scales_with_n() {
        // At maturity: doubling miners should double emission
        let em1 = compute_emission_rate_v3(constants::S_THRESHOLD_UNITS, 100_000);
        let em2 = compute_emission_rate_v3(constants::S_THRESHOLD_UNITS, 200_000);
        assert!(em2 >= em1 * 2 || em2 as u128 * 100 > em1 as u128 * 199,
            "Doubling miners should double emission: {} vs 2×{}",
            em2, em1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_founder_lock() {
        assert!(founder_lock_block(500) >= 50000);
        assert_eq!(founder_lock_block(15000), 0);
    }
    #[test] fn test_reward_proportional_int() {
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_int(eff * 2, eff * 2);
        let commits = vec![(eff, [1u8;32]), (eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert_eq!(rewards[0].1, rewards[1].1);
        assert!(rewards[0].1 > 0);
    }
    #[test] fn test_reward_honest_more_int() {
        let honest_eff: u64 = 100_000_000_000;
        let under_eff: u64 = 10_000_000_000;
        let total_eff = honest_eff + under_eff;
        let em = compute_emission_rate_int(total_eff, total_eff);
        let commits = vec![(honest_eff, [1u8;32]), (under_eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert!(rewards[0].1 > rewards[1].1);
        let ratio = rewards[0].1 as f64 / (rewards[0].1 + rewards[1].1) as f64;
        assert!((ratio - 0.909).abs() < 0.01);
    }
    #[test] fn test_solo_miner_reward_positive() {
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_int(eff, eff);
        let commits = vec![(eff, [1u8;32])];
        let rewards = compute_block_rewards_int(5000, &commits, em);
        assert!(!rewards.is_empty());
        assert!(rewards[0].1 > 0);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Phase 3 — Economic Security Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod econ_tests {
    use super::*;
    use crate::constants;

    #[test]
    fn econ_emission_rate_bounds() {
        let high = compute_emission_rate_int(constants::EFF_REF_INT * 100, 1);
        let expected = constants::BASE_EMISSION_INT
            .saturating_mul(100);
        assert_eq!(high, expected, "100x network must give 100x BASE");
        let low = compute_emission_rate_int(1, u64::MAX);
        let expected_boot = constants::BASE_EMISSION_INT
            .saturating_mul(constants::EFF_REF_INT) / 1;
        assert_eq!(low, expected_boot, "Single miner must get bootstrap reward");
        let eq = compute_emission_rate_int(constants::EFF_REF_INT, 0);
        assert_eq!(eq, constants::BASE_EMISSION_INT,
            "At equilibrium R must equal BASE_EMISSION_INT");
        let zero = compute_emission_rate_int(0, 100);
        assert_eq!(zero, constants::BASE_EMISSION_INT,
            "Zero total_eff must return BASE_EMISSION_INT");
    }

    #[test]
    fn econ_zero_commitment_no_reward() {
        let em = compute_emission_rate_int(100, 100);
        let rewards = compute_block_rewards_int(1000, &[], em);
        assert!(rewards.is_empty(), "No commitments = no rewards");
    }

    #[test]
    fn econ_reward_proportionality() {
        let eff: u64 = 1_000_000_000;
        let em = compute_emission_rate_int(eff * 3, eff * 3);
        let commits = vec![(eff * 2, [1u8; 32]), (eff, [2u8; 32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert_eq!(rewards.len(), 2);
        assert!(rewards[0].1 >= rewards[1].1 * 2,
            "2x commit should get >= 2x reward: {} vs {}",
            rewards[0].1, rewards[1].1);
    }

    #[test]
    fn econ_ramp_up_cap_burns_excess() {
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_int(eff, eff);
        let commits = vec![(eff, [1u8; 32])];
        let early = constants::RAMP_UP_BLOCKS - 1;
        let rewards = compute_block_rewards_int(early, &commits, em);
        assert!(!rewards.is_empty(), "Early block must produce rewards");
        assert!(rewards[0].1 > 0, "Reward must be positive");
        let late = constants::RAMP_UP_BLOCKS + 1000;
        let late_rewards = compute_block_rewards_int(late, &commits, em);
        assert!(!late_rewards.is_empty());
        assert!(late_rewards[0].1 >= rewards[0].1,
            "Post-ramp reward must not be less: {} vs {}",
            late_rewards[0].1, rewards[0].1);
    }

    #[test]
    fn econ_founder_lock_enforced() {
        use crate::block::*;
        use crate::state::UtxoSet;
        use ed25519_dalek::SigningKey;
        let sk = SigningKey::generate(&mut rand::thread_rng());
        let pk = sk.verifying_key().to_bytes();
        let mut state = UtxoSet::genesis(100_000_000, &pk);
        let (gen_block, _) = crate::mine_block_with_difficulty(
            [0u8; 32], 0, &mut state, 1, 64 * 1024,
        ).expect("Genesis");
        let gen_hash = gen_block.header.hash();
        state.apply_block_and_track(&gen_block, 0).expect("Apply genesis");
        let (block1, _) = crate::mine_block_with_difficulty(
            gen_hash, 1, &mut state, 1, 64 * 1024,
        ).expect("Block 1");
        let expected_lock = founder_lock_block(1);
        for output in &block1.body.transactions[0].outputs {
            assert!(output.spendable_after >= expected_lock,
                "Coinbase output must have spendable_after >= {}. got {}",
                expected_lock, output.spendable_after);
        }
        state.apply_block_and_track(&block1, 1).expect("Apply block 1");
        let keys = state.utxo_keys_for(&pk);
        assert!(!keys.is_empty(), "Miner must have UTXOs after mining");
        for key in &keys {
            if let Some(utxo) = state.get_utxo(key) {
                if utxo.spendable_after > 0 {
                    assert!(utxo.spendable_after >= expected_lock,
                        "UTXO locked until {}, expected >= {}",
                        utxo.spendable_after, expected_lock);
                }
            }
        }
    }

    #[test]
    fn econ_supply_cap_not_exceeded() {
        use crate::state::UtxoSet;
        use ed25519_dalek::SigningKey;
        let sk = SigningKey::generate(&mut rand::thread_rng());
        let pk = sk.verifying_key().to_bytes();
        let mut state = UtxoSet::genesis(100_000_000, &pk);
        let (gen_block, _) = crate::mine_block_with_difficulty(
            [0u8; 32], 0, &mut state, 1, 64 * 1024,
        ).expect("Genesis");
        let gen_hash = gen_block.header.hash();
        state.apply_block_and_track(&gen_block, 0).expect("Apply genesis");
        let initial_supply = state.total_supply();
        assert_eq!(initial_supply, 100_000_000, "Genesis supply must be 100M");
        let mut prev_hash = gen_hash;
        let mut last_supply = initial_supply;
        for height in 1..=20u64 {
            let (block, _) = crate::mine_block_with_difficulty(
                prev_hash, height, &mut state, 1, 64 * 1024,
            ).expect(&format!("Block {}", height));
            state.apply_block_and_track(&block, height)
                .expect(&format!("Apply block {}", height));
            let supply = state.total_supply();
            assert!(supply >= last_supply,
                "Supply must not decrease: {} < {} at height {}",
                supply, last_supply, height);
            let max_expected = 100_000_000 + 20 * constants::BASE_EMISSION_UNITS;
            assert!(supply <= max_expected,
                "Supply must not exceed expected max: {} > {}",
                supply, max_expected);
            last_supply = supply;
            prev_hash = block.header.hash();
        }
    }
}
