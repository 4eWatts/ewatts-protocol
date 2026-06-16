use crate::constants;

// ─── Bootstrap Multiplier Table (v3) ──────────────────────────────────
// Generated offline at build time. All nodes use exactly the same 4096
// values regardless of architecture, libc version, or FPU behavior.
// NO f64.exp() at runtime. Bit-exact deterministic across all platforms.

include!("bootstrap_table.rs");

const BOOTSTRAP_TABLE_SIZE: usize = 4096;

/// v3: Compute bootstrap multiplier M(S) in EMISSION_PRECISION units.
///
/// Uses pre-computed lookup table with linear interpolation.
/// No floating-point at runtime. Bit-exact deterministic.
pub fn bootstrap_multiplier(total_supply_units: u64) -> u64 {
    let s_threshold = constants::S_THRESHOLD_UNITS;
    if total_supply_units >= s_threshold {
        return constants::EMISSION_PRECISION;
    }
    let n = BOOTSTRAP_TABLE_SIZE;
    let idx = ((total_supply_units as u128 * (n - 1) as u128) / s_threshold as u128) as usize;
    let idx = idx.min(n - 2);
    let lo = BOOTSTRAP_TABLE[idx];
    let hi = BOOTSTRAP_TABLE[idx + 1];
    let seg_start = (idx as u128 * s_threshold as u128) / (n - 1) as u128;
    let seg_end = ((idx + 1) as u128 * s_threshold as u128) / (n - 1) as u128;
    let seg_w = seg_end.saturating_sub(seg_start);
    if seg_w == 0 { return lo; }
    let off = (total_supply_units as u128).saturating_sub(seg_start);
    if hi >= lo {
        lo + ((off * (hi - lo) as u128) / seg_w) as u64
    } else {
        lo - ((off * (lo - hi) as u128) / seg_w) as u64
    }
}

/// v3: Compute emission rate in EMISSION_PRECISION units.
///
/// Formula: E_block = total_eff × C_node × M(S) / (EFF_PER_MINER_REF × P_target)
///
/// Where C_node = 0.0020625 USD/block/node, EFF_PER_MINER_REF = 1e9 COMMIT_PRECISION,
/// and P_target = 1.0.
///
/// E_prec = total_eff × M_prec × 2_062_500 / 1e18
///
/// At calibration (total_eff = 100k × 1e9 = 1e14, M=1):
///   E_prec = 1e14 × 1e9 × 2.0625e6 / 1e18 = 206.25 eW/block ✓
pub fn compute_emission_rate_v3(total_supply_units: u64, total_eff: u64) -> u64 {
    let mult_prec = bootstrap_multiplier(total_supply_units);
    const COST_NODE_AMPLIFIED: u64 = 2_062_500;  // 0.0020625 × 1e9
    
    let numerator = (total_eff as u128)
        .saturating_mul(mult_prec as u128)
        .saturating_mul(COST_NODE_AMPLIFIED as u128);
    let denominator: u128 = 1_000_000_000_000_000_000u128; // 1e18
    if denominator == 0 { return 0; }
    (numerator / denominator) as u64
}

/// Convert emission rate (EMISSION_PRECISION units) to base units
pub fn emission_prec_to_units(emission_prec: u64) -> u64 {
    emission_prec.saturating_mul(constants::UNITS_PER_EWATT) / constants::EMISSION_PRECISION
}

// ═══════════════════════════════════════════════════════════════════════
// Deprecated (v27) emission — kept for reference, removed when v3 is live
// ═══════════════════════════════════════════════════════════════════════

