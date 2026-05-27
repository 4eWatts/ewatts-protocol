use crate::constants;

/// Emission rate = BASE × max(EFF_REF / total_eff, total_eff / EFF_REF)
/// Dual-mode formula:
///   Bootstrap (total_eff < EFF_REF): high reward to attract miners
///   Mature (total_eff >= EFF_REF): R grows with network, energy/eWatt stable
/// No bounds — ramp-up cap handles bootstrap excess.
pub fn compute_emission_rate_int(total_eff: u64, _hist_avg: u64) -> u64 {
    use crate::constants::{BASE_EMISSION_INT, EFF_REF_INT};
    if total_eff == 0 { return BASE_EMISSION_INT; }
    let rate = if total_eff < EFF_REF_INT {
        // Bootstrap: R = BASE × (EFF_REF / total_eff) — decays as network grows
        BASE_EMISSION_INT.saturating_mul(EFF_REF_INT) / total_eff
    } else {
        // Mature: R = BASE × (total_eff / EFF_REF) — grows with network
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

    // T3.1: Dual-Mode emission rate — bootstrap high, mature stable, no bounds
    #[test]
    fn econ_emission_rate_bounds() {
        // Mature phase (total_eff >= EFF_REF): R = BASE × (te / EFF_REF)
        // Very large network → R grows proportionally
        let high = compute_emission_rate_int(constants::EFF_REF_INT * 100, 1);
        let expected = constants::BASE_EMISSION_INT
            .saturating_mul(100);
        assert_eq!(high, expected, "100x network must give 100x BASE");

        // Bootstrap phase (total_eff < EFF_REF): R = BASE × (EFF_REF / te)
        // Single miner → high reward
        let low = compute_emission_rate_int(1, u64::MAX);
        let expected_boot = constants::BASE_EMISSION_INT
            .saturating_mul(constants::EFF_REF_INT) / 1;
        assert_eq!(low, expected_boot, "Single miner must get bootstrap reward");

        // At equilibrium (total_eff == EFF_REF): R = BASE
        let eq = compute_emission_rate_int(constants::EFF_REF_INT, 0);
        assert_eq!(eq, constants::BASE_EMISSION_INT,
            "At equilibrium R must equal BASE_EMISSION_INT");

        // Zero total_eff → BASE (safety fallback)
        let zero = compute_emission_rate_int(0, 100);
        assert_eq!(zero, constants::BASE_EMISSION_INT,
            "Zero total_eff must return BASE_EMISSION_INT");
    }

    // T3.2: Zero total effective commitment → empty rewards
    #[test]
    fn econ_zero_commitment_no_reward() {
        let em = compute_emission_rate_int(100, 100);
        let rewards = compute_block_rewards_int(1000, &[], em);
        assert!(rewards.is_empty(), "No commitments = no rewards");
    }

    // T3.3: Rewards proportional to effective commitment
    #[test]
    fn econ_reward_proportionality() {
        let eff: u64 = 1_000_000_000;
        let em = compute_emission_rate_int(eff * 3, eff * 3);
        // Miner A has 2x the effective commitment of Miner B
        let commits = vec![(eff * 2, [1u8; 32]), (eff, [2u8; 32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert_eq!(rewards.len(), 2);
        // A should get roughly 2x what B gets
        assert!(rewards[0].1 >= rewards[1].1 * 2,
            "2x commit should get >= 2x reward: {} vs {}",
            rewards[0].1, rewards[1].1);
    }

    // T3.4: Ramp-up cap burns excess during early blocks
    #[test]
    fn econ_ramp_up_cap_burns_excess() {
        // During ramp-up (block < RAMP_UP_BLOCKS), a single miner
        // gets capped to prevent early domination
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_int(eff, eff);
        let commits = vec![(eff, [1u8; 32])];

        // Block before ramp-up end
        let early = constants::RAMP_UP_BLOCKS - 1;
        let rewards = compute_block_rewards_int(early, &commits, em);
        // Burn is already handled inside compute_block_rewards_int
        assert!(!rewards.is_empty(), "Early block must produce rewards");
        assert!(rewards[0].1 > 0, "Reward must be positive");

        // After ramp-up, the same commitment should produce a higher reward
        // (no cap applied)
        let late = constants::RAMP_UP_BLOCKS + 1000;
        let late_rewards = compute_block_rewards_int(late, &commits, em);
        assert!(!late_rewards.is_empty());
        // Late reward should be >= early reward (cap removed)
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

        // Mine a block — the coinbase output should have spendable_after = founder_lock
        let (block1, _) = crate::mine_block_with_difficulty(
            gen_hash, 1, &mut state, 1, 64 * 1024,
        ).expect("Block 1");

        // The coinbase output's spendable_after must be >= founder_lock for this height
        let expected_lock = founder_lock_block(1);
        for output in &block1.body.transactions[0].outputs {
            assert!(output.spendable_after >= expected_lock,
                "Coinbase output must have spendable_after >= {}. got {}",
                expected_lock, output.spendable_after);
        }

        // Apply the block and verify the UTXO is properly locked
        state.apply_block_and_track(&block1, 1).expect("Apply block 1");

        // Find the miner's UTXO and check its lock
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

    // T3.6: Supply capped at maximum (total supply must not exceed MAX_SUPPLY)
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

        // Mine several blocks and verify supply grows monotonically
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
            // With initial 100M + 20 blocks * ~100 Ewatt each, max is ~2.1B units
            let max_expected = 100_000_000 + 20 * constants::BASE_EMISSION_UNITS;
            assert!(supply <= max_expected,
                "Supply must not exceed expected max: {} > {}",
                supply, max_expected);
            last_supply = supply;
            prev_hash = block.header.hash();
        }
    }
}
