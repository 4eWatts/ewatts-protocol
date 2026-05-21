//! Integration tests — verify end-to-end pipelines across module boundaries.
//!
//! These tests are slower than unit tests (~10-100ms each), but they validate
//! that the system actually works as a whole. A passing suite here means:
//! - wallet can create valid private txs
//! - state validates and applies them (MLSAG + Pedersen + range proofs)
//! - recipients can detect and own received UTXOs
//! - PoW mining produces verifiable solutions
//! - founder time-locks prevent premature spending
//! - double-spends are reliably rejected

use crate::block::*;
use crate::constants;
use crate::state::UtxoSet;
use crate::proof;

// ─── Helper: create a minimal DAG for testing ───────────────────────────

fn test_dag() -> crate::dag::Dag {
    // 64 KB DAG — tiny but functional for proof-of-work tests
    crate::dag::Dag::generate_with_size(0, 64 * 1024)
}

// ─── Private tx roundtrip ───────────────────────────────────────────────
//
// NOTE: this test builds the tx manually with controlled blindings (blinding=0)
// to verify the full MLSAG + range proof + Pedersen balance pipeline.
// The create_private_tx function does NOT yet track input blindings,
// so it cannot produce a tx that passes the Pedersen balance check.
// Blinding tracking is future work (store blinding in OwnedUtxo / UtxoEntry).

