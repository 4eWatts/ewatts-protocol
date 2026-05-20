use crate::reward::compute_emission_rate;
use crate::commitment::Commitment;
use crate::constants;
use crate::block::BlockHeader;
use crate::vr;

#[test]
fn integration_emission_bounds() {
    let floor = compute_emission_rate(1.0, 100.0);
    assert!((floor - constants::BASE_EMISSION * 0.05).abs() < 1e-6);
    let ceil = compute_emission_rate(2000.0, 100.0);
    assert!((ceil - constants::BASE_EMISSION * 20.0).abs() < 1e-6);
    let normal = compute_emission_rate(100.0, 100.0);
    assert!((normal - constants::BASE_EMISSION).abs() < 1e-6);
}

#[test]
fn integration_block_hash_determinism() {
    let h1 = BlockHeader {
        version: constants::PROTOCOL_VERSION,
        previous_hash: [0;32], merkle_root: [0;32],
        timestamp: 1000, height: 0, epoch: 0,
        difficulty_target: 1, total_effective_commit: 100.0,
        emission_rate: 100_000_000, miner_effective_commit: 50.0,
        vr_block: 0.001, coinbase_burn: 0, nonce: 42, elapsed_ms: 5000,
    };
    assert_eq!(h1.hash(), h1.hash());
}

#[test]
fn integration_vr_stability() {
    let v1 = vr::compute_vr(100.0, 100000.0, 1000, 600);
    let v2 = vr::compute_vr(100.0, 100000.0, 1000, 600);
    assert!((v1.vr_kwh_per_ewatt - v2.vr_kwh_per_ewatt).abs() < 1e-12);
}

#[test]
fn integration_reward_proportionality() {
    use crate::reward::compute_block_rewards;
    use ed25519_dalek::Signer;
    let sk = ed25519_dalek::SigningKey::from_bytes(&[1u8;32]);
    let pk = sk.verifying_key().to_bytes();
    let mut c1 = Commitment { miner_id: pk, bandwidth_gbps: 100., block_number: 0, work_gb: 100., time_seconds: 1., signature: vec![] };
    let msg1 = crate::commitment::commit_msg(&c1);
    c1.signature = sk.sign(&msg1).to_bytes().to_vec();
    let mut c2 = Commitment { miner_id: pk, bandwidth_gbps: 100., block_number: 0, work_gb: 100., time_seconds: 1., signature: vec![] };
    let msg2 = crate::commitment::commit_msg(&c2);
    c2.signature = sk.sign(&msg2).to_bytes().to_vec();
    let r = compute_block_rewards(20000, &[c1, c2], &[100.0], 100.0);
    assert!(r.miner_rewards.len() == 2);
    assert_eq!(r.miner_rewards[0].1, r.miner_rewards[1].1);
}
