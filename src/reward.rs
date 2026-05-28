use crate::constants;

/// Emission rate = supply × (0.015 / BLOCKS_PER_YEAR) × (REF_COMMIT + te) / te
///
/// Supply-based emission that asymptotes to 1.5%/year as te → ∞.
/// The bootstrap boost comes naturally from (REF_COMMIT + te) / te:
///   - At 1 miner (te = REF_COMMIT = 1e9): multiplier = 2 → 3.0%/year
///   - At 3 miners (te = 3e9): multiplier = 1.33 → 2.0%/year
///   - At 10 miners (te = 1e10): multiplier = 1.1 → 1.65%/year
///   - At infinity: multiplier → 1.0 → 1.5%/year (asymptotic minimum)
///
/// Design rationale:
///   - 1.5% is the ASYMPTOTE (minimum rate), not the target at equilibrium
///   - The inverse boost is natural: less network = more emission per miner
///   - No floors, no caps, no bounds — the formula is smooth across all te
///   - "Inflation" in eWatts is DRAM efficiency (~1.5%/year), converging
///     to ~0% real dilution as the network grows
pub fn compute_emission_rate_int(total_eff: u64, total_supply: u64) -> u64 {
    use crate::constants;
    if total_eff == 0 || total_supply == 0 { return 0; }

    // R = supply × (0.015 / 52596) × (REF_COMMIT + te) / te
    // Single u128 chain to preserve precision:
    // R = supply × GROWTH_RATE × (REF_COMMIT + te) × EMISSION_PRECISION
    //     / (52596 × RATE_PRECISION × te × UNITS_PER_EWATT)
    ((total_supply as u128)
        .saturating_mul(constants::ANNUAL_GROWTH_RATE as u128)
        .saturating_mul((total_eff as u128).saturating_add(constants::REF_COMMIT_INT as u128))
        .saturating_mul(constants::EMISSION_PRECISION as u128)
        / (constants::BLOCKS_PER_YEAR as u128)
        / (constants::RATE_PRECISION as u128)
        / (total_eff as u128)
        / (constants::UNITS_PER_EWATT as u128)) as u64
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

/// Convert Ewatt (f64) to base units with rounding.
#[cfg(test)]
fn ewatt_to_units(ewatt: f64) -> u64 {
    (ewatt * constants::UNITS_PER_EWATT as f64).round() as u64
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
mod tests {
    use super::*;
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
    #[test] fn test_reward_proportional_int() {
        let eff: u64 = 1_000_000_000;  // ~1 miner at 1 GB/s
        let supply: u64 = 100_000_000;  // genesis supply (100 Ewatt)
        let em = compute_emission_rate_int(eff * 2, supply);
        let commits = vec![(eff, [1u8;32]), (eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert_eq!(rewards[0].1, rewards[1].1);
        assert!(rewards[0].1 > 0);
    }
    #[test] fn test_reward_honest_more_int() {
        let honest_eff: u64 = 1_000_000_000;
        let under_eff: u64 = 100_000_000;
        let total_eff = honest_eff + under_eff;
        let supply: u64 = 100_000_000;
        let em = compute_emission_rate_int(total_eff, supply);
        let commits = vec![(honest_eff, [1u8;32]), (under_eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert!(rewards[0].1 > rewards[1].1);
        let total_reward = rewards[0].1 + rewards[1].1;
        let ratio = rewards[0].1 as f64 / total_reward as f64;
        assert!((ratio - 0.909).abs() < 0.02,
            "Honest miner ratio {:.3}, expected ~0.909", ratio);
        assert!(ratio > 0.89, "Honest miner should get majority: {:.3}", ratio);
    }
    #[test] fn test_solo_miner_reward_positive() {
        let eff: u64 = 500_000_000;  // below REF_COMMIT → bootstrap boost
        let supply: u64 = 100_000_000;
        let em = compute_emission_rate_int(eff, supply);
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

    // Genesis supply used in tests (100 Ewatt in base units)
    const GENESIS_SUPPLY: u64 = 100_000_000;

    // T3.1: Supply-based emission — inverse boost asymptoting to 1.5%
    #[test]
    fn econ_emission_rate_basic() {
        let te_ref = constants::REF_COMMIT_INT;

        // At te = REF_COMMIT (1 miner): multiplier = (1e9+1e9)/1e9 = 2 → 3.0%/year
        let eq = compute_emission_rate_int(te_ref, GENESIS_SUPPLY);
        let expected_eq = ((GENESIS_SUPPLY as u128)
            .saturating_mul(constants::ANNUAL_GROWTH_RATE as u128)
            .saturating_mul((te_ref as u128) + (te_ref as u128))
            .saturating_mul(constants::EMISSION_PRECISION as u128)
            / (constants::BLOCKS_PER_YEAR as u128)
            / (constants::RATE_PRECISION as u128)
            / (te_ref as u128)
            / (constants::UNITS_PER_EWATT as u128)) as u64;
        assert_eq!(eq, expected_eq,
            "At 1 miner R ≈ {expected_eq} (got {eq})");

        // Tiny te (1 unit): very high boost
        let tiny = compute_emission_rate_int(1, GENESIS_SUPPLY);
        let expected_tiny = ((GENESIS_SUPPLY as u128)
            .saturating_mul(constants::ANNUAL_GROWTH_RATE as u128)
            .saturating_mul(constants::REF_COMMIT_INT as u128 + 1)
            .saturating_mul(constants::EMISSION_PRECISION as u128)
            / (constants::BLOCKS_PER_YEAR as u128)
            / (constants::RATE_PRECISION as u128)
            / 1u128
            / (constants::UNITS_PER_EWATT as u128)) as u64;
        assert_eq!(tiny, expected_tiny,
            "Tiny te must give boost: {tiny} vs {expected_tiny}");
        assert!(tiny > expected_eq,
            "Tiny te must give higher emission than 1 miner");

        // Large network (te = 100× REF_COMMIT): multiplier ≈ 1.01
        let large_te = te_ref * 100;
        let large = compute_emission_rate_int(large_te, GENESIS_SUPPLY);
        let expected_large = ((GENESIS_SUPPLY as u128)
            .saturating_mul(constants::ANNUAL_GROWTH_RATE as u128)
            .saturating_mul((te_ref as u128) + (large_te as u128))
            .saturating_mul(constants::EMISSION_PRECISION as u128)
            / (constants::BLOCKS_PER_YEAR as u128)
            / (constants::RATE_PRECISION as u128)
            / (large_te as u128)
            / (constants::UNITS_PER_EWATT as u128)) as u64;
        assert!((large as i64 - expected_large as i64).abs() <= 1,
            "100× network gives R ≈ {expected_large} (got {large})");

        // Zero te → 0
        assert_eq!(compute_emission_rate_int(0, GENESIS_SUPPLY), 0);
        // Zero supply → 0
        assert_eq!(compute_emission_rate_int(te_ref, 0), 0);
    }

    // T3.2: Zero total effective commitment → empty rewards
    #[test]
    fn econ_zero_commitment_no_reward() {
        let em = compute_emission_rate_int(constants::REF_COMMIT_INT, GENESIS_SUPPLY);
        let rewards = compute_block_rewards_int(1000, &[], em);
        assert!(rewards.is_empty(), "No commitments = no rewards");
    }

    // T3.3: Rewards proportional to effective commitment
    #[test]
    fn econ_reward_proportionality() {
        let eff_a: u64 = constants::REF_COMMIT_INT * 2;  // 2× equilibrium
        let eff_b: u64 = constants::REF_COMMIT_INT;       // 1× equilibrium
        let te = eff_a + eff_b;
        let em = compute_emission_rate_int(te, GENESIS_SUPPLY);
        let commits = vec![(eff_a, [1u8; 32]), (eff_b, [2u8; 32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert_eq!(rewards.len(), 2);
        // A has 2× B's commitment → A gets 2× reward
        assert!(rewards[0].1 >= rewards[1].1 * 2,
            "2x commit should get >= 2x reward: {} vs {}",
            rewards[0].1, rewards[1].1);
    }

    // T3.4: Ramp-up cap burns excess during early blocks
    #[test]
    fn econ_ramp_up_cap_burns_excess() {
        let eff: u64 = constants::REF_COMMIT_INT;
        let em = compute_emission_rate_int(eff, GENESIS_SUPPLY);
        let commits = vec![(eff, [1u8; 32])];

        let early = constants::RAMP_UP_BLOCKS - 1;
        let rewards = compute_block_rewards_int(early, &commits, em);
        assert!(!rewards.is_empty(), "Early block must produce rewards");
        assert!(rewards[0].1 > 0, "Reward must be positive");

        let late = constants::RAMP_UP_BLOCKS + 1000;
        let late_rewards = compute_block_rewards_int(late, &commits, em);
        assert!(!late_rewards.is_empty());
        assert!(late_rewards[0].1 >= rewards[0].1,
            "Post-ramp reward must not be less than ramp reward: {} vs {}",
            late_rewards[0].1, rewards[0].1);
    }

    // T3.5: Founder lock — mined miner rewards have spendable_after >= founder_lock_block(height)
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

    // T3.6: Supply grows monotonically and within expected bounds
    #[test]
    fn econ_supply_growth_positive() {
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
                "Supply must not decrease: {supply} < {last_supply} at height {height}");
            // With new supply-based emission, each block adds ~0.0005-50 base units
            // Sanity check: never exceed +100 Ewatt/block safety cap
            let max_per_block = 100_000_000u64;  // 100 Ewatt safety ceiling
            let max_expected = initial_supply + height * max_per_block;
            assert!(supply <= max_expected,
                "Supply must not exceed expected max: {supply} > {max_expected}");
            last_supply = supply;
            prev_hash = block.header.hash();
        }
    }
}