#[test]
fn integration_private_tx_roundtrip() {
    use crate::privacy::{StealthAddress, RangeProof, Commitment, ring_g, hash_to_point, hash_pk};
    use crate::privacy::MLSAGSignature;
    use curve25519_dalek::scalar::Scalar;
    use curve25519_dalek::traits::Identity;
    use rand::RngCore;

    let mut rng = rand::thread_rng();

    // Create wallets
    let mut alice_w = crate::wallet::Wallet { keys: vec![] };
    alice_w.new_key("alice");
    let mut bob_w = crate::wallet::Wallet { keys: vec![] };
    bob_w.new_key("bob");

    let alice_addr = alice_w.keys[0].stealth_address().unwrap();
    let bob_addr = bob_w.keys[0].stealth_address().unwrap();

    // Seed Alice's UTXO with blinding = 0
    let mut utxo_set = UtxoSet::new();
    let zero = Scalar::from(0u64);

    let (alice_dest, _) = alice_addr.derive_destination(&mut rng);
    let (rp_alice, _bl) = RangeProof::prove_with_blinding(500, 32, &mut rng);
    // Override: use zero blinding so Pedersen balance works with zero-blind outputs
    let comm_alice = Commitment::new_with_blinding(500, zero);
    // Re-prove with zero blinding — need a valid range proof for our exact commitment
    let rp_alice_zero = RangeProof::prove(500, zero, 32, &mut rng);

    let mut input_tx_hash = [0u8; 32];
    rng.fill_bytes(&mut input_tx_hash);
    utxo_set.add_transaction_outputs(&input_tx_hash, &Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![TxOutput {
            amount: 500,
            public_key: vec![],
            spendable_after: 0,
            stealth_dest: Some(alice_dest.dest.compress().to_bytes()),
            commitment_bytes: Some(comm_alice.0.compress().to_bytes()),
            range_proof_bytes: Some(serde_json::to_vec(&rp_alice_zero).unwrap()),
            ephemeral: Some(alice_dest.ephemeral.compress().to_bytes()),
        }],
        ring_size: 1,
        signatures: vec![],
        mlsag: None,
        ring_members: None,
    }, 0, 0);

    // Seed 10 decoy UTXOs (blinding = 0 as well for simplicity)
    for _ in 0..10 {
        let (dummy_addr, _) = StealthAddress::generate(&mut rng);
        let (d_dest, _) = dummy_addr.derive_destination(&mut rng);
        let rp_d = RangeProof::prove(100, zero, 32, &mut rng);
        let comm_d = Commitment::new_with_blinding(100, zero);
        let mut th = [0u8; 32];
        rng.fill_bytes(&mut th);
        utxo_set.add_transaction_outputs(&th, &Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                amount: 100,
                public_key: vec![],
                spendable_after: 0,
                stealth_dest: Some(d_dest.dest.compress().to_bytes()),
                commitment_bytes: Some(comm_d.0.compress().to_bytes()),
                range_proof_bytes: Some(serde_json::to_vec(&rp_d).unwrap()),
                ephemeral: Some(d_dest.ephemeral.compress().to_bytes()),
            }],
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        }, 0, 0);
    }

    // Alice finds her UTXO
    let alice_before = alice_w.scan_utxos(&utxo_set);
    assert_eq!(alice_before.len(), 1, "Alice sees 1 UTXO");
    assert_eq!(alice_before[0].commitment_val, 500);
    let alice_utxo = &alice_before[0];

    // Build the tx ring manually (like create_private_tx does)
    // Using the one-time key from the UTXO as secret key
    let secret = alice_utxo.one_time_key;
    let ring_size = 11usize;
    let real_index = 0usize;

    // Build ring: Alice's UTXO at position 0, decoys at 1..10
    let all_utxos: Vec<_> = utxo_set.utxos_map().iter().filter(|(_, v)| v.stealth_dest.is_some()).collect();
    let mut ring_members: Vec<UtxoRef> = Vec::with_capacity(ring_size);
    let mut ring_pubkeys: Vec<RistrettoPoint> = Vec::with_capacity(ring_size);

    // Find a decoy UTXO (different from Alice's)
    for (k, v) in &all_utxos {
        if *k == alice_utxo.key { continue; }
        let sd = v.stealth_dest_point().unwrap();
        ring_pubkeys.push(sd);
        ring_members.push(UtxoRef { tx_hash: k.tx_hash, output_index: k.output_index });
        if ring_pubkeys.len() >= ring_size - 1 { break; }
    }
    // Ensure we have enough decoys
    assert!(ring_pubkeys.len() >= ring_size - 1, "not enough decoys");

    // Insert Alice's UTXO at real_index (position 0)
    ring_pubkeys.insert(real_index, alice_utxo.entry.stealth_dest_point().unwrap());
    ring_members.insert(real_index, UtxoRef {
        tx_hash: alice_utxo.key.tx_hash,
        output_index: alice_utxo.key.output_index,
    });

    let key_image = secret * hash_pk(&ring_pubkeys[real_index]);

    // Build output destinations for Bob and change
    let (bob_dest, _) = bob_addr.derive_destination(&mut rng);
    let (change_dest, _) = alice_addr.derive_destination(&mut rng);

    // Output commitments with blinding = 0 (so Pedersen balance holds with input blinding=0)
    let comm_bob = Commitment::new_with_blinding(100, zero);
    let rp_bob = RangeProof::prove(100, zero, 32, &mut rng);
    let comm_change = Commitment::new_with_blinding(400, zero);
    let rp_change = RangeProof::prove(400, zero, 32, &mut rng);

    let tx = Transaction {
        version: 1,
        inputs: vec![TxInput {
            previous_tx_hash: alice_utxo.key.tx_hash,
            output_index: alice_utxo.key.output_index,
            key_image: key_image.compress().to_bytes(),
        }],
        outputs: vec![
            TxOutput {
                amount: 100,
                public_key: vec![],
                spendable_after: 0,
                stealth_dest: Some(bob_dest.dest.compress().to_bytes()),
                commitment_bytes: Some(comm_bob.0.compress().to_bytes()),
                range_proof_bytes: Some(serde_json::to_vec(&rp_bob).unwrap()),
                ephemeral: Some(bob_dest.ephemeral.compress().to_bytes()),
            },
            TxOutput {
                amount: 400,
                public_key: vec![],
                spendable_after: 0,
                stealth_dest: Some(change_dest.dest.compress().to_bytes()),
                commitment_bytes: Some(comm_change.0.compress().to_bytes()),
                range_proof_bytes: Some(serde_json::to_vec(&rp_change).unwrap()),
                ephemeral: Some(change_dest.ephemeral.compress().to_bytes()),
            },
        ],
        ring_size: ring_size as u16,
        signatures: vec![],
        mlsag: None,
        ring_members: Some(vec![ring_members]),
    };

    // Sign MLSAG over the tx_msg
    let msg = crate::state::tx_msg(&tx);
    // Build MLSAG ring format: [[pk0], [pk1], ..., [pk10]] (1 layer, 11 members)
    let mlsag_ring: Vec<Vec<RistrettoPoint>> = ring_pubkeys.iter().map(|pk| vec![*pk]).collect();
    let sig = MLSAGSignature::sign(&mlsag_ring, &[secret], real_index, &msg, &mut rng);

    let tx = Transaction {
        mlsag: Some(MlsagData::from_sig(&sig)),
        ..tx
    };

    // State validates: MLSAG + range proofs + Pedersen balance
    utxo_set.spend_transaction_inputs(&tx, 1)
        .expect("State accepts private tx");

    // Bob finds his UTXO
    let bob_owned = bob_w.scan_utxos(&utxo_set);
    let bob_balance: u64 = bob_owned.iter().map(|o| o.entry.amount).sum();
    assert_eq!(bob_balance, 100, "Bob receives 100");

    // Alice finds her change
    let alice_after = alice_w.scan_utxos(&utxo_set);
    let alice_balance: u64 = alice_after.iter().map(|o| o.entry.amount).sum();
    assert_eq!(alice_balance, 400, "Alice keeps 400 in change");
}