/// v27 emission formula — DEPRECATED. Use compute_emission_rate_v3 instead.
/// Dual-mode: R = BASE × max(EFF_REF / total_eff, total_eff / EFF_REF)
#[deprecated(note = "use compute_emission_rate_v3 instead")]
pub fn compute_emission_rate_v27_deprecated(total_eff: u64, _hist_avg: u64) -> u64 {
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

/// v3: Compute per-miner rewards in one pass. Integrates v3 emission rate
/// with proportional commitment distribution and ramp-up cap.
/// total_supply_units: current chain supply (determines bootstrap multiplier)
/// commitments: slice of (effective_commitment, miner_id) pairs
/// Returns Vec<(miner_pubkey_vec, reward_in_base_units)>
pub fn compute_block_rewards_v3(
    block_number: u64,
    total_supply_units: u64,
    commitments: &[(u64, [u8; 32])],
) -> Vec<(Vec<u8>, u64)> {
    let total_eff: u64 = commitments.iter().map(|(c, _)| c).sum();
    if total_eff == 0 { return vec![]; }
    let emission_prec = compute_emission_rate_v3(total_supply_units, total_eff);
    // Proportional distribution in EMISSION_PRECISION units
    let mut rewards_prec: Vec<(Vec<u8>, u64)> = commitments.iter().map(|(c, mid)| {
        let r = ((*c as u128).saturating_mul(emission_prec as u128) / total_eff as u128) as u64;
        (mid.to_vec(), r)
    }).collect();
    // Apply ramp-up cap, track burned
    apply_ramp_up_cap_int(block_number, &mut rewards_prec);
    // Convert to base units
    rewards_prec.iter().map(|(pk, r)| {
        let base = r.saturating_mul(constants::UNITS_PER_EWATT) / constants::EMISSION_PRECISION;
        (pk.clone(), base)
    }).collect()
}

/// Founder outputs locked.
/// Mainnet: max(50000, block + 40000) blocks (~347 days)
/// Testnet: max(500, block) blocks (~8 minutes at 1s block time)
pub fn founder_lock_block(block_number: u64) -> u64 {
    if block_number >= constants::RAMP_UP_BLOCKS {
        return 0;
    }
    #[cfg(feature = "testnet")] {
        return std::cmp::max(constants::TESTNET_FOUNDER_LOCK, block_number);
    }
    #[cfg(not(feature = "testnet"))] {
        std::cmp::max(constants::FOUNDER_LOCK_BLOCKS, block_number + constants::FOUNDER_LOCK_ADDITIONAL)
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
    fn test_emission_v3_genesis() {
        // Solo miner at genesis: M=M_MAX, total_eff=1e9
        // E_prec = 1e9 × 1e14 × 2,062,500 / 1e18 ≈ 2.0625e11
        // ≈ 206,250 eW/block
        let total_eff = 1_000_000_000u64;
        let em = compute_emission_rate_v3(0, total_eff);
        let base = emission_prec_to_units(em);
        let ewatt = base as f64 / constants::UNITS_PER_EWATT as f64;
        let ratio = ewatt / 206.25;
        assert!((ratio - 1.0).abs() < 0.01,
            "Genesis: expected ~206 ewatt/block, got {} (ratio: {})",
            ewatt, ratio);
    }

    #[test]
    fn test_emission_v3_mature() {
        // At maturity (S >= 10B): M=1, total_eff = 1e14 (100k miners × 1e9)
        // E = 1e14 × 1.0 × 2,062,500 / 1e18 = 2.0625e20 / 1e18 = 206.25
        let total_eff = 100_000u64 * 1_000_000_000u64;  // 1e14
        let em = compute_emission_rate_v3(constants::S_THRESHOLD_UNITS, total_eff);
        let base_units = emission_prec_to_units(em);
        let expected_ew = 206.25f64;
        let got_ew = base_units as f64 / constants::UNITS_PER_EWATT as f64;
        let ratio = got_ew / expected_ew;
        assert!((ratio - 1.0).abs() < 0.001,
            "Mature emission (total_eff=1e14): expected ~206.25 eW/block, got {} eW/block (ratio: {})",
            got_ew, ratio);
    }

    #[test]
    fn test_emission_v3_scales_with_eff() {
        // At maturity: doubling total_eff should double emission
        let te1 = 50_000u64 * 1_000_000_000u64;
        let te2 = 100_000u64 * 1_000_000_000u64;
        let em1 = compute_emission_rate_v3(constants::S_THRESHOLD_UNITS, te1);
        let em2 = compute_emission_rate_v3(constants::S_THRESHOLD_UNITS, te2);
        let ratio = em2 as f64 / em1 as f64;
        assert!((ratio - 2.0).abs() < 0.002,
            "Doubling total_eff should double emission: ratio {}", ratio);
    }
    #[test]
    fn test_emission_v3_zero_eff() {
        assert_eq!(compute_emission_rate_v3(0, 0), 0);
    }

    #[test]
    fn test_compute_block_rewards_v3_proportional() {
        let commits = vec![(3_000_000_000u64, [1u8;32]), (1_000_000_000u64, [2u8;32])];
        let rewards = compute_block_rewards_v3(300, 0, &commits);
        assert_eq!(rewards.len(), 2);
        let ratio = rewards[0].1 as f64 / rewards[1].1 as f64;
        assert!((ratio - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_block_rewards_v3_empty() {
        assert!(compute_block_rewards_v3(300, 0, &[]).is_empty());
    }

    #[test]
    fn test_compute_block_rewards_v3_solo() {
        let supply = constants::S_THRESHOLD_UNITS;
        let eff = 1_000_000_000u64;
        let direct = emission_prec_to_units(compute_emission_rate_v3(supply, eff));
        // block after ramp-up (>=10000) so no 80% cap applies
        let rewards = compute_block_rewards_v3(20000, supply, &[(eff, [1u8;32])]);
        assert!(!rewards.is_empty());
        let ratio = rewards[0].1 as f64 / direct as f64;
        assert!((ratio - 1.0).abs() < 0.001,
            "Solo reward mismatch: reward={} direct={} ratio={}",
            rewards[0].1, direct, ratio);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(deprecated)]
    #[test] fn test_founder_lock() {
        // testnet: block+0 with 500-block lock; mainnet: 50000-block lock
        #[cfg(feature = "testnet")]
        assert_eq!(founder_lock_block(500), 500);
        #[cfg(not(feature = "testnet"))]
        assert!(founder_lock_block(500) >= 50000);
        assert_eq!(founder_lock_block(15000), 0);
    }
    #[allow(deprecated)]
    #[test] fn test_reward_proportional_int() {
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_v27_deprecated(eff * 2, eff * 2);
        let commits = vec![(eff, [1u8;32]), (eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert_eq!(rewards[0].1, rewards[1].1);
        assert!(rewards[0].1 > 0);
    }
    #[allow(deprecated)]
    #[test] fn test_reward_honest_more_int() {
        let honest_eff: u64 = 100_000_000_000;
        let under_eff: u64 = 10_000_000_000;
        let total_eff = honest_eff + under_eff;
        let em = compute_emission_rate_v27_deprecated(total_eff, total_eff);
        let commits = vec![(honest_eff, [1u8;32]), (under_eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert!(rewards[0].1 > rewards[1].1);
        let ratio = rewards[0].1 as f64 / (rewards[0].1 + rewards[1].1) as f64;
        assert!((ratio - 0.909).abs() < 0.01);
    }
    #[allow(deprecated)]
    #[test] fn test_solo_miner_reward_positive() {
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_v27_deprecated(eff, eff);
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

    // v3 equivalents should go in tests_v3 module.
    // These tests are preserved for v27 backward compatibility only.

    #[test]
    #[ignore = "v27 formula — needs rewrite for v3"]
    #[allow(deprecated)]
    fn econ_emission_rate_bounds() {
        let high = compute_emission_rate_v27_deprecated(constants::EFF_REF_INT * 100, 1);
        let expected = constants::BASE_EMISSION_INT.saturating_mul(100);
        assert_eq!(high, expected, "100x network must give 100x BASE");
        let low = compute_emission_rate_v27_deprecated(1, u64::MAX);
        let expected_boot = constants::BASE_EMISSION_INT
            .saturating_mul(constants::EFF_REF_INT) / 1;
        assert_eq!(low, expected_boot, "Single miner must get bootstrap reward");
        let eq = compute_emission_rate_v27_deprecated(constants::EFF_REF_INT, 0);
        assert_eq!(eq, constants::BASE_EMISSION_INT,
            "At equilibrium R must equal BASE_EMISSION_INT");
        let zero = compute_emission_rate_v27_deprecated(0, 100);
        assert_eq!(zero, constants::BASE_EMISSION_INT,
            "Zero total_eff must return BASE_EMISSION_INT");
    }

    #[test]
    #[ignore = "v27 formula — needs rewrite for v3"]
    #[allow(deprecated)]
    fn econ_zero_commitment_no_reward() {
        let em = compute_emission_rate_v27_deprecated(100, 100);
        let rewards = compute_block_rewards_int(1000, &[], em);
        assert!(rewards.is_empty(), "No commitments = no rewards");
    }

    #[test]
    #[ignore = "v27 formula — needs rewrite for v3"]
    #[allow(deprecated)]
    fn econ_reward_proportionality() {
        let eff: u64 = 1_000_000_000;
        let em = compute_emission_rate_v27_deprecated(eff * 3, eff * 3);
        let commits = vec![(eff * 2, [1u8; 32]), (eff, [2u8; 32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert_eq!(rewards.len(), 2);
        assert!(rewards[0].1 >= rewards[1].1 * 2,
            "2x commit should get >= 2x reward: {} vs {}", rewards[0].1, rewards[1].1);
    }

    #[test]
    #[ignore = "v27 formula — needs rewrite for v3"]
    #[allow(deprecated)]
    fn econ_ramp_up_cap_burns_excess() {
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_v27_deprecated(eff, eff);
        let commits = vec![(eff, [1u8; 32])];
        let early = constants::RAMP_UP_BLOCKS - 1;
        let rewards = compute_block_rewards_int(early, &commits, em);
        assert!(!rewards.is_empty(), "Early block must produce rewards");
        assert!(rewards[0].1 > 0, "Reward must be positive");
        let late = constants::RAMP_UP_BLOCKS + 1000;
        let late_rewards = compute_block_rewards_int(late, &commits, em);
        assert!(!late_rewards.is_empty());
        assert!(late_rewards[0].1 >= rewards[0].1,
            "Post-ramp reward must not be less: {} vs {}", late_rewards[0].1, rewards[0].1);
    }

    #[test]
    #[ignore = "v27 mining test — needs rewrite for v3"]
    #[allow(deprecated)]
    fn econ_founder_lock_enforced() {
        use crate::state::UtxoSet;
        use ed25519_dalek::SigningKey;
        let sk = SigningKey::generate(&mut rand::thread_rng());
        let pk = sk.verifying_key().to_bytes();
        let mut state = UtxoSet::genesis(100_000_000, &pk);
        let (gen_block, _) = crate::mine_block_with_difficulty(
            [0u8; 32], 0, &mut state, 1, 64 * 1024).expect("Genesis");
        let gen_hash = gen_block.header.hash();
        state.apply_block_and_track(&gen_block, 0).expect("Apply genesis");
        let (block1, _) = crate::mine_block_with_difficulty(
            gen_hash, 1, &mut state, 1, 64 * 1024).expect("Block 1");
        let expected_lock = founder_lock_block(1);
        for output in &block1.body.transactions[0].outputs {
            assert!(output.spendable_after >= expected_lock,
                "Coinbase output lock: got {}, expected >= {}", output.spendable_after, expected_lock);
        }
        state.apply_block_and_track(&block1, 1).expect("Apply block 1");
        let keys = state.utxo_keys_for(&pk);
        assert!(!keys.is_empty(), "Miner must have UTXOs after mining");
    }

    #[test]
    #[ignore = "v27 mining test — needs rewrite for v3"]
    #[allow(deprecated)]
    fn econ_supply_cap_not_exceeded() {
        use crate::state::UtxoSet;
        use ed25519_dalek::SigningKey;
        let sk = SigningKey::generate(&mut rand::thread_rng());
        let pk = sk.verifying_key().to_bytes();
        let mut state = UtxoSet::genesis(100_000_000, &pk);
        let (gen_block, _) = crate::mine_block_with_difficulty(
            [0u8; 32], 0, &mut state, 1, 64 * 1024).expect("Genesis");
        let gen_hash = gen_block.header.hash();
        state.apply_block_and_track(&gen_block, 0).expect("Apply genesis");
        let initial_supply = state.total_supply();
        assert_eq!(initial_supply, 100_000_000, "Genesis supply must be 100M");
        let mut prev_hash = gen_hash;
        let mut last_supply = initial_supply;
        for height in 1..=20u64 {
            let (block, _) = crate::mine_block_with_difficulty(
                prev_hash, height, &mut state, 1, 64 * 1024
            ).expect(&format!("Block {}", height));
            state.apply_block_and_track(&block, height)
                .expect(&format!("Apply block {}", height));
            let supply = state.total_supply();
            assert!(supply >= last_supply, "Supply must not decrease at height {}", height);
            last_supply = supply;
            prev_hash = block.header.hash();
        }
    }
}