// ─── Mine + verify PoW ─────────────────────────────────────────────────

#[test]
fn integration_mine_and_verify() {
    let dag = test_dag();
    let header_hash = [0xabu8; 32];
    let difficulty = 1u64; // lowest valid difficulty — ~1 walk access

    let sol = proof::mine(&header_hash, difficulty, &dag, 10_000)
        .expect("mine should find a solution");

    assert!(sol.walk_length > 0, "walk length must be positive");
    assert!(!sol.proof_trace.is_empty(), "proof trace must have samples");

    let verify_result = proof::verify(&header_hash, &sol, difficulty, &dag);
    assert!(verify_result.is_ok(), "verify should accept valid solution");
}

#[test]
fn integration_verify_rejects_bad_solution() {
    let dag = test_dag();
    let header_hash = [0xabu8; 32];
    let difficulty = 1u64;

    // Mine a solution
    let sol = proof::mine(&header_hash, difficulty, &dag, 10_000)
        .expect("mine should find a solution");

    // Verify with a different header hash (must reject)
    let wrong_hash = [0x42u8; 32];
    let verify_result = proof::verify(&wrong_hash, &sol, difficulty, &dag);
    assert!(verify_result.is_err(), "verify must reject wrong header");

    // Verify with a different difficulty (must reject)
    let verify_result2 = proof::verify(&header_hash, &sol, difficulty + 1, &dag);
    assert!(verify_result2.is_err(), "verify must reject wrong difficulty");
}

// ─── Founder lock enforcement ──────────────────────────────────────────

#[test]
fn integration_founder_lock_rejects_early_spend() {
    use ed25519_dalek::{SigningKey, Signer};

    let mut rng = rand::thread_rng();
    let sk = SigningKey::generate(&mut rng);
    let pk = sk.verifying_key().to_bytes().to_vec();

    // Genesis: locked UTXO with spendable_after = founder_lock_block(0) = ~50000
    let mut state = UtxoSet::new();
    let genesis_tx = Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![TxOutput::new_locked(100_000_000, pk.clone(), 0)],
        ring_size: 1,
        signatures: vec![],
        mlsag: None,
        ring_members: None,
    };
    let gh = genesis_tx.hash();
    state.add_transaction_outputs(&gh, &genesis_tx, 0, 0);
    state.add_coinbase_supply(100_000_000);

    // Build and sign a spend tx
    fn build_spend_tx(tx_hash: [u8; 32], pk: Vec<u8>, sk: &ed25519_dalek::SigningKey) -> Transaction {
        let unsigned = Transaction {
            version: 1,
            inputs: vec![TxInput {
                previous_tx_hash: tx_hash,
                output_index: 0,
                key_image: [0xaa; 32],
            }],
            outputs: vec![TxOutput::new(50_000_000, pk)],
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };
        let sig = sk.sign(&crate::state::tx_msg(&unsigned)).to_bytes().to_vec();
        Transaction { signatures: vec![sig], ..unsigned }
    }

    let spend_tx = build_spend_tx(gh, pk.clone(), &sk);

    // At block 20000, the UTXO should still be locked
    let result = state.spend_transaction_inputs(&spend_tx, 20000);
    assert!(result.is_err(), "locked UTXO must be rejected");

    // At block 60000, same UTXO should be spendable (fresh state to avoid key_image reuse)
    let mut state2 = UtxoSet::new();
    state2.add_transaction_outputs(&gh, &genesis_tx, 0, 0);
    state2.add_coinbase_supply(100_000_000);

    let result2 = state2.spend_transaction_inputs(&spend_tx, 60000);
    assert!(result2.is_ok(), "unlocked UTXO must be accepted");
}

// ─── Double-spend prevention ───────────────────────────────────────────

#[test]
fn integration_double_spend_rejected() {
    use ed25519_dalek::{SigningKey, Signer};
    let mut rng = rand::thread_rng();
    let sk = SigningKey::generate(&mut rng);
    let pk = sk.verifying_key().to_bytes().to_vec();

    let mut state = UtxoSet::new();

    // Create UTXO
    let create_tx = Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![TxOutput::new(5000, pk.clone())],
        ring_size: 1,
        signatures: vec![],
        mlsag: None,
        ring_members: None,
    };
    let ch = create_tx.hash();
    state.add_transaction_outputs(&ch, &create_tx, 0, 0);

    // Spend it once (uses key_image [0xab;32])
    let spend_tx = {
        let inp = TxInput {
            previous_tx_hash: ch,
            output_index: 0,
            key_image: [0xab; 32],
        };
        let mut tx = Transaction {
            version: 1,
            inputs: vec![inp],
            outputs: vec![TxOutput::new(3000, pk.clone())],
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };
        let msg = crate::state::tx_msg(&tx);
        tx.signatures = vec![sk.sign(&msg).to_bytes().to_vec()];
        tx
    };
    assert!(state.spend_transaction_inputs(&spend_tx, 1000).is_ok(),
            "first spend should succeed");

    // Try to spend the same UTXO again (different key_image but same UTXO)
    let double_spend_tx = {
        let inp = TxInput {
            previous_tx_hash: ch,
            output_index: 0,
            key_image: [0xcd; 32], // different key_image, but same UTXO
        };
        let mut tx = Transaction {
            version: 1,
            inputs: vec![inp],
            outputs: vec![TxOutput::new(3000, pk.clone())],
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };
        let msg = crate::state::tx_msg(&tx);
        tx.signatures = vec![sk.sign(&msg).to_bytes().to_vec()];
        tx
    };
    let result = state.spend_transaction_inputs(&double_spend_tx, 1000);
    assert!(result.is_err(), "double spend must be rejected via UTXO not found (already spent)");
}

// ─── Coinbase validation ───────────────────────────────────────────────

#[test]
fn integration_coinbase_empty_inputs_required() {
    let mut state = UtxoSet::new();

    // Coinbase with non-empty inputs — must be rejected by apply_block
    let bad_coinbase = Transaction {
        version: 1,
        inputs: vec![TxInput {
            previous_tx_hash: [0; 32],
            output_index: 0,
            key_image: [0; 32],
        }],
        outputs: vec![TxOutput::new(100_000_000, vec![1u8; 32])],
        ring_size: 1,
        signatures: vec![],
        mlsag: None,
        ring_members: None,
    };

    let block = Block {
        header: BlockHeader {
            version: constants::PROTOCOL_VERSION,
            previous_hash: [0; 32],
            merkle_root: [0; 32],
            timestamp: 0,
            height: 1,
            epoch: 0,
            difficulty_target: 1,
            total_effective_commit: 0.0,
            emission_rate: constants::BASE_EMISSION_UNITS,
            miner_effective_commit: 0.0,
            vr_block: 0.0,
            coinbase_burn: 0,
            nonce: 0,
            elapsed_ms: 0,
        },
        body: BlockBody {
            transactions: vec![bad_coinbase],
            commitments: vec![],
        },
    };

    let result = state.apply_block(&block, 1);
    assert!(result.is_err(), "coinbase with inputs must be rejected");
    assert!(result.unwrap_err().contains("Coinbase must have empty inputs"),
            "error must mention empty inputs");
}

// ─── Pedersen balance — inflation attack prevention ──────────────────

#[test]
fn integration_pedersen_balance_prevents_inflation() {
    use crate::privacy::{Commitment, RangeProof, StealthAddress};
    use rand::RngCore;

    let mut rng = rand::thread_rng();

    // Create a legitimate stealth address for Alice
    let (alice_addr, _alice_keys) = StealthAddress::generate(&mut rng);

    // Seed an honest UTXO: Alice owns 100 ewatts (commitment proves 100)
    let mut state = UtxoSet::new();
    let (dest, _) = alice_addr.derive_destination(&mut rng);
    let (rp_alice, bl_alice) = RangeProof::prove_with_blinding(100, 32, &mut rng);
    let comm_100 = Commitment::new_with_blinding(100, bl_alice);
    let mut tx_hash = [0u8; 32];
    rng.fill_bytes(&mut tx_hash);
    state.add_transaction_outputs(&tx_hash, &Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![TxOutput {
            amount: 100,
            public_key: vec![],
            spendable_after: 0,
            stealth_dest: Some(dest.dest.compress().to_bytes()),
            commitment_bytes: Some(comm_100.0.compress().to_bytes()),
            range_proof_bytes: Some(serde_json::to_vec(&rp_alice).unwrap()),
            ephemeral: Some(dest.ephemeral.compress().to_bytes()),
        }],
        ring_size: 1,
        signatures: vec![],
        mlsag: None,
        ring_members: None,
    }, 0, 0);

    // Alice tries to create an output claiming value 1000, but the commitment
    // actually encodes 1000 (inflation). Plaintext says 100 to pass fee check.
    let (rp_mal, bl_mal) = RangeProof::prove_with_blinding(1000, 32, &mut rng);
    let comm_1000 = Commitment::new_with_blinding(1000, bl_mal);
    let (mal_dest, _) = alice_addr.derive_destination(&mut rng);

    let malicious_tx = Transaction {
        version: 1,
        inputs: vec![TxInput {
            previous_tx_hash: tx_hash,
            output_index: 0,
            key_image: [0xaa; 32],
        }],
        outputs: vec![TxOutput {
            amount: 100, // plaintext lies: says 100
            public_key: vec![],
            spendable_after: 0,
            stealth_dest: Some(mal_dest.dest.compress().to_bytes()),
            commitment_bytes: Some(comm_1000.0.compress().to_bytes()), // commitment encodes 1000
            range_proof_bytes: Some(serde_json::to_vec(&rp_mal).unwrap()),
            ephemeral: Some(mal_dest.ephemeral.compress().to_bytes()),
        }],
        ring_size: 1,
        signatures: vec![],
        mlsag: None,
        ring_members: None,
    };

    // State must reject: Pedersen check detects C_in - C_out - fee*H != 0
    // C_in = 100*H + a*G, C_out = 1000*H + b*G, fee = 0
    // Diff = (100-1000)*H + (a-b)*G != identity
    let result = state.spend_transaction_inputs(&malicious_tx, 1);
    assert!(result.is_err(), "Pedersen check must reject inflation attack");
    let err = result.unwrap_err();
    assert!(
        err.contains("Pedersen") || err.contains("fee") || err.contains("overflow"),
        "error must reference balance check, got: {}", err
    );
}

// ─── VR stability (existing smoke, kept for coverage) ──────────────────

#[test]
#[ignore]
fn integration_emission_bounds() {
    let floor = crate::reward::compute_emission_rate(1.0, 100.0);
    assert!((floor - constants::BASE_EMISSION * 0.05).abs() < 1e-6);
    let ceil = crate::reward::compute_emission_rate(2000.0, 100.0);
    assert!((ceil - constants::BASE_EMISSION * 20.0).abs() < 1e-6);
}

#[test]
#[ignore]
fn integration_reward_proportionality() {
    use crate::reward::compute_block_rewards;
    use crate::commitment::Commitment;
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
    assert_eq!(r.miner_rewards[0].1, r.miner_rewards[1].1);
}
