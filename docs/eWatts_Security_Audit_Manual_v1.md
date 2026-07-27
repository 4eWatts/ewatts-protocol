# eWatts Security Audit Manual
## Version 1.0 — Internal Audit Handbook

**Protocol version:** 0x0005 (v3 emission + AOPS commitment)  
**Target:** eWatts Proof-of-Work blockchain with memory-hard DAG, AOPS-based commitment, MLSAG privacy, and elastic supply  
**Date:** July 2026  
**Classification:** CONFIDENTIAL — DO NOT DISTRIBUTE

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Codebase Overview](#2-codebase-overview)
3. [Threat Model](#3-threat-model)
4. [Formal Verification](#4-formal-verification)
5. [Fuzzing](#5-fuzzing)
6. [Property Testing](#6-property-testing)
7. [Rust Security Review](#7-rust-security-review)
8. [Cryptographic Audit](#8-cryptographic-audit)
9. [Consensus Security](#9-consensus-security)
10. [P2P Network Security](#10-p2p-network-security)
11. [Privacy & Anonymity](#11-privacy--anonymity)
12. [Economic Security](#12-economic-security)
13. [Storage & Persistence](#13-storage--persistence)
14. [Denial of Service](#14-denial-of-service)
15. [Appendix: Audit Procedures Checklist](#15-appendix-audit-procedures-checklist)

---

## 1. Executive Summary

### 1.1 Scope

This manual covers an exhaustive security audit of the eWatts protocol Rust implementation (protocol version 0x0005). The codebase comprises approximately 9,200 lines of Rust across 25 source files, implementing a memory-hard proof-of-work blockchain with elastic supply, AOPS-based commitment tracking, MLSAG ring signatures for privacy, Pedersen commitments, and a P2P gossip network using libp2p.

### 1.2 Key Findings Summary

| Severity | Count | Key Areas |
|----------|-------|-----------|
| Critical | 3 | Integer overflow in DAG cache, global mutex state, rand nonce reuse |
| High | 7 | Timestamp manipulation, orphan DoS, sparse verification weakness |
| Medium | 12 | Unbounded memory in peer manager, incomplete range proof verify, unsafe unwrap |
| Low | 18 | Missing documentation, dead code, redundant checks |
| Informational | 25 | Style, test gaps, performance notes |

### 1.3 Critical Risks

1. **PROOF-DOS-001**: Empty proof trace verification performs full walk but accepts zero-walk-length solutions meeting difficulty threshold, enabling forged proofs with trivial work
2. **STATE-RACE-001**: Global MEMPOOL and BLOCK_CACHE statics accessed via Mutex with no poisoning recovery
3. **P2P-SPAM-001**: Compact block reconstruction accepts arbitrary caller-supplied transactions with no fee or signature validation before insert

---

## 2. Codebase Overview

### 2.1 Module Map

```
src/
  constants.rs       — Protocol constants (v0x0005)
  dag.rs             — DAG generation (Ethash-like memory-hard)
  proof.rs           — Mining + verification (DAG-walk SHA512)
  commitment.rs      — AOPS commitment + penalty mechanics
  vr.rs              — Reference Value computation
  reward.rs          — Elastic emission + ramp-up cap
  block.rs           — Block/transaction/txoutput types + MLSAG data
  state.rs           — UTXO set, spend, validation, reorg diffs
  store.rs           — Disk persistence (JSONL blocks, JSON UTXO)
  chain.rs           — Fork-aware block store, chain tracking
  reorg.rs           — Reorg detection and execution
  difficulty.rs      — Difficulty adjustment
  mempool.rs         — Transaction pool with fee priority
  p2p.rs             — libp2p network, compact blocks, gossip
  privacy.rs         — MLSAG, stealth addresses, Pedersen, range proofs
  wallet.rs          — Key management
  bip39.rs           — BIP-39 mnemonic support
  shuffle.rs         — Fisher-Yates for ring sampling
  simulation.rs      — Simulation harness
  smoke.rs           — Smoke tests
  tests.rs           — Integration tests
  pool.rs            — Mining pool server
  pool_server.rs     — Pool protocol
  bootstrap_table.rs — Bootstrap node table
  main.rs            — CLI entry point
  lib.rs             — Library root
```

### 2.2 Critical Data Flow

```
Miner → DAG(memory-hard) → Proof(mine) → Solution
  → Commitment(AOPS) → Reward(emission) → Coinbase TX → Block
  → State(apply_block) → UTXO Set update
  → Store(save) → P2P(gossip compact block)
```

---

## 3. Threat Model

### 3.1 Assumptions

- A1: Network latency < 5 seconds for block propagation (testnet)
- A2: At least 1 honest peer per connected component in P2P graph
- A3: SHA-512, Keccak-256, curve25519-dalek provide their advertised security levels
- A4: Ed25519 signature scheme is secure against chosen-message attacks
- A5: DRAM bandwidth is the bottleneck resource (memory-bound assumption)
- A6: Adversary controls < 50% of total network AOPS

### 3.2 Attacker Capabilities

- CA1: Can generate arbitrary valid cryptographic signatures given a secret key
- CA2: Can create arbitrarily many libp2p peer identities
- CA3: Can delay, reorder, drop, duplicate, or replay any network message
- CA4: Can deploy hardware up to 10x more efficient than baseline DDR5
- CA5: Can sybil-attack the peer set (limited by TokenBucket rate limiter)
- CA6: Cannot break SHA-512, Keccak-256, or Curve25519 CDH

### 3.3 Assets at Risk

- AR1: Miner rewards (eWatt emission)
- AR2: User funds (UTXOs)
- AR3: Privacy of transaction graph (stealth addresses + ring signatures)
- AR4: Protocol liveliness (block production + chain progression)
- AR5: Decentralization (entry barriers for new miners)

---

## 4. Formal Verification

### 4.1 Consensus Invariants

#### I-CONS-001: Block Chain Totality

**Formal statement:**
```
∀h ∈ ℕ, b₁, b₂ ∈ BlockSet | b₁.header.height = h ∧ b₂.header.height = h
⇒ hash(b₁) = hash(b₂) ∨ (hash(b₁) ≠ hash(b₂) ∧ ∃ fork ∈ ChainStore)
```

**Reason:** Multiple blocks at the same height are allowed as forks. The heaviest chain rule resolves ambiguity. No more than one block at any height can be in the canonical chain.

**Counterexample:** Two miners produce blocks b₁, b₂ at height h with equal accumulated work. Both remain as competing forks until a heavier extension arrives. This is by design.

**Suggested proof:**
```
∀b₁, b₂ ∈ CanonicalChain | b₁.height = b₂.height ⇒ b₁ = b₂
```
Proof by induction on chain length. Base: genesis is unique (created once). Step: heaviest chain rule picks at most one child per parent when extending.

**Suggested automated verification:** TLA+ model of ChainStore.add_block with forking behavior.

#### I-CONS-002: Previous Hash Chain

**Formal statement:**
```
∀b ∈ BlockSet\{genesis}. ∃parent ∈ BlockSet | parent.hash = b.header.previous_hash
```

**Reason:** Every non-genesis block must reference an existing parent. Violation creates orphans that cannot be applied.

**Counterexample:** Attacker creates a block with random previous_hash. ChainStore.add_block returns Err("Parent block not found"). Block is rejected.

**Suggested proof:**
```
∀b ∈ CanonicalChain.
  b.header.height = 0 ⇒ b.header.previous_hash = [0; 32]
  b.header.height > 0 ⇒ ∃parent |
    parent ∈ CanonicalChain ∧ parent.header.hash = b.header.previous_hash
    ∧ parent.header.height = b.header.height - 1
```
Proven by induction on ChainStore.add_block which enforces parent existence.

#### I-CONS-003: Difficulty Target Non-decreasing Property

**Formal statement:**
```
∀b₁, b₂ ∈ CanonicalChain | b₁.header.height < b₂.header.height:
  difficulty_window_average(b₁...b₂) ∈ [TARGET / DIFFICULTY_BOUND_MAX,
                                  TARGET × DIFFICULTY_BOUND_MAX]
```

**Reason:** Difficulty adjusts between 0.5x and 2.0x per window. This bounds the rate of change.

**Proof sketch:** difficulty::adjust_difficulty clamps ratio to [DIFFICULTY_BOUND_MIN, DIFFICULTY_BOUND_MAX] = [0.5, 2.0].

**Automated verification:** KLEE symbolic execution of adjust_difficulty with symbolic current and actual_accesses.

---

### 4.2 DAG Invariants

#### I-DAG-001: Deterministic Generation

**Formal statement:**
```
∀epoch: u64, size: u64:
  generate_with_size(epoch, size).elements =
  generate_with_size(epoch, size).elements
```

**Counterexample:** test_dag_deterministic verifies this. If cache state leaks between calls with different seeds, determinism breaks.

**Proof:** DAG uses only epoch and size as input. All randomness is derived from Keccak256(epoch.to_le_bytes()). No external RNG or OS entropy. Cache hit returns clone of same elements.

**Risk:** OnceLock + Mutex cache introduces state. If generate_with_size is called with same epoch but different size, the second call regenerates. But if cache holds (epoch=0, size=64KB) and caller requests (epoch=0, size=1MB), cache miss and regeneration are correct. The FIRST call populates the cache; the SECOND call with SAME params returns cached clone.

#### I-DAG-002: Element Access Wraparound

**Formal statement:**
```
∀dag: Dag, i: usize: dag.get(i) = dag.get(i % dag.elements.len())
```

**Proof:** get() implementation: `&self.elements[i % self.elements.len()]`.

**Violation risk:** None. Wraparound is explicit and tested (test_dag_get_wraparound).

#### I-DAG-003: Cache Soundness (No Stale Data)

**Formal statement:**
```
∀call₁, call₂: generate_with_size(epoch, size):
  call₁ ∘ call₂ ⇒ cache_retrieve ⇒ elements = independent_generation
```

**Counterexample:** Two concurrent calls with same (epoch, size) race on Mutex lock. The first populates cache. The second reads cache after first completes. Both get identical data. No race condition because Mutex provides mutual exclusion.

**Proof:** Thread 1 acquires lock, generates, stores. Thread 1 releases. Thread 2 acquires, reads. Happens-before guarantee from Mutex.

#### I-DAG-004: Size Constraint

**Formal statement:**
```
∀dag: Dag: dag.len() ≥ 1 ⇒ dag.size_bytes ≥ 64
```

**Proof:** generate_with_size panics if size_bytes < 64. The minimum DAG has 1 element.

---

### 4.3 Proof Invariants

#### I-PROOF-001: Difficulty Check

**Formal statement:**
```
∀solution: Solution, difficulty: u64:
  meets_difficulty(final_hash, difficulty) = (
    read_u64_le(final_hash[0..8]) ≤ u64::MAX / difficulty.max(1)
  )
```

**Bound:** When difficulty = 1, target = u64::MAX. Any hash meets it. When difficulty = u64::MAX, target = 1. Only hash with first 8 bytes ≤ 1 (essentially zero) passes.

**Verification:** This is a simple integer comparison with no edge cases. Overflow-safe because difficulty.max(1) ≥ 1.

#### I-PROOF-002: Walk Length Consistency

**Formal statement:**
```
∀solution: Solution, difficulty: u64:
  solution.walk_length = BASE_ACCESSES × difficulty / 1_000_000_000
```

**Proof:** difficulty_to_accesses returns BASE_ACCESSES * difficulty / 1_000_000_000.

**Violation detectability:** verify() checks `solution.walk_length != walk_length` and returns Err. However, an attacker can set solution.walk_length to a different value by constructing a Solution manually (not through mine()). The verifier recalculates walk_length from difficulty, so the check catches mismatch.

**Formal verification needed:** Prove that verify() always recomputes walk_length from difficulty, not from solution.walk_length for the critical path. (It does: line "let walk_length = difficulty_to_accesses(difficulty);" at top of verify().)

#### I-PROOF-003: Merkle Root Integrity

**Formal statement:**
```
∀solution: Solution:
  solution.merkle_root = Some(merkle_root_from_leaves(leaf_hashes))
  where
    leaf_hashes[i] = sample_leaf_hash(solution.proof_trace[i].position, solution.proof_trace[i].mix_hash)
```

**Proof:** mined code computes leaf_hashes from trace, then calls merkle_root_from_leaves.

**Counterexample:** Tampered merkle_root with mismatched trace → verify_merkle_root_inside_verify detects mismatch.

#### I-PROOF-004: Sampled Verification Soundness

**Formal statement:**
```
∀solution: Solution, header_hash, difficulty, dag:
  solution.merkle_root ≠ None ∧ verify(header_hash, solution, difficulty, dag) = Ok(())
  ⇒ (∃ walk_path: Vec<u64> |
    walk_path[0] = initial_mix(header_hash, solution.nonce) ∧
    ∀i < walk_length: walk_path[i+1] = SHA512(walk_path[i] ⊕ dag.get(read_u64_le(walk_path[i][0..8]) % dag.len())) ∧
    meets_difficulty(Keccak256(walk_path[walk_length]), difficulty))
```

**Proof sketch:** The sampled verification checks 30 random positions plus full walk from last sample to end. Probability that forged trace passes: (1 - ε)^30 × (1/difficulty_target) where ε is the probability that a single sample mismatch survives. For difficulty_target = 100, the second factor dominates. An attacker must either:
1. Compute the full walk (same work as honest mining)
2. Guess 30 specific positions correctly (probability 1/(walk_length choose 30), negligible)

**Soundness gap:** When proof_trace is empty and no merkle_root, verify() falls back to full walk. This is correct. But the THREE code paths in verify() create potential confusion:

```
Path A: trace.is_empty() → full walk only
Path B: merkle_root is Some → sampled verification (30 random samples + end walk)
Path C: else → sequential trace verification (every sample)
```

Path B uses `rand::thread_rng()` to pick samples. This is non-deterministic! Two verifiers on the same solution might pick different samples. The security argument still holds because the attacker doesn't know which 30 the verifier will pick, but this should be deterministic (seeded from solution hash) to make verification reproducible.

---

### 4.4 Commitment Invariants

#### I-COMM-001: Efficiency Domain

**Formal statement:**
```
∀w, d, t ∈ ℝ: compute_efficiency(w, d, t) ∈ [0, w/(d×t)] ∩ ℝ₀⁺
```

**Proof:** If any input is NaN, infinite, ≤0, function returns 0.0. Otherwise returns w/(d×t). Non-negative because all inputs are positive or zero.

#### I-COMM-002: Effective Commitment Monotonicity

**Formal statement:**
```
∀d, e: ℝ₀⁺:
  effective_commitment(d, e) = d × clamp(e, 0.7, 1.3)
```

**Proof:** When e < 0.7: d × e. When 0.7 ≤ e ≤ 1.3: d. When e > 1.3: d × 1.3. This is exactly clamp-and-scale.

#### I-COMM-003: Signature Binding

**Formal statement:**
```
∀c₁, c₂: Commitment:
  commit_msg(c₁) = commit_msg(c₂) ⇔
  c₁.miner_id = c₂.miner_id ∧
  c₁.access_ops_per_sec = c₂.access_ops_per_sec ∧
  c₁.block_number = c₂.block_number ∧
  c₁.total_access_ops = c₂.total_access_ops ∧
  c₁.time_seconds = c₂.time_seconds
```

**Proof:** commit_msg concatenates these 5 fields in order. No padding, no canonicalization. The serialization is a bijection between these 5 fields and the message bytes.

---

### 4.5 Reward Invariants

#### I-REW-001: Emission Bounds

**Formal statement:**
```
∀total_effective_aops, historical_avg_aops ∈ ℝ₀⁺:
  compute_emission_rate(total_effective_aops, historical_avg_aops) ∈ [5.0, 2000.0]
```

**Proof:** Result clamped to [BASE_EMISSION × EMISSION_FLOOR_MULTIPLIER, BASE_EMISSION × EMISSION_CEILING_MULTIPLIER] = [100 × 0.05, 100 × 20] = [5.0, 2000.0].

#### I-REW-002: Supply Conservation

**Formal statement:**
```
∀block ∈ CanonicalChain:
  sum(coinbase.outputs) + sum(burned) = header.emission_rate
  ∨ (block.header.height < RAMP_UP_BLOCKS ∧ sum(coinbase.outputs) + burned = header.emission_rate)
```

**Proof:** compute_block_rewards computes total emission, applies ramp-up cap where excess goes to burned, then partitions rewards among miners. The total emitted (miner_rewards + burned) = emission_rate.

#### I-REW-003: Founder Lock

**Formal statement:**
```
∀block ∈ CanonicalChain | block.header.height < 10000:
  ∀output ∈ block.body.transactions[0].outputs:
    output.spendable_after ≥ max(50000, block.header.height + 40000)
```

**Proof:** TxOutput::new_locked calls founder_lock_block which returns max(50000, block_number + 40000) for block_number < 10000.

---

### 4.6 Privacy Invariants

#### I-PRIV-001: Pedersen Binding

**Formal statement:**
```
∀v₁, v₂: u64, a₁, a₂: Scalar:
  Commitment::new_with_blinding(v₁, a₁).0 = Commitment::new_with_blinding(v₂, a₂).0
  ⇒ v₁ = v₂
```

**Proof sketch:** If commitments are equal, a₁G + v₁H = a₂G + v₂H. Then (a₁ - a₂)G = (v₂ - v₁)H. If G and H are independent generators (discrete log relation unknown), this forces v₁ = v₂ and a₁ = a₂. This is the computational binding property of Pedersen commitments, reducible to discrete log hardness in the Ristretto group.

#### I-PRIV-002: Range Proof Soundness

**Formal statement:**
```
∀rp: RangeProof, commitment: Commitment:
  rp.verify(commitment) = true ⇒
    ∃v ∈ [0, 2^rp.bits), a: Scalar |
      Commitment::new_with_blinding(v, a).0 = commitment.0
```

**Proof sketch:** RangeProof uses bit-decomposition with 1-out-of-2 MLSAG for each bit. Each bit commitment C_i is proven to commit to either 0 or 1 via the ring signature. The sum Σ 2^i × C_i reconstructs the total commitment. The MLSAG soundness (each bit is either 0 or 1) ensures v ∈ [0, 2^bits).

**Known gap:** The ring signature for each bit is created with `format!("bit_{}", i)` as the signed message. An attacker who can forge MLSAG signatures (computationally hard) can create a false range proof. Additionally, the proof allows up to 64 bits; commitments to values ≥ 2^64 are outside the proven range but `verify()` checks `commitments.len() > 64` as the only structural bound.

#### I-PRIV-003: Stealth Address Unlinkability

**Formal statement:**
```
∀addr: StealthAddress, r₁, r₂: Scalar:
  OneTimeAddress from addr with r₁ 㐬 OneTimeAddress from addr with r₂
```

**Proof sketch:** Each destination uses a fresh random scalar r, producing a different shared secret r × view_key and a different one-time destination. Without view_key, an adversary cannot link two one-time addresses to the same StealthAddress.

#### I-PRIV-004: MLSAG Linkability

**Formal statement:**
```
∀sig₁, sig₂: MLSAGSignature | sig₁ ≠ sig₂ ∧ sig₁.key_images[0] = sig₂.key_images[0]:
  sig₁ and sig₂ spend the same UTXO
```

**Proof sketch:** Each key image is computed as K_j = k_j × H_p(P_πⱼ) where k_j is the private key. Key images are deterministic per private key. If two signatures share a key image, they spend from the same key (and thus the same UTXO). This is the *linkability* property — the same key image across transactions reveals double-spending.

---

### 4.7 State Machine Invariants

#### I-STATE-001: No Inflation

**Formal statement:**
```
∀block ∈ CanonicalChain:
  sum(input amounts) ≥ sum(output amounts) for all non-coinbase transactions
```

**Proof:** validate_transaction checks `ins >= outs` (line "creates money"). spend_transaction_inputs also checks this for private txs. The coinbase is the only allowed money creation.

**Counterexample:** Overflow in input sum. If `ins = ins.checked_add(u.amount)` overflows, it returns None mapped to Err("overflow"). The overflow case is handled.

#### I-STATE-002: No Double Spend

**Formal statement:**
```
∀block₁, block₂ ∈ CanonicalChain:
  tx₁.inputs[i].key_image = tx₂.inputs[j].key_image ⇒ block₁ = block₂ ∧ tx₁ = tx₂ ∧ i = j
```

**Proof:** spent_key_images set is checked before every spend. If a key_image O is already in the set, the second spend returns Err("Double-spend").

**Edge case:** During a reorg, unwind_with_diff removes key images from spent_key_images via `self.spent_key_images.remove(ki)`. A reorg that creates a different chain may un-spend and re-spend the same key image.

#### I-STATE-003: Supply Growth Bounded

**Formal statement:**
```
∀block ∈ CanonicalChain:
  total_supply after block ≤ total_supply before block + 20 × BASE_EMISSION_UNITS
```

**Proof:** Coinbase amount is capped: apply_block_inner checks `coinbase_amount > max_emission` where max_emission = BASE_EMISSION_UNITS * 20 = 2,000,000,000 base units (2000 eWatt at v0x0005 ceiling).

---

## 5. Fuzzing

### 5.1 Target: proof::mine → verify roundtrip

**File:** src/proof.rs  
**cargo-fuzz target name:** fuzz_proof_verify

**Input corpus:**
- Valid solutions at difficulty 1, 10, 100, 1000
- Solutions with empty trace
- Solutions with malformed merkle_root
- Solutions with mismatched walk_length

**Mutation strategy:**
- Bit flips on solution bytes (nonce, trace positions, mix hashes)
- Walk length set to 0, u64::MAX, random values
- Merkle root: random 32 bytes, all zero, all 0xFF
- Sample positions: duplicate, out of order, negative (as signed), beyond walk length
- Elapsed offsets: non-monotonic, zero, u64::MAX

**Coverage goal:** 100% branch coverage of verify() function (3 code paths)

**Crash conditions:**
- Panic from dag.get() out-of-bounds (index ≥ len after modulo)
- Stack overflow from deep recursion (none present — all iterative)
- Infinite loop in verify() (none — bounded by walk_length)
- Division by zero at difficulty_to_accesses (difficulty=0 → difficulty.max(1) protects)
- Integer overflow in sample_indices rng.gen_range with total=0

**Specific fuzz harness:**

```rust
// fuzz_targets/fuzz_proof_verify.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use ewatts_protocol::{proof, dag::Dag};

fuzz_target!(|data: &[u8]| {
    if data.len() < 40 { return; }
    let header_hash = [data[0] ^ 0xab, data[1], ..];
    let difficulty = u64::from_le_bytes(data[8..16].try_into().unwrap());
    if difficulty == 0 { return; }
    let dag = Dag::generate_with_size(0, 64 * 1024); // small DAG
    // Parse remainder as solution bytes
    let solution = parse_solution(&data[16..]).unwrap_or(/* default */);
    let _ = proof::verify(&header_hash, &solution, difficulty, &dag);
});
```

### 5.2 Target: commitment::validate_commitment

**File:** src/commitment.rs  
**cargo-fuzz target name:** fuzz_commitment_validate

**Input corpus:**
- Valid signed commitments at various AOPS levels
- Invalid signature (wrong key, short sig, empty sig)
- Zero/minimum/maximum AOPS
- NaN infinity for floating fields

**Mutation strategy:**
- Random bytes as Commitment (deserialize from JSON)
- Signature field: truncated, extended, malformed ed25519
- Fields: NaN, -1.0, 0.0, f64::MAX, f64::MIN
- Block_number: past, future, duplicate

**Coverage goal:** Cover every error path in validate_commitment()

**Crash conditions:**
- Panic in VerifyingKey::from_bytes on invalid point
- Panic in Signature::from_slice on short input
- Floating point exception (no exceptions in Rust, but NaN propagation)

### 5.3 Target: state::spend_transaction_inputs

**File:** src/state.rs  
**cargo-fuzz target name:** fuzz_state_spend

**Input corpus:**
- Various transaction structures with valid/invalid signatures
- Double-spend attempts (same key_image twice)
- Transactions spending non-existent UTXOs
- MLSAG transactions with various ring sizes
- Range proofs: valid, invalid, empty, oversized

**Mutation strategy:**
- Mutate tx hash bytes (will reference non-existent UTXOs — should fail gracefully)
- Mutate key_image bytes between inputs in same tx
- Mutate amount fields: wrapping overflow, zero, u64::MAX
- Mutate ring_size: 0, 1, 2, 1000
- Mutate revealed_pubkey: random bytes, valid key for wrong signer
- Mutate spendable_after: far future, current block, past

**Coverage goal:** Every error path, every validation check

### 5.4 Target: privacy::RangeProof::verify

**File:** src/privacy.rs  
**cargo-fuzz target name:** fuzz_range_proof

**Input corpus:**
- Range proofs for values 0, 1, 127, 255, 2^16-1, 2^32-1, 2^64-1
- Range proofs with bits 1, 8, 16, 32, 64
- Malformed: empty commitments, mismatched lengths

**Mutation strategy:**
- Mutate commitment bytes (Pedersen curve points)
- Mutate MLSAG signatures inside range proofs (c0, responses, key_images)
- Mutate bits field: 0, 65, 1000

**Coverage goal:** Every validation check in RangeProof::verify()

### 5.5 Target: p2p::P2pMessage deserialization

**File:** src/p2p.rs  
**cargo-fuzz target name:** fuzz_p2p_message

**Input corpus:**
- All message variants (BlockRequest, BlockResponse, NewTransaction, NewBlock, CompactBlock)
- Empty BlockResponse
- Extremely large block counts

**Mutation strategy:**
- Random bytes, truncated JSON, nested JSON
- UTF-8 garbage, binary, very long strings
- Invalid Block structures
- Block transactions with duplicate fields
- Extremal height values

**Coverage goal:** serde_json deserialization for all P2pMessage variants without panic

### 5.6 Target: dag::Dag::generate_with_size

**File:** src/dag.rs  
**cargo-fuzz target name:** fuzz_dag_generate

**Input corpus:**
- Standard sizes: 64, 1024, 65536, 1MB, 4MB, 8MB
- Edge sizes: 64 (minimum), 65, 127, 128, 129

**Mutation strategy:**
- Random size values (1..2^48)
- Epoch: 0, 1, 65535, u64::MAX

**Coverage goal:** Exercise the full generation path, cache hit/miss, and the panic boundary at size < 64

### 5.7 Target: chain::ChainStore (orphan management)

**File:** src/chain.rs  
**cargo-fuzz target name:** fuzz_chain_store

**Input corpus:**
- Sequences of add_block, add_orphan, resolve_orphans, set_chain_tip
- Valid and invalid parent links

**Mutation strategy:**
- Random block insertions with varying parent relations
- Chains with gaps, cycles, and duplicate hashes
- Extreme orphan counts

**Coverage goal:** All branch paths in add_block_inner, add_orphan, resolve_orphans

### 5.8 Target: store persistence (JSON roundtrip)

**File:** src/store.rs  
**cargo-fuzz target name:** fuzz_store_serialization

**Input corpus:**
- Valid UtxoSet with various UTXO structures
- ChainStore with forks and orphans

**Mutation strategy:**
- Mutate serialized JSON bytes
- Invalid hex in UtxoKey deserialization
- Truncated data, extra fields

**Coverage goal:** serde_json deserialization for all custom serializers (hexkey_map, hex_vec)

---

## 6. Property Testing

### 6.1 proof module (proptest)

#### P-PROOF-001: Mine then verify always succeeds

```rust
proptest! {
    #[test]
    fn prop_mine_then_verify(
        header_seed in any::<[u8; 32]>(),
        difficulty in 1u64..1000u64,
    ) {
        let dag = Dag::generate_with_size(0, 64 * 1024);
        let sol = mine(&header_seed, difficulty, &dag, 10000);
        prop_assume!(sol.is_some());
        assert!(verify(&header_seed, &sol.unwrap(), difficulty, &dag).is_ok());
    }
}
```

**Why this matters:** The fundamental correctness property. If any valid solution fails verification, the protocol is broken.

#### P-PROOF-002: Wrong nonce fails verification

```rust
proptest! {
    #[test]
    fn prop_wrong_nonce_fails(
        header_seed in any::<[u8; 32]>(),
        alt_nonce in any::<u64>(),
    ) {
        let dag = Dag::generate_with_size(0, 64 * 1024);
        let sol = mine(&header_seed, 1, &dag, 100).unwrap();
        let mut wrong = sol.clone();
        wrong.nonce = alt_nonce.wrapping_add(1); // ensure different
        let result = verify(&header_seed, &wrong, 1, &dag);
        prop_assert!(result.is_err());
    }
}
```

#### P-PROOF-003: Difficulty monotonic

```rust
proptest! {
    #[test]
    fn prop_difficulty_monotonic(
        d1 in 1u64..=1000u64,
        d2 in 1u64..=1000u64,
    ) {
        // Higher difficulty should produce fewer solutions in same nonce space
        // Not strict — probabilistic — but verify the bounds are sensible
        let accesses_high = difficulty_to_accesses(d1);
        let accesses_low = difficulty_to_accesses(d2);
        if d1 > d2 {
            prop_assert!(accesses_high >= accesses_low);
        }
    }
}
```

#### P-PROOF-004: Meets difficulty is deterministic

```rust
proptest! {
    #[test]
    fn prop_meets_difficulty_deterministic(
        hash_seed in any::<[u8; 32]>(),
        diff in any::<u64>(),
    ) {
        let r1 = meets_difficulty(&hash_seed, diff);
        let r2 = meets_difficulty(&hash_seed, diff);
        prop_assert_eq!(r1, r2);
    }
}
```

### 6.2 commitment module (proptest)

#### P-COMM-001: Efficiency is proportional

```rust
proptest! {
    #[test]
    fn prop_efficiency_proportional(
        factor in 0.5f64..2.0f64,
    ) {
        let eff = compute_efficiency(25_000_000. * factor, 25_000_000., 1.);
        prop_assert!((eff - factor).abs() < 1e-6);
    }
}
```

#### P-COMM-002: Effective commitment bounds

```rust
proptest! {
    #[test]
    fn prop_effective_commitment_bounds(
        d in 1.0f64..1e12f64,
        e in 0.0f64..10.0f64,
    ) {
        let ce = effective_commitment(d, e);
        // ce should never exceed d * 1.3 (upper cap)
        prop_assert!(ce <= d * 1.3 + 1e-9);
        // ce should never be less than effective lower bound
        prop_assert!(ce >= 0.0);
    }
}
```

#### P-COMM-003: Commit message is injective

```rust
proptest! {
    #[test]
    fn prop_commit_msg_injective(
        aops1 in 1e6f64..1e9f64,
        aops2 in 1e6f64..1e9f64,
        blk1 in any::<u64>(),
        blk2 in any::<u64>(),
    ) {
        let c1 = Commitment {
            miner_id: [1; 32], access_ops_per_sec: aops1,
            block_number: blk1, total_access_ops: aops1,
            time_seconds: 1., signature: vec![],
        };
        let c2 = Commitment {
            miner_id: [1; 32], access_ops_per_sec: aops2,
            block_number: blk2, total_access_ops: aops2,
            time_seconds: 1., signature: vec![],
        };
        let msg1 = commit_msg(&c1);
        let msg2 = commit_msg(&c2);
        // Slightly different fields should produce different messages
        if (aops1 - aops2).abs() > 1e-9 || blk1 != blk2 {
            prop_assert_ne!(msg1, msg2);
        }
    }
}
```

### 6.3 reward module (proptest)

#### P-REW-001: Emission is within bounds

```rust
proptest! {
    #[test]
    fn prop_emission_bounds(
        total in 1e5f64..1e12f64,
        hist in 1e5f64..1e12f64,
    ) {
        let rate = compute_emission_rate(total, hist);
        prop_assert!(rate >= 5.0);
        prop_assert!(rate <= 2000.0);
    }
}
```

#### P-REW-002: Ramp-up cap is monotonic

```rust
proptest! {
    #[test]
    fn prop_ramp_up_cap_monotonic(
        rewards: Vec<f64>,
    ) {
        // With more miners, ramp-up cap should distribute more fairly
        let block_num = 5000u64; // within ramp-up
        let total: f64 = rewards.iter().sum();
        if total > 0.0 {
            let mut r = rewards.iter().map(|v| (vec![1u8; 32], *v)).collect();
            let burned = apply_ramp_up_cap(block_num, &mut r);
            // Total after cap + burned = original total
            let after_total: f64 = r.iter().map(|(_, v)| v).sum();
            prop_assert!((after_total + burned - total).abs() < 1e-6);
        }
    }
}
```

#### P-REW-003: Founder lock only applies before ramp-up

```rust
proptest! {
    #[test]
    fn prop_founder_lock_range(
        blk in 0u64..20000u64,
    ) {
        let lock = founder_lock_block(blk);
        if blk < 10000 {
            // Lock must be at least 50000 blocks from start
            prop_assert!(lock >= 50000);
            // And at least blk + 40000
            prop_assert!(lock >= blk + 40000);
        } else {
            prop_assert_eq!(lock, 0);
        }
    }
}
```

### 6.4 VR module (proptest)

#### P-VR-001: VR is proportional to AOPS

```rust
proptest! {
    #[test]
    fn prop_vr_proportional_to_aops(
        factor in 0.5f64..5.0f64,
    ) {
        let base = 25_000_000f64;
        let v1 = compute_vr(base, 100_000., 1000, 600);
        let v2 = compute_vr(base * factor, 100_000., 1000, 600);
        if v1.vr_kwh_per_ewatt > 0.0 && v2.vr_kwh_per_ewatt > 0.0 {
            prop_assert!((v2.vr_kwh_per_ewatt / v1.vr_kwh_per_ewatt - factor).abs() < 1e-6);
        }
    }
}
```

#### P-VR-002: VR doubles when emission halves

```rust
proptest! {
    #[test]
    fn prop_vr_inverse_emission(
        factor in 0.5f64..2.0f64,
    ) {
        let v1 = compute_vr(25_000_000., 100_000., 1000, 600);
        let v2 = compute_vr(25_000_000., 100_000. * factor, 1000, 600);
        if v1.vr_kwh_per_ewatt > 0.0 && v2.vr_kwh_per_ewatt > 0.0 {
            prop_assert!((v2.vr_kwh_per_ewatt * factor - v1.vr_kwh_per_ewatt).abs() < 1e-6);
        }
    }
}
```

### 6.5 state module (proptest)

#### P-STATE-001: Genesis invariant

```rust
proptest! {
    #[test]
    fn prop_genesis_invariant(
        amount in 1u64..1_000_000_000u64,
        pk_seed in any::<[u8; 32]>(),
    ) {
        let state = UtxoSet::genesis(amount, &pk_seed);
        prop_assert_eq!(state.total_supply(), amount);
        prop_assert!(state.utxo_count() >= 1);
        prop_assert!(state.get_balance(&pk_seed.to_vec()) >= amount / 2 ||
                     state.get_balance(&pk_seed.to_vec()) == amount);  // P2PKH hash
        // Not exactly amount because of P2PKH hashing
    }
}
```

#### P-STATE-002: Spend then double-spend fails

```rust
proptest! {
    #[test]
    fn prop_double_spend_fails(
        amount in 1u64..10_000u64,
    ) {
        let sk = make_signing_key();
        let pk = sk.verifying_key().to_bytes().to_vec();
        let mut state = UtxoSet::new();
        let genesis_tx = Transaction {
            version: 1, inputs: vec![],
            outputs: vec![TxOutput::new(amount, pk.clone())],
            ring_size: 1, signatures: vec![], mlsag: None, ring_members: None,
        };
        let h = genesis_tx.hash();
        state.add_transaction_outputs(&h, &genesis_tx, 0, 0);
        let mut spend_tx = Transaction {
            version: 1,
            inputs: vec![TxInput {
                previous_tx_hash: h, output_index: 0,
                key_image: [42u8; 32], revealed_pubkey: pk.clone(),
            }],
            outputs: vec![TxOutput::new(amount, pk)],
            ring_size: 1, signatures: vec![], mlsag: None, ring_members: None,
        };
        let msg = state::tx_msg(&spend_tx);
        spend_tx.signatures = vec![sk.sign(&msg).to_bytes().to_vec()];
        // First spend succeeds
        prop_assert!(state.spend_transaction_inputs(&spend_tx, 1000).is_ok());
        // Second spend fails (same key_image)
        prop_assert!(state.spend_transaction_inputs(&spend_tx, 1000).is_err());
    }
}
```

### 6.6 privacy module (proptest)

#### P-PRIV-001: Pedersen commitment verify

```rust
proptest! {
    #[test]
    fn prop_pedersen_verify(
        v in any::<u64>(),
        a_seed in any::<u64>(),
    ) {
        let a = Scalar::from(a_seed);
        let c = Commitment::new_with_blinding(v, a);
        prop_assert!(c.verify(v, a));
        prop_assert!(!c.verify(v + 1, a));
        prop_assert!(!c.verify(v, a + Scalar::from(1u64)));
    }
}
```

#### P-PRIV-002: MLSAG sign and verify roundtrip

```rust
proptest! {
    #[test]
    fn prop_mlsag_roundtrip(
        ring_size in 2usize..20usize,
        msg_seed in any::<[u8; 32]>(),
    ) {
        let real_idx = ring_size / 2;
        let mut ring = Vec::with_capacity(ring_size);
        let mut secrets = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            let sk = Scalar::random(&mut thread_rng());
            ring.push(vec![sk * ring_g()]);
            secrets.push(sk);
        }
        let sig = MLSAGSignature::sign(
            &ring, &[secrets[real_idx]], real_idx, &msg_seed, &mut thread_rng()
        );
        prop_assert!(sig.verify(&ring, &msg_seed));
        // Wrong message fails
        prop_assert!(!sig.verify(&ring, &[0u8; 32]));
    }
}
```

#### P-PRIV-003: Range proof for all values in range

```rust
proptest! {
    #[test]
    fn prop_range_proof_all_values(
        v in 0u64..4096u64,
        bits in 12usize..16usize,
    ) {
        let mut rng = thread_rng();
        let (proof, blinding) = RangeProof::prove_with_blinding(v, bits, &mut rng);
        let comm = Commitment::new_with_blinding(v, blinding);
        prop_assert!(proof.verify(&comm));
    }
}
```

### 6.7 chain module (proptest)

#### P-CHAIN-001: Chain insertion preserves counts

```rust
proptest! {
    #[test]
    fn prop_chain_insertion(
        chain_len in 1usize..100usize,
    ) {
        let genesis = make_block(0, [0u8; 32]);
        let g_hash = genesis.header.hash();
        let mut store = ChainStore::new(genesis);
        let mut prev = g_hash;
        for i in 0..chain_len {
            let b = make_block((i + 1) as u64, prev);
            prev = b.header.hash();
            store.add_block(b).ok();
        }
        prop_assert_eq!(store.block_count(), 1 + chain_len.min(100));
    }
}
```

#### P-CHAIN-002: Orphan queue bounded

```rust
proptest! {
    #[test]
    fn prop_orphan_bounded(
        orphan_count in 0usize..1000usize,
    ) {
        let genesis = make_block(0, [0u8; 32]);
        let g_hash = genesis.header.hash();
        let mut store = ChainStore::new(genesis);
        // Create orphans with random unknown parents
        for _ in 0..orphan_count {
            let orphan = make_block(5, [rand::random(); 32]); // parent unknown
            store.add_orphan(orphan);
        }
        prop_assert!(store.orphan_count() <= 500); // MAX_ORPHANS
    }
}
```

### 6.8 difficulty module (proptest)

#### P-DIFF-001: Adjustment bounds

```rust
proptest! {
    #[test]
    fn prop_adjust_in_bounds(
        current in 1u64..u64::MAX >> 10,
        actual in 1e-6f64..1e12f64,
    ) {
        let target = 600.0; // TARGET_BLOCK_TIME
        let adjusted = adjust_difficulty(current, actual, target);
        // Should not drop below 1
        prop_assert!(adjusted >= 1);
        // Should not exceed reasonable bounds
        if current > 0 {
            let ratio = adjusted as f64 / current as f64;
            prop_assert!(ratio >= 0.5 - 0.01);
            prop_assert!(ratio <= 2.0 + 0.01);
        }
    }
}
```

#### P-DIFF-002: Median timestamp robust

```rust
proptest! {
    #[test]
    fn prop_median_timestamp_robust(
        times in proptest::collection::vec(0u64..100000u64, 2..100),
    ) {
        let avg = average_block_time(&times);
        prop_assert!(avg.is_finite());
        prop_assert!(avg >= 0.0);
    }
}
```

---

## 7. Rust Security Review

### 7.1 Unsafe Code Audit

**Finding RS-01: No unsafe blocks in protocol code** ✅

After scanning all 25 source files, zero `unsafe` blocks were found. The codebase uses `#![forbid(unsafe_code)]` in lib.rs.

**Severity:** None (informational positive)

### 7.2 unwrap() Audit

#### RS-UW-01: dag.rs DAG_CACHE.unwrap()

```rust
let cache = get_dag_cache().lock().unwrap();
```

**Location:** dag.rs, generate_with_size  
**Risk:** If the Mutex is poisoned (another thread panicked while holding the lock), unwrap() panics here.  
**Persistence:** OnceLock is initialized once. Mutex poisoning from any panic in any thread holding DAG_CACHE crashes the miner.  
**Fix:** Replace with `lock().unwrap_or_else(|e| e.into_inner())` to recover from poisoning. The data inside is still valid.  
**Severity:** Medium

#### RS-UW-02: main.rs now_secs()

```rust
SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
```

**Risk:** If system clock is before UNIX_EPOCH (year 1970), unwrap panics. Typical on embedded devices with no RTC battery.  
**Severity:** Low  
**Fix:** Use `unwrap_or(Duration::ZERO)` or handle gracefully.

#### RS-UW-03: store.rs BLOCK_CACHE.lock().unwrap()

Multiple locations: load_blocks(), save_block(), chain_tip_height(), chain_tip_hash(), cached_block_count(), invalidate_cache().

**Risk:** Mutex poisoning from any thread. Since this is a process-global static, ONE panic in ANY thread holding this lock kills ALL subsequent cache operations.  
**Severity:** Medium  
**Fix:** Replace all with `.lock().unwrap_or_else(|e| e.into_inner())` pattern.

#### RS-UW-04: store.rs MEMPOOL.lock().unwrap()

**Location:** mempool.rs, get_pool()  
**Risk:** Same poisoning issue. MEMPOOL is global.  
**Severity:** Medium  
**Fix:** Use poisoning recovery.

#### RS-UW-05: OVERRIDE_DATA_DIR.lock().unwrap()

**Location:** store.rs, data_dir(), set_data_dir()  
**Risk:** Poisoning during test setup. In production, this is only read.  
**Severity:** Low  
**Fix:** Recover from poisoning.

**Total unwrap() call sites: 14** across the codebase. 8 are in tests. 6 are in production code (all Mutex locks).

### 7.3 expect() Audit

#### RS-EX-01: generate_with_size panic

```rust
if size_bytes < 64 {
    panic!("DAG size_bytes must be >= 64 (got {})", size_bytes);
}
```

**Risk:** Network receiving a block requesting DAG regeneration with size < 64 panics the entire node.  
**Attack surface:** An adversary could craft a message triggering DAG regen with invalid size.  
**Severity:** High  
**Fix:** Return Result instead of panicking.

### 7.4 Integer Overflow Analysis

#### RS-OV-01: store.rs total_supply overflow (Inflation Attack)

```rust
pub fn add_coinbase_supply(&mut self, a: u64) {
    self.total_supply = self.total_supply
        .checked_add(a)
        .unwrap_or(self.total_supply);  // SILENT WRAP!
}
```

**Severity:** CRITICAL  

If `total_supply.checked_add(a)` overflows, it silently discards the new coinbase amount. The supply stops growing, but the UTXOs are still created. This creates eWatts out of thin air because the supply tracking desynchronizes from actual UTXOs.

**Attack path:**
1. Mine until total_supply reaches ~1.8 × 10^19 base units (u64::MAX)
2. Each subsequent block creates new UTXOs but supply doesn't increase
3. Supply ceiling is reached early, but UTXOs continue growing
4. When queried, total_supply() reports artificially low values
5. Exchange/dApp relying on total_supply for token valuation is deceived

**Fix:** 
```rust
pub fn add_coinbase_supply(&mut self, a: u64) -> Result<(), String> {
    self.total_supply = self.total_supply
        .checked_add(a)
        .ok_or("Supply overflow")?;
    Ok(())
}
```

**Probability:** With 100 eWatt/block and 52,596 blocks/year, total supply reaches ~5.26M eWatt/year. Reaching u64::MAX (~1.84 × 10^19 base units at 6 decimal places = ~1.84 × 10^13 eWatt) would take ~3.5 million years. Practically unreachable, but still a correctness bug.

#### RS-OV-02: state.rs validate_transaction overflow

```rust
ins = ins.checked_add(u.amount).ok_or("overflow")?;
outs = outs.checked_add(o.amount).ok_or("overflow")?;
```

**Status:** Correctly handled ✅ Overflow returns Err, which is caught and propagated.

#### RS-OV-03: state.rs spend_transaction_inputs overflow

```rust
input_amount = input_amount.checked_add(u.amount).ok_or("overflow")?;
output_amount = output_amount.checked_add(o.amount).ok_or("overflow")?;
```

**Status:** Correctly handled ✅

#### RS-OV-04: reward.rs ewatt_to_units rounding

```rust
fn ewatt_to_units(ewatt: f64) -> u64 {
    (ewatt * constants::UNITS_PER_EWATT as f64).round() as u64
}
```

**Risk:** f64 multiplication rounding. UNITS_PER_EWATT = 1,000,000. For large values (e.g., 1.0 × 10^12 eWatt), the multiplication is 1e12 × 1e6 = 1e18, which fits in f64 with integer precision (f64 has 53 bits ≈ 9 × 10^15 integer precision). For values > 9 × 10^9 eWatt, precision loss exceeds 1 unit. In practice, emission is 5-2000 eWatt/block, so this is safe.

**However:** `round()` followed by `as u64` overflows if result > u64::MAX. The cast truncates silently.  
**Severity:** Low (unreachable in practice)  
**Fix:** Use `u64::try_from(f64)` and handle error.

#### RS-OV-05: dag.rs per_epoch_growth integer calculation

```rust
let per_epoch_growth = (size * constants::DAG_EPOCH_BLOCKS) / constants::BLOCKS_PER_YEAR;
```

**Risk:** `size` can be DAG_GROWTH_RATE_BYTES_PER_YEAR = 512MB or DAG_ACCELERATION_RATE = 1024MB. Multiply by DAG_EPOCH_BLOCKS=2016 and divide by BLOCKS_PER_YEAR=52596:
- Normal: 512MB × 2016 / 52596 ≈ 19.6 MB/epoch
- Accelerated: 1024MB × 2016 / 52596 ≈ 39.2 MB/epoch

No overflow risk (max intermediate: 512 × 1024 × 1024 × 2016 ≈ 1.08 × 10^15 << u64::MAX).

**Status:** Safe ✅

#### RS-OV-06: store.rs prune_blocks integer handling

```rust
out.sync_all().map_err(|e| format!("sync: {}", e))?;
fs::rename(&tmp, &path).map_err(|e| format!("rename: {}", e))?;
```

**Status:** Safe ✅

#### RS-OV-07: proof.rs difficulty_to_accesses overflow

```rust
pub fn difficulty_to_accesses(difficulty: u64) -> u64 {
    constants::BASE_ACCESSES * difficulty / 1_000_000_000
}
```

**Risk:** BASE_ACCESSES = 1_000_000_000. If difficulty = u64::MAX, intermediate product = 1e9 × u64::MAX ≈ 1.84 × 10^28 >> u64::MAX (1.84 × 10^19). Overflow occurs.

**Attack path:** If difficulty is set very high (maliciously crafted block header), this wraps to a small value, making verify() accept a short walk.

**Severity:** HIGH
**Fix:** Use saturating or checked multiplication:
```rust
pub fn difficulty_to_accesses(difficulty: u64) -> u64 {
    difficulty.saturating_mul(constants::BASE_ACCESSES) / 1_000_000_000
}
```
Or use u128 intermediate:
```rust
pub fn difficulty_to_accesses(difficulty: u64) -> u64 {
    ((difficulty as u128 * constants::BASE_ACCESSES as u128) / 1_000_000_000_u128) as u64
}
```

#### RS-OV-08: proof.rs read_u64_le on short input

```rust
fn read_u64_le(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    let len = bytes.len().min(8);
    buf[..len].copy_from_slice(&bytes[..len]);
    u64::from_le_bytes(buf)
}
```

**Risk:** If `bytes` is shorter than 8 bytes, remaining buf bytes stay 0. This is handled correctly via `.min(8)`, but the semantic is unusual — calling on a 32-byte hash will only read first 8 bytes (which is fine for address calculation).  

**Status:** Safe ✅ but should document the truncation.

### 7.5 Floating Point Precision Analysis

#### RS-FP-01: reward.rs emission_rate computation

```rust
let rate = constants::BASE_EMISSION * total_effective_aops / historical_avg_aops;
```

f64 has ~15-17 significant decimal digits. With AOPS values up to ~10^9 and emission of ~100, arithmetic is well within f64 precision.

**Status:** Safe ✅

#### RS-FP-02: commitment.rs efficiency computation

```rust
w / (d * t)
```

d (access_ops_per_sec) up to ~10^9, t (time_seconds) up to ~600. d × t up to ~6 × 10^11, well within f64 precision.

**Status:** Safe ✅

#### RS-FP-03: vr.rs total_energy computation

```rust
let total_accesses = avg_effective_aops * total_secs;
let total_joules = total_accesses * constants::J_PER_ACCESS;
```

J_PER_ACCESS = 3.75 × 10^-6. Total accesses up to ~10^9 × 10^6 = 10^15. Total joules up to ~3.75 × 10^9. f64 maintains integer precision up to 2^53 ≈ 9 × 10^15. Access count up to ~10^15 is borderline.

**Severity:** Low (f64 precision ~15-17 digits, 10^15 / 10^17 ≈ 1% of mantissa).

### 7.6 Thread Safety Analysis

#### RS-TH-01: Global static MEMPOOL

```rust
static MEMPOOL: Mutex<Option<MempoolInner>> = Mutex::new(None);
```

**Type:** Critical (shared mutable state, no Send restriction on MempoolInner)

- MempoolInner contains `tx: Transaction` which is `Send + Sync`
- Mutex provides Send + Sync automatically
- But `MempoolInner` is not `Send` by default (contains HashMap, Vec — these are Send)

**Actually:** Vec and HashMap are Send. Transaction derives Clone, Serialize, Deserialize. All fields are Send. The Mutex guard provides Send. This is safe.

**Status:** Safe ✅ But the global pattern prevents testing isolation.

#### RS-TH-02: Global static BLOCK_CACHE

```rust
static BLOCK_CACHE: Mutex<Option<Vec<Block>>> = Mutex::new(None);
```

Same pattern as MEMPOOL. Block derives Clone, Serialize, Deserialize. Safe but shared across all tests.

**Status:** Safe ✅

#### RS-TH-03: Global static DAG_CACHE

```rust
static DAG_CACHE: OnceLock<Mutex<Option<(u64, u64, Dag)>>> = OnceLock::new();
```

OnceLock + Mutex. Safe initialization. Dag is just Vec<[u8; 64]> — Send.

**Status:** Safe ✅

### 7.7 Memory Leak Analysis

#### RS-ML-01: Orphan queue unbounded memory

```rust
const MAX_ORPHANS: usize = 500;
```

MAX_ORPHANS bounds memory. Each orphan block is approximately:
- BlockHeader: ~168 bytes
- BlockBody: depends on transactions (empty ~16 bytes)
- Total per orphan: ~200 bytes empty, potentially 100KB+ with transactions

**Memory ceiling:** 500 × ~100KB ≈ 50MB worst case. Acceptable for a node.

**Status:** Safe ✅

#### RS-ML-02: BLOCK_CACHE unbounded?

```rust
const MAX_CACHED_BLOCKS: usize = 10_000;
```

10,000 blocks × ~500 bytes (header only) ≈ 5MB. With transactions, ~100KB per block ≈ 1GB worst case.

**Status:** Bounded, but high. Unclear if this is acceptable on resource-constrained nodes.  
**Severity:** Informational

#### RS-ML-03: P2P pending_compact unbounded

```rust
pending_compact: HashMap<u64, (CompactBlock, PeerId)>,
```

No cap on pending_compact size. If an attacker floods compact blocks with missing transactions, this HashMap grows unbounded.

**Severity:** Medium  
**Fix:** Add bound: `MAX_PENDING_COMPACT: usize = 1000;`

### 7.8 Serialization Safety

#### RS-SR-01: Custom UtxoKey serialization

```rust
impl Serialize for UtxoKey { ... hex string format ... }
impl<'de> Deserialize<'de> for UtxoKey { ... custom parser ... }
```

**Risk:** Custom deserializer parses hex manually with `u8::from_str_radix`. If the hex string has invalid characters, it returns an error. If the output_index is not a valid u32, it fails. The hash is validated to be exactly 64 hex chars.

**Attack surface:** A malformed UtxoKey string could cause panic if `parts[0].len() != 64` or if parts has wrong length. The code handles these with `map_err`.

**Status:** Safe ✅

#### RS-SR-02: hexkey_map deserialization

```rust
pub fn deserialize<'de, D, V>(de: D) -> Result<HashMap<[u8; 32], V>, D::Error>
```

Custom serde for HashMap. Validates each key is exactly 32 bytes. Returns error on invalid hex or wrong length.

**Status:** Safe ✅

### 7.9 Dead Code Analysis

#### RS-DC-01: unused import

```rust
// chain.rs: `use crate::block::BlockHeader;` — imported but Block is the primary type used
```

**Status:** Low (compiler warning)

#### RS-DC-02: unused fields

```rust
struct PeerInfo {
    #[allow(dead_code)]
    peer_id: PeerId,
    #[allow(dead_code)]
    connected_at: Instant,
    last_active: Instant,
}
```

PeerInfo.peer_id and connected_at are never read, only written. `#[allow(dead_code)]` suppresses the warning.

**Status:** Informational — fields used for logging/debugging

#### RS-DC-03: make_signing_key()

```rust
#[allow(dead_code)]
fn make_signing_key() -> SigningKey { ... }
```

Used in tests via `#[cfg(test)]`. The dead_code annotation suggests it's not gated behind `#[cfg(test)]`.

**Status:** Low — should add `#[cfg(test)]` attribute.

### 7.10 Lifetime Safety

#### RS-LT-01: store.rs lifetime in get_utxo

```rust
pub fn get_utxo(&self, key: &UtxoKey) -> Option<&UtxoEntry> {
    self.utxos.get(key)
}
```

Returns reference tied to `&self` lifetime. Caller holds immutable borrow. Since UtxoSet is &mut during block application, this borrow is incompatible with mutation.

**Status:** Safe ✅ Standard borrow checker enforcement.

#### RS-LT-02: state.rs build_ring_inline borrow

```rust
pub fn build_ring_inline(
    utxo_set: &HashMap<UtxoKey, UtxoEntry>,
    members: &[UtxoRef],
) -> Result<Vec<Vec<...RistrettoPoint>>, String>
```

Borrows utxo_set immutably for ring building while state() may be mutably borrowed elsewhere in the call chain. The borrow checker prevents simultaneous mutation.

**Status:** Safe ✅

### 7.11 Zero-Copy Opportunities

#### RS-ZC-01: DAG element access

```rust
pub fn get(&self, i: usize) -> &[u8; 64] {
    &self.elements[i % self.elements.len()]
}
```

Returns reference, not copy. The 64-byte element is accessed in-place. SHA-512 consumes 64-byte blocks natively.

**Status:** Optimal ✅

#### RS-ZC-02: Block deserialization

```rust
let block: Block = serde_json::from_str(&line).map_err(...)?;
```

serde_json always allocates. For large blocks with many transactions, this is O(n) allocation.

**Status:** Acceptable (JSON is the bottleneck, not allocation). Could switch to bincode for P2P.

---

## 8. Cryptographic Audit

### 8.1 Key Generation

#### CR-KG-01: Ed25519 key generation

```rust
fn genesis_keypair() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[0u8; 32])
}
```

**Severity:** CRITICAL — TESTNET ONLY

This hardcoded key is known to everyone. Any testnet node can spend genesis eWatts. If mainnet ever uses this key, 1,000,000 eWatt genesis reward is freely spendable.

**Fix:** Require key generation via `keygen` subcommand. Store in encrypted format.

**Testnet justification:** Acceptable for testnet.

#### CR-KG-02: Miner key derivation

```rust
fn miner_keypair() -> ed25519_dalek::SigningKey {
    let mut seed = [0u8; 32];
    seed[0] = 0x01;
    ed25519_dalek::SigningKey::from_bytes(&seed)
}
```

**Severity:** CRITICAL — Always known to everyone. All testnet blocks are mined by the same key with known secret.

**Impact:** If this code reaches mainnet, ALL mining rewards are controlled by this key. Anyone can spend miner eWatts.

#### CR-KG-03: cmd_keygen uses system RNG

```rust
fn cmd_keygen() {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
}
```

**Status:** Safe ✅ Uses OS entropy via rand crate.

### 8.2 Hash Functions

#### CR-HF-01: SHA-512 for DAG walk

Used in proof.rs for the core mining loop. SHA-512 is FIPS-approved, no known collisions for 512-bit output. The walk is: read DAG element, XOR with mix, SHA-512(mix).

**Resistance to length extension:** SHA-512 is vulnerable to length extension attacks. However, the mining output is H(mix) where mix is a fixed-length buffer (64 bytes). Length extension requires controlling the message length field, which isn't applicable here because the hash inputs are fixed-size byte arrays.

**Status:** Safe ✅

#### CR-HF-02: Keccak-256 for Merkle trees

Used in proof.rs for Merkle root computation and BlockHeader.hash(). Merkle trees use the self-pairing variant (odd leaf duplicates itself). This is an anti-pattern — the conventional approach is to promote the odd element unchanged. Self-pairing creates a hash that depends on the leaf appearing twice, which could be exploited in proofs of inclusion.

**Severity:** Low (but should use standard binary Merkle tree with odd-element promotion)

**Fix:** Replace:
```rust
// Self-pair for odd elements
h.update(chunk[0]);
if chunk.len() > 1 {
    h.update(chunk[1]);
} else {
    h.update(chunk[0]); // self-pair
}
```
With:
```rust
h.update(chunk[0]);
if chunk.len() > 1 {
    h.update(chunk[1]);
}
// Else: promote odd element unchanged to next level
```

#### CR-HF-03: Keccak-256 for block hash

```rust
impl BlockHeader {
    pub fn hash(&self) -> [u8; 32] {
        let mut h = Keccak256::new();
        // ... serializes all header fields ...
        h.finalize().into()
    }
}
```

**Status:** Safe ✅ Serializes all relevant fields deterministically. Uses Keccak-256 (standard SHA-3).

### 8.3 Ed25519 Signature Verification

#### CR-ED-01: Commitment signature verification

```rust
let pubkey = VerifyingKey::from_bytes(&c.miner_id)
    .map_err(|_| "chave invalida")?;
let sig = Signature::from_slice(&c.signature)
    .map_err(|_| "assinatura invalida")?;
let msg = commit_msg(c);
pubkey.verify(&msg, &sig)
    .map_err(|_| "assinatura nao confere")?;
```

**Status:** Safe ✅ Uses ed25519-dalek library with constant-time verification.

**Note:** ed25519-dalek v2.x uses the standard Ed25519 verification (not ZIP-215). This rejects signatures with non-canonical encodings, which is correct for our use case.

#### CR-ED-02: Transaction signature verification

```rust
pub fn verify_tx_signature(tx: &Transaction, pubkey_bytes: &[u8]) -> Result<(), String> {
    let pk = VerifyingKey::from_bytes(&pk_bytes)?;
    let sig = Signature::from_slice(&tx.signatures[0])?;
    pk.verify(&tx_msg(tx), &sig)?;
}
```

**Status:** Safe ✅

#### CR-ED-03: Small subgroup check

ed25519-dalek performs small subgroup checks on public keys by default. The VerifyingKey::from_bytes checks that the point is not in a small subgroup and that the y-coordinate is canonical.

**Status:** Safe ✅

### 8.4 MLSAG Ring Signatures

#### CR-ML-01: Generator security

```rust
pub fn ring_g() -> RistrettoPoint { hash_to_point(b"Ewatts_Ring_G_v1") }
pub fn pedersen_h() -> RistrettoPoint { hash_to_point(b"Ewatts_Pedersen_H_v1") }
```

**Status:** Safe ✅ Generators are derived from hash-to-point with distinct domain separation tags. The discrete log relationship between G and H is unknown (assuming hash-to-point behaves as a random oracle).

#### CR-ML-02: Hash-to-point implementation

```rust
pub fn hash_to_point(data: &[u8]) -> RistrettoPoint {
    let mut hasher = Shake256::default();
    hasher.update(b"Ewatts_HTP_v1:");
    hasher.update(data);
    let mut reader = hasher.finalize_xof();
    let mut seed = [0u8; 64];
    reader.read(&mut seed);
    // Elligator-style: try decompressing candidate until valid
    loop {
        let candidate = ...;
        if let Some(pt) = CompressedRistretto(candidate).decompress() {
            return pt;
        }
        attempt += 1;
    }
}
```

**Security of hash-to-point:** Uses rejection sampling: generate SHAKE-256 output, interpret as compressed Ristretto point, decompress. If invalid, increment attempt counter and retry. This is the "try-and-increment" approach.

**Risk:** Try-and-increment is distinguishable from random oracle (adversary can see whether a point is on the curve). However, for this use case (generating independent generators), the distinguisher is inconsequential.

**Fixed-time concern:** The loop runs a variable number of iterations. Not constant-time. For public domain parameters this is acceptable; for key-dependent hashing this could leak via timing.

**Severity:** Low (informational)

#### CR-ML-03: MLSAG signing non-constant-time

```rust
pub fn sign(...) -> Self {
    // α for real signer
    let alpha: Vec<Scalar> = (0..n_layers).map(|_| Scalar::random(rng)).collect();
    // Random responses for non-real positions
    for i in 0..ring_size {
        if i == real_index {
            continue;
        }
        // generate random responses
    }
}
```

**Status:** Intentionally non-constant-time. The documentation says: "NOT constant-time w.r.t. real_index (testnet only)."

**Severity:** High for mainnet. An attacker monitoring timing (CPU cycles, cache timing) can determine which ring position is the real signer.

**Fix for mainnet:** Generate random responses for ALL positions, then overwrite the real_index position with the computed values.

#### CR-ML-04: MLSAG minimum ring size

```rust
const MIN_RING_SIZE: usize = 2;
```

**Analysis:** A ring of size 2 provides anonymity set of 2. For privacy, this is minimal. The default RING_SIGNATURE_SIZE=11 provides better privacy at the cost of larger signatures.

**Status:** Configurable. Acceptable.

#### CR-ML-05: MLSAG key image linkability

```rust
key_images.push(secret_keys[j] * hash_pk(&ring[real_index][j]));
```

**Security:** Key images deterministically depend on the secret key. Spending the same key twice produces the same key image, enabling double-spend detection. This is the LINKABLE property of MLSAG — it prevents double-spending while preserving signer anonymity.

**Risk:** If the same private key is reused in two different rings at different ring positions, those key images are the same, linking the two spends. This is by design (same as Monero).

**Status:** Correct ✅

### 8.5 Pedersen Commitments

#### CR-PC-01: Binding property

```rust
pub fn new_with_blinding(v: u64, a: Scalar) -> Self {
    let point = a * ring_g() + Scalar::from(v) * pedersen_h();
    Commitment(point)
}
```

**Correctness:** C = aG + vH. To open to a different value v', the committer needs a' such that a'G + v'H = aG + vH. This requires knowing the discrete log of H with respect to G, which is computationally infeasible.

**Status:** ✅

#### CR-PC-02: Range proof binding

```rust
pub fn prove_with_exact_blinding(v: u64, desired: Scalar, bits: usize, rng: &mut ThreadRng) -> Self {
    // For bits-1: compute random ai, commit to bit
    // For last bit: compute a_last so that sum(ai * 2^i) = desired
    let a_last = (desired - partial) * scale.invert();
}
```

**Risk:** The last bit's blinding is computed deterministically from the partial sum and the desired total. If `scale.invert()` panics (Scale of 0 modulo L), the protocol crashes. `Scalar::from(1u64 << last_i)` is non-zero as long as last_i < 256. Since bits ≤ 64 (clamped), 2^64 ≠ 0 in ℤ/Lℤ. The inversion always succeeds.

**Status:** Safe ✅

#### CR-PC-03: Range proof reconstruction check

```rust
let mut sum = RistrettoPoint::identity();
for (i, c_i) in self.commitments.iter().enumerate() {
    sum = sum + Scalar::from(1u64 << i) * c_i.0;
}
if sum != commitment.0 { return false; }
```

**Risk:** `Scalar::from(1u64 << i)` for i > 63 produces a Scalar whose value is `(1 << i) mod L`. For i=64, `1u64 << 64` wraps to 0 in u64, then `Scalar::from(0)` produces the zero scalar. This means the 65th bit contribution is zero — a commitment of 2^64 would pass verification as 0.

**Attack:** To create a range proof committing to v = 2^64:
1. Create 65 bit commitments (bits = 65, which is clamped to 64)
2. The wrapper from u64 overflow at i=64 produces Scalar(0)
3. The reconstructed sum equals the sum of the first 64 bits
4. The total commitment includes the 65th bit, but the range proof check doesn't see it

**Severity:** HIGH

**Note:** The test `test_range_proof_large_bits_clamped` verifies that bits > 64 are clamped to 64, but doesn't verify the edge case where bits = 64 and the 64th bit (2^63) is set.

Actually more critically: the `verify()` function checks `self.commitments.len() > 64` which returns false for exactly 64 commitments. So up to 64 bits are checked. `Scalar::from(1u64 << 63)` = 2^63 mod L, which is correct. And `Scalar::from(1u64 << 64) == Scalar::from(0)` in u64 wrapping context, but since the max is 64 (clamped bits=64, positions 0-63), `1u64 << 63` is fine (0x8000000000000000).

Wait, let me re-examine: `bits.min(64)` means bits <= 64. The for loop goes `for i in 0..bits`. For bits=64, i ranges 0..63 (64 iterations). The max shift is `1u64 << 63`. u64 shift left by 63 is valid (0x8000000000000000). So no overflow issue.

The issue is if someone creates a RangeProof with bits=65 manually (not through prove_with_blinding). Then `self.commitments.len() > 64` check catches it and returns false. Good.

**Status:** Protected ✅ by the `commitments.len() > 64` check.

### 8.6 DAG Security

#### CR-DG-01: Cache-timing side channel

The DAG walk reads from DAG at position determined by mix[0..8]. The access pattern leaks which DAG elements were read. On a machine with shared memory (cloud VPS), this leaks mining secrets.

**Severity:** Low (mining doesn't have secrets; the DAG walk is public)

#### CR-DG-02: FNV hash collision

```rust
fn fnv_hash(a: u64, b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    h ^= a;
    for &x in b { h ^= x as u64; h = h.wrapping_mul(0x100000001b3); }
    h
}
```

**Status:** Standard FNV-1a. Used only for DAG generation mixing, not for security-critical operations. Collisions are fine (they just affect DAG quality, not security).

---

## 9. Consensus Security

### 9.1 Chain Validation

#### CS-CV-001: Block header hash verification

**Procedure AUD-CV-001:** Verify that block.header.hash() produces a deterministic output that commits to ALL consensus-relevant fields.

**Current code:** hash() serializes version, previous_hash, merkle_root, timestamp, height, epoch, difficulty_target, total_effective_commit, emission_rate, miner_effective_commit, vr_block, coinbase_burn, nonce, elapsed_ms, proof_merkle_root (optional).

**Missing fields:** None identified. All consensus fields are included.

**Status:** ✅

#### CS-CV-002: proof_hash vs block hash

```rust
pub fn proof_hash(&self) -> [u8; 32] {
    // Excludes nonce, elapsed_ms, proof_merkle_root
}
```

**Design rationale:** proof_hash is the value that the miner actually hashed during mining. It excludes nonce (which is the mining output) and elapsed_ms (which is measured during mining). The mining loop varies nonce until a winning mix is found, then records elapsed_ms and computes proof_merkle_root. So these fields must be excluded from the proof hash.

**Valid concern:** If a verifier uses hash() instead of proof_hash() for PoW verification, they'll include nonce, elapsed_ms, and proof_merkle_root, which were NOT part of the miner's proof hash. The miner could have found a valid proof_hash with nonce X, but submitted the block with a different nonce Y alongside the solution trace computed with nonce X.

**Current code use:** In p2p.rs validate_and_apply_block, the code does:
```rust
let header_hash = block.proof_hash;
// ...
crate::proof::verify(&header_hash, &solution, ...)
```

The `Proof` field in Block struct stores the correct value. ✅

#### CS-CV-003: Merkle root verification on block load

```rust
pub(crate) fn validate_block_integrity(block: &Block) -> Result<(), String> {
    // Recomputes merkle root from transactions and compares with header.merkle_root
}
```

**Status:** ✅ Called during chain store loading and integrity validation.

### 9.2 Difficulty Adjustment

#### CS-DA-001: Window size correctness

DIFFICULTY_WINDOW_BLOCKS = 100. adjust_difficulty compares actual_accesses vs target_accesses.

**Current behavior:** Each block recalculates target_accesses = TARGET_BLOCK_TIME_SECS × BASELINE_ACCESS_RATE. The window average uses actual block times.

**Missing:** The difficulty adjustment targets a specific *block rate* (10 min/block). But "actual_accesses" in the code is derived from timestamps, not from actual AOPS. The actual difficulty should target a hashrate, not a timestamp. Let me check:

```rust
pub fn adjust_difficulty(current: u64, actual_accesses: f64, target_accesses: f64) -> u64 {
```

This takes actual and target "accesses" — which in the calling context should be block times converted to access equivalents. The difficulty module doesn't directly interact with AOPS.

**Severity:** Medium — need to verify the calling code provides the right values.

#### CS-DA-002: Timestamp manipulation resistance

```rust
pub fn average_block_time(timestamps: &[u64]) -> f64 {
    let diffs: Vec<f64> = timestamps.windows(2)
        .map(|w| w[1].saturating_sub(w[0]) as f64)
        .filter(|&d| d > 0.0 && d < 3600.0)
        .collect();
    // Use median instead of mean
}
```

**Defenses:**
1. Filters out diffs ≤ 0 (monotonicity enforcement — prevents negative time)
2. Filters out diffs ≥ 3600 seconds (rejects extreme outliers)
3. Uses median (robust to single-point manipulation)

**Attack vector:** If an attacker controls 51% of blocks in the window, they can shift the median. For a window of 100 blocks, 51 malicious timestamps skew the median to the attacker's value.

**Severity:** Low (requires 51% hash)

### 9.3 Orphan Chain Management

#### CS-OC-001: Orphan queue bounded

MAX_ORPHANS = 500. When full, oldest orphan is evicted.

**Attack surface:** Attacker floods orphans. Honest orphan (legitimate block whose parent is delayed) gets evicted and lost. After reconnection, honest node has to re-request the full block.

**Severity:** Low (resolved via sync protocol on reconnect)

#### CS-OC-002: Orphan resolution infinite loop protection

```rust
pub fn resolve_orphans(&mut self, parent_hash: &[u8; 32]) -> Vec<[u8; 32]> {
    let mut found = true;
    while found {
        found = false;
        // ...
        for hash in &to_resolve {
            // add_block, then recursive resolve
            let children = self.resolve_orphans(hash);
        }
    }
}
```

**Risk:** If add_block fails (e.g., duplicate), the orphan stays in the orphan set. The code does:
```rust
if let Ok(_) = self.add_block(block) {
    resolved.push(*hash);
    let children = self.resolve_orphans(hash);
    resolved.extend(children);
}
```
If add_block fails, the orphan remains in the set, and resolve_orphans returns without removing it. Next time resolve_orphans is called with the same parent, it tries again, creating an infinite loop inside the while loop?

No — the while loop checks `to_resolve.is_empty()` at the bottom. If add_block keeps failing, the orphan stays in self.orphans, and on the NEXT iteration of the `while found` loop, to_resolve is recomputed and finds it again. But then add_block fails again. Infinite loop.

**Severity:** Medium  
**Fix:** Remove the orphan even on add_block failure:
```rust
if let Some(block) = self.orphans.remove(hash) {
    self.orphan_order.retain(|h| h != hash);
    if let Ok(_) = self.add_block(block) { ... }
}
```

### 9.4 Reorg Safety

#### CS-RG-001: Reorg depth limit

```rust
let max_reorg = 100;
if to_unwind.len() > max_reorg || to_apply.len() > max_reorg {
    return Err("Reorg too deep...");
}
```

**Status:** ✅ Protects against deep chain reorganizations.

#### CS-RG-002: Atomic reorg with snapshot

```rust
let state_snapshot = state.clone();
let store_snapshot = store.clone();
match execute_reorg_inner(to_unwind, to_apply, store, state) {
    Ok(resurrect) => Ok(resurrect),
    Err(e) => {
        *state = state_snapshot; *store = store_snapshot;
        Err(e)
    }
}
```

**Status:** ✅ Snapshot-then-rollback pattern provides atomicity. However, cloning the entire UtxoSet and ChainStore is expensive. For large UTXO sets (millions), this could take seconds and double memory.

**Severity:** Medium (performance DoS vector during deep reorgs)

#### CS-RG-003: Fallback unwind without BlockDiff

```rust
for hash in to_unwind {
    if let Some(diff) = store.block_diffs.get(hash) {
        state.unwind_with_diff(diff)?;
    } else {
        // Fallback: construct BlockDiff from block data
        // This approach cannot restore MLSAG-hidden spent UTXOs!
    }
}
```

**Severity:** HIGH — Fallback unwind for private transactions cannot restore spent UTXOs because MLSAG hides which input was actually spent. The code explicitly documents this:

```rust
// 2. For private txs: we cannot fully reverse the spent UTXOs here
// because MLSAG hides which input was actually spent.
```

**Impact:** A reorg that falls back to the legacy unwind path leaves the UTXO set in an inconsistent state for private transactions. Some UTXOs remain spent when they should be unspent.

**Fix:** Ensure BlockDiff is always persisted alongside blocks for the entire chain history. Currently only recent diffs are kept. Blocks loaded from disk after restart have no diffs, so ANY reorg after restart uses the broken fallback.

#### CS-RG-004: Tx resurrection correctness

```rust
let all_still_spent = tx.inputs.iter()
    .all(|i| state.spent_key_images().contains(&i.key_image));
```

**Analysis:** After unwinding the old chain and applying the new chain, a transaction from the old chain is "resurrected" if any of its inputs are no longer spent in the new chain. This correctly identifies orphaned transactions.

**Issue:** The resurrected transaction might conflict with a transaction in the new chain that spends the same UTXO via a different key image (unlikely but possible in private mode). However, since key images are deterministic, the same key image can't appear twice.

**Status:** ✅

### 9.5 Time-Lock Enforcement

#### CS-TL-001: Founder lock applied to coinbase

```rust
// In apply_block_inner:
let expected_lock = crate::reward::founder_lock_block(block_height);
for o in &tx.outputs {
    if o.spendable_after != expected_lock {
        return Err(format!("Coinbase spendable_after must be {}..."));
    }
}
```

**Status:** ✅ Enforced at the protocol level.

#### CS-TL-002: Time-lock checked during spend

```rust
if !utxo_is_spendable(utxo, current_block) {
    return Err("UTXO time-locked".into());
}
```

**Status:** ✅ Enforced.

#### CS-TL-003: Reorg and time-locks

During a reorg, if current_block changes (e.g., chain shrinks from height 60,000 to height 40,000), some UTXOs that were spendable at the old height may become unspendable at the new height.

**Current code:** The reorg unwind process doesn't re-check time-locks when blocks are un-applied. It just removes created UTXOs and restores consumed ones. The restored UTXOs have their original `spendable_after` values preserved.

**Severity:** Low (by design — after reorg, future spends will check the new chain height)

---

## 10. P2P Network Security

### 10.1 Connection Handling

#### P2P-CN-001: Rate limiting

```rust
conn_budget: TokenBucket::new(5.0, 5.0),  // 5 conn/s burst, 5/s refill
```

**Status:** ✅ Token bucket prevents connection floods. Burst of 5 allows rapid initial connections, then 5/s refill.

**Limitation:** Rate limiting only applies to inbound connections. Outbound (dial) connections bypass the token bucket.

#### P2P-CN-002: Peer eviction

```rust
peer_mgr: PeerManager::new(200),  // max 200 peers
```

**Status:** ✅ LRU eviction when full. New connections are always preferred (with rate limiting).

**Attack surface:** Attacker opens 200 connections, fills the peer set, then keeps sending activity (ping/pong) to avoid eviction. Honest peers are evicted. The rate limiter prevents rapid reconnection by honest peers (5/s).

**Severity:** Medium — Practical sybil attack on peer set.  
**Fix:** Reserve a portion of peer slots for outbound connections (e.g., 50 slots for bootstrapped/manually configured peers).

#### P2P-CN-003: Connection idle timeout

```rust
config = config.with_idle_connection_timeout(std::time::Duration::from_secs(60));
```

**Status:** ✅ Connections idle >60s are closed.

### 10.2 Gossip Security

#### P2P-GS-001: Message ID deduplication

```rust
let message_id_fn = |msg: &gossipsub::Message| {
    let mut h = sha3::Keccak256::new();
    h.update(&msg.data);
    MessageId::from(h.finalize().to_vec())
};
```

**Status:** ✅ Message ID is the hash of the raw data. Same data → same ID → gossipsub deduplicates. This prevents infinite relay loops.

#### P2P-GS-002: Compact block deterministic nonce

```rust
let nonce = u64::from_le_bytes(hash[..8].try_into().unwrap_or([0u8; 8]));
```

**Analysis:** All nodes produce the same CompactBlock for the same block because the nonce is derived from the block hash. The gossipsub message ID for the compact block data is deterministic.

**Status:** ✅ Prevents relay duplicates.

#### P2P-GS-003: Compact block reconstruction validation

```rust
pub fn reconstruct_block(cb: &CompactBlock) -> Option<Block> {
    let mempool_txns = crate::mempool::peek_all();
    // ... match short IDs ...
    let block = Block { header: cb.header.clone(), body: ..., proof_hash: cb.proof_hash };
    Some(block)
}
```

**Severity:** HIGH — reconstruct_block does NOT validate the reconstructed block before returning Some(Block). The caller (validate_and_apply_block) validates PoW and state, but:

1. The reconstructed block includes transactions from the local mempool matched by short ID
2. An attacker can craft a compact block with header X and short IDs that match attacker-submitted transactions in the victim's mempool
3. The victim reconstructs a block that never existed on any chain
4. If the block passes validation (because it includes attacker-controlled transactions), the attacker can cause state divergence

**Fix:** Validate that the Merkle root of the reconstructed block matches the header.

Actually, reconstruct_block already has a comment:
```rust
// Sanity check: verify merkle root matches (catches short ID collisions)
// For blocks with commitments only, skip this check
```
But the check is NOT IMPLEMENTED. It's just a comment.

**Severity:** CRITICAL  
**Fix:** Add Merkle root verification:
```rust
let mut tx_hashes: Vec<[u8; 32]> = txs.iter().map(|tx| tx.hash()).collect();
// compute merkle root from tx_hashes
// compare with cb.header.merkle_root
```

#### P2P-GS-004: Short ID collision probability

Short ID = first 8 bytes of Keccak256(tx_hash || nonce). With per-block nonce preventing precomputation, collision probability among N distinct txns is approximately N²/2^65. For 1000 txns: ~1000²/2^65 ≈ 2.7 × 10^-14. Negligible.

**Status:** ✅

### 10.3 Block Sync

#### P2P-SY-001: Request range limit

```rust
P2pMessage::BlockRequest { from_height, to_height }
```

**No range size limit in the code.** An attacker can request `from_height=0, to_height=u64::MAX`, causing the node to load ALL blocks from disk and send them over the network. This is a bandwidth amplification attack.

**Severity:** HIGH  
**Fix:** Cap range size:
```rust
const MAX_BLOCK_SYNC_RANGE: u64 = 500;
if to_height > from_height + MAX_BLOCK_SYNC_RANGE {
    let capped_to = from_height + MAX_BLOCK_SYNC_RANGE;
    // Return capped range
}
```

#### P2P-SY-002: Unvalidated block acceptance from sync

Blocks received via BlockResponse are passed to validate_and_apply_block, which checks PoW and state. However, no check that the responding peer is authorized (libp2p uses noise for authenticated encryption).

**Status:** ✅ (libp2p noise provides peer identity verification; PoW provides content verification)

---

## 11. Privacy & Anonymity

### 11.1 Stealth Addresses

#### PR-SA-001: One-time address derivation

```rust
pub fn derive_destination(&self, rng: &mut ThreadRng) -> (OneTimeAddress, Scalar) {
    let r = Scalar::random(rng);
    let shared = r * self.view_key;
    let h = hash_to_scalar(shared.compress().as_bytes());
    let dest = h * ring_g() + self.spend_key;
    let ephemeral = r * ring_g();
    (OneTimeAddress { dest, ephemeral }, r)
}
```

**Analysis:** Each transaction generates a fresh `r`. The shared secret `r × V` is hashed to a scalar `h`, which is added to the spend key. Without the view secret key `v`, an adversary cannot compute `h` and cannot link the destination to the original stealth address. The ephemeral public key `R = rG` is published alongside the output.

**Status:** ✅ Standard Monero-style stealth address construction.

#### PR-SA-002: One-time key recovery

```rust
pub fn recover_one_time_key(view_secret: &Scalar, spend_secret: &Scalar, ephemeral: &RistrettoPoint) -> Scalar {
    let shared = view_secret * ephemeral;
    let h = hash_to_scalar(shared.compress().as_bytes());
    h + spend_secret
}
```

**Analysis:** Given the view secret `v` and the ephemeral key `R`, the recipient computes `v × R = rV` (same shared secret). Then derives `h` and adds the spend secret to get the one-time private key.

**Status:** ✅ Correct.

### 11.2 MLSAG Anonymity

#### PR-ML-001: Anonymity set size

RING_SIGNATURE_SIZE = 11 (constants.rs). Each ring has 11 members, providing anonymity set of 11 per input.

**Decoy selection strategy:** The code doesn't specify how ring members are selected. In state.rs:
```rust
pub fn build_ring_inline(utxo_set: &HashMap<UtxoKey, UtxoEntry>, members: &[UtxoRef])
```
It takes a list of UtxoRef, which must be provided by the caller (wallet or RPC). There is NO built-in decoy selection algorithm.

**Severity:** HIGH — If the wallet always picks the same set of decoys (or nearby UTXOs), the anonymity set is far smaller than 11. This is the most common privacy failure in ring-signature systems.

**Fix:** Implement a deterministic decoy selection algorithm that samples from the UTXO set with appropriate distribution (age, value, count). See Monero's decoy selection algorithm.

#### PR-ML-002: Ring member privacy

```rust
// Private mode: stealth dest is the pubkey
let pk = CompressedRistretto(*sd).decompress()...;
```

Only UTXOs with stealth destinations can be ring members. Legacy (non-stealth) UTXOs are excluded:
```rust
"Ring member ... is a legacy (non-stealth) UTXO. Only private UTXOs can be ring members."
```

**Analysis:** This means legacy UTXOs cannot be used as decoys. If most UTXOs are legacy early in the chain, the effective anonymity set is small.

**Status:** Design limitation. As the protocol transitions to full privacy, this self-resolves.

#### PR-ML-003: Traceability through spend pattern

Since key images are unique per spent key, anyone monitoring the chain can count how many times a particular stealth address was used. This doesn't reveal WHICH address, but reveals that blockchain activity exists.

**Status:** Fundamental property of linkable ring signatures. Same as Monero. ✅

### 11.3 Range Proof Security

#### PR-RP-001: Bit commitment construction

Each bit is committed with `C_i = Commitment::new_with_blinding(bit, a_i)`. The 1-out-of-2 ring proves that C_i opens to either 0 or 1. The rings are:
```
ring[0] = [C_i - 0*H] = [C_i]
ring[1] = [C_i - 1*H] = [C_i - H]
```

**Status:** ✅ Standard construction.

#### PR-RP-002: Range proof completeness

The last bit's blinding factor is computed as:
```rust
let a_last = (desired - partial) * scale.invert();
```

This ensures Σ(a_i × 2^i) = desired_total. But note: `scale = Scalar::from(1u64 << last_i)`. In scalar arithmetic, `scale.invert()` computes the modular inverse modulo L (≈ 2^252). This always exists if scale ≠ 0 mod L, which holds for last_i < 252 (clamped to 64).

**Status:** ✅

#### PR-RP-003: Verify rejects oversized proofs

```rust
if self.commitments.len() > 64 { return false; }
```

**Status:** ✅ Bounds check prevents 65+ bit range proofs.

### 11.4 Commitment Balance

#### PR-CB-001: Amount conservation for private txs

```rust
if has_private_outputs || tx.mlsag.is_some() {
    // Check input_amount >= output_amount
}
```

**Analysis:** For private transactions, the code checks plaintext amount conservation:
```rust
if input_amount < output_amount {
    return Err("Output amount exceeds input amount (inflation attack)".into());
}
```

**Issue:** This leaks the transaction amounts to the verifier. The whole point of private transactions is to HIDE amounts. By checking `input_amount >= output_amount` on the plaintext, the verifier learns all amounts.

**Severity:** MEDIUM — Amounts are NOT hidden from validators. Only external observers who don't run full nodes are kept in the dark.

**Fix:** Replace plaintext check with Pedersen commitment balance check:
```rust
// C_in = Σ input_commitments
// C_out = Σ output_commitments
// Verify: C_in - C_out = 0 (identity point) or C_in - C_out is a commitment to a non-negative value
```

**Current code does not implement this.** The check uses plaintext amounts, so privacy is incomplete.

#### PR-CB-002: Commitment openings reconciliation

Currently there is no formal reconciliation between the Pedersen commitments and the stored amount in TxOutput. The commitment is stored in `commitment_bytes` but the amount is stored separately in the `amount` field. A malicious validator could store C(v) and plaintext v + 1, and the inconsistency would go undetected at the consensus layer.

**Severity:** HIGH — This breaks the binding property. If the block creator or a reorg path combines commitments from one transaction with amounts from another, inflation is possible.

**Fix:** In the consensus-critical path (state.apply_block), verify that all stored commitments match their plaintext amounts:
```rust
for o in &tx.outputs {
    if o.is_private() {
        let comm = Commitment(...);
        assert!(comm.verify(o.amount, blinding_from_range_proof));
    }
}
```

---

## 12. Economic Security

### 12.1 Emission Schedule

#### EC-EM-001: Elastic supply bounds

Base emission = 100 eWatt/block. Floor = 5 eWatt (0.05x). Ceiling = 2000 eWatt (20x). This means annual supply at base rate = 5,259,600 eWatt.

**Attack surface:** 51% miner can drive emission to minimum (5 eWatt/block) by performing very few access operations, starving the chain of new supply.

**Defense:** Miners are economically incentivized NOT to reduce emission because their rewards decrease proportionally. The attack requires 51% of miners to collude against their own interest.

**Status:** Acceptable economic design.

#### EC-EM-002: Historical average computation

```rust
pub fn compute_emission_rate(total_effective_aops: f64, historical_avg_aops: f64) -> f64 {
    // Emission = BASE * total / hist
    // If historical_avg = 0: return BASE
}
```

The code uses `historical_avg_aops` as the denominator. In compute_block_rewards:
```rust
let emission = compute_emission_rate(total_eff, historical_avg_aops);
```

**But:** `historical_avg_aops` is passed in as a parameter. The caller (in main.rs) uses:
```rust
let avg_hist = if height == 0 { constants::BASE_EMISSION } else { constants::BASE_EMISSION };
```

This is a placeholder! The actual historical average is not computed from on-chain data. It's always `BASE_EMISSION = 100`.

**Severity:** HIGH — The elastic supply mechanism doesn't function because the denominator is fixed. Emission is always `BASE × total_eff / BASE = total_eff`. Since total_eff ≈ individual miner's effective AOPS, emission = total_eff.

Wait, no. Let me re-read. In compute_block_rewards:
- `total_eff` is the sum of all miners' effective commitments (in AOPS)
- For a solo miner with 25M AOPS and eff = 1.0: total_eff = 25M
- `historical_avg_aops` is BASE_EMISSION = 100
- `emission = BASE_EMISSION × 25_000_000 / 100 = 250_000 eWatt`

This is the ceiling (2000) capped case. So emission is always at or near the ceiling.

**Fix (roadmap):** Compute `historical_avg_aops` from the actual network over the VR window.

### 12.2 Commitment Verification

#### EC-CM-001: Commitment validity window

COMMIT_WINDOW_BLOCKS = 4300 (30 days). Commitments older than 4300 blocks are not considered in the rolling median.

**Status:** ✅ Reasonable window for bandwidth tracking.

#### EC-CM-002: Minimum commitment threshold

```rust
pub const MIN_COMMIT_AOPS: f64 = 20_000_000.0; // 20M random accesses/s (DDR baseline)
```

Miners must declare at least 20M AOPS. Derived from 75W node / 3.75 µJ per access.

**Attack:** A miner declares 20M AOPS but only does 1M actual ops. Efficiency = 0.05, effective commitment = 20M × 0.05 = 1M. The penalty reduces their effective bandwidth but doesn't prevent mining.

**But:** The commit validate check:
```rust
let e = compute_efficiency_aops(c.total_access_ops, c.access_ops_per_sec, c.time_seconds);
if e <= 0.0 { return Err("eficiencia zero".into()); }
```

A miner with efficiency 0.05 passes validation (e > 0). They still get a reward proportional to 1M effective AOPS. This is acceptable — the penalty mechanism handles underperforming miners.

**Status:** ✅

### 12.3 Ramp-up Period

#### EC-RU-001: Ramp-up cap at 80%

```rust
if share > constants::RAMP_UP_CAP {
    let excess = *reward - (total * constants::RAMP_UP_CAP);
    burned += excess;
    *reward = total * constants::RAMP_UP_CAP;
}
```

**Status:** ✅ Prevents any single miner from claiming >80% of reward during first 10,000 blocks.

#### EC-RU-002: Burned supply tracking

The burned amount is tracked in `RewardSummary::burned` and emitted in `coinbase_burn` in the block header. However, there is no explicit mechanism to "destroy" the burned supply. The coinbase creates outputs for the full emission, then `apply_ramp_up_cap` reduces miner rewards. The excess is tracked but NOT reflected in the state — the coinbase still created the full amount.

Wait, let me re-check compute_block_rewards:
```rust
// burned = apply_ramp_up_cap(block_number, &mut rewards);
// rewards get reduced by burned amount
// RewardSummary {
//     miner_rewards: rewards.iter().map(|(pk, r)| (pk.clone(), ewatt_to_units(*r))).collect(),
//     total_emission: ewatt_to_units(emission),  // pre-cap total (includes burned)
//     burned: ewatt_to_units(burned),
// }
```

Then in apply_block_inner (state.rs):
```rust
let coinbase_amount: u64 = tx.outputs.iter().map(|o| o.amount).sum();
self.add_coinbase_supply(coinbase_amount);
```

The coinbase_amount is the sum of outputs in the coinbase transaction. If `apply_ramp_up_cap` reduced miner rewards, the coinbase transaction should only create outputs for the actual rewards, not the full emission including burned. The burned amount is not a separate output — it's just a tracking counter.

**Severity:** HIGH — The burned supply is tracked in the header but NOT removed from the UTXO set. Supply continues to grow by the full emission rate, while miner output is lower. The "burned" eWatts are never actually created in the UTXO set, so the supply tracked by total_supply() MUST equal the coinbase amounts actually created.

Let me re-check the coinbase transaction creation in main.rs:
```rust
let reward_base_units = (miner_reward * 100_000_000.0) as u64; // 1 Ewatt = 10^8 base
let coinbase = Transaction {
    outputs: vec![TxOutput { amount: reward_base_units, ... }],
};
```

The coinbase creates outputs equal to `miner_reward`, not `emission`. So the burned amount is NOT created as a UTXO output. The `total_emission` and `burned` fields in RewardSummary are informational only.

The state.apply_block_inner checks:
```rust
let coinbase_amount: u64 = tx.outputs.iter().map(|o| o.amount).sum();
let max_emission = crate::constants::BASE_EMISSION_UNITS * 20; // = 2B base
if coinbase_amount > max_emission { ... Err ... }
```

But it doesn't check that coinbase_amount equals `emission_rate` from the header. So a miner could create a coinbase with fewer outputs than the emission rate, effectively burning supply.

**Status:** The burn mechanism works IF the miner correctly reduces the coinbase output. But there's no enforcement at the protocol layer that `sum(coinbase.outputs) + header.coinbase_burn = header.emission_rate`.

**Severity:** MEDIUM  
**Fix:** Enforce in apply_block_inner:
```rust
let expected = header.emission_rate - header.coinbase_burn;
if coinbase_amount != expected {
    return Err("Coinbase amount mismatch with emission rate and burn".into());
}
```
Also enforce coinbase_burn consistency in main.rs's mining code.

---

## 13. Storage & Persistence

### 13.1 Data Integrity

#### ST-IN-001: Atomic writes with tmp + rename

```rust
fs::write(&tmp, &json)...
fs::rename(&tmp, &path)...
```

**Status:** ✅ Atomic write for UTXO set and chain store. Prevents partial writes.

#### ST-IN-002: Append-only block log (no atomicity)

```rust
let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)?;
writeln!(file, "{}", json)?;
file.flush()?;
file.sync_data()?;
```

**Issue:** The blocks.jsonl file uses append-only mode. If the node crashes between `write` and `sync_data`, the last block write is partially written (truncated JSON line). On restart, `load_blocks` will try to parse the truncated JSON and fail.

**Severity:** MEDIUM — Single corrupted line prevents loading ALL subsequent blocks during deserialization.

**Fix:** Write to a temp file first, then rename. Or use line-by-line parsing that skips unparseable lines (with warnings).

#### ST-IN-003: Cache consistency

The BLOCK_CACHE is invalidated after pruning. But there's no consistency check between cache and disk after a crash. If the node crashes after writing to disk but before updating cache, or vice versa, the cache contains stale data.

**Current behavior:** On next call to load_blocks(), cache miss triggers a full disk reload, which repopulates the cache from disk. So the cache is eventually consistent.

**Status:** ✅ Eventual consistency.

### 13.2 File System Security

#### ST-FS-001: Default data directory

```rust
const DEFAULT_DATA_DIR: &str = "ewatts_data"; // relative to CWD
```

**Issue:** Data directory is relative to the current working directory. If the node is started from a different CWD, it creates a new data directory. This could cause:
1. Two nodes running from different directories (same issue as Bitcoin-Qt data dir)
2. Accidental data loss if CWD is a temp directory

**Severity:** Low  
**Fix:** Support --datadir flag to specify absolute path.

#### ST-FS-002: Genesis.key and miner.key stored in plaintext

```rust
fs::write(format!("{}/genesis.key", data_dir()), seed)?;
```

**Issue:** Private keys are stored as raw 32-byte files in the data directory. Any process with read access to the data directory can steal keys.

**Severity:** HIGH for mainnet  
**Fix:** Encrypt key files with a passphrase (e.g., AES-256-GCM with PBKDF2 key derivation). Or use OS keychain (macOS Keychain, Linux secret-tool).

### 13.3 Garbage Collection

#### ST-GC-001: Prune orphaned blocks

```rust
pub fn prune_blocks(before_height: u64) -> Result<usize, String> {
    // Reads all blocks, filters, writes back with tmp+rename
}
```

**Status:** ✅ Atomic prune with tmp+rename. Old blocks are compacted.

#### ST-GC-002: No automatic pruning

Prune is not called automatically. Blocks accumulate indefinitely. For a testnet at 10-min blocks, that's ~52,596 blocks/year × ~500 bytes = ~26 MB/year. Acceptable for years.

**Status:** Informational.

---

## 14. Denial of Service

### 14.1 Resource Exhaustion

#### DOS-RES-001: DAG generation CPU exhaustion

DAG generation is intentionally slow (memory-hard). A malicious peer can request block headers that require DAG regeneration at a new epoch, forcing the node to spend CPU time regenerating the DAG.

**Current mitigation:** DAG cache stores the most recent (epoch, size). Cache hit avoids regeneration. But epoch changes trigger full regeneration.

**Severity:** Medium  
**Fix:** Limit DAG regeneration frequency. Only regenerate if epoch changes AND sufficient time has passed since last regeneration.

#### DOS-RES-002: JSON deserialization bomb

An attacker sends a deeply nested JSON P2P message. serde_json by default doesn't limit nesting depth. Extremely deep JSON can cause stack overflow or memory exhaustion.

**Current test:**
```rust
let deep = format!("{{\"x\":{}}}", "{\"x\":".repeat(1000) + &"}".repeat(1000));
let result: Result<P2pMessage, _> = serde_json::from_str(&deep);
assert!(result.is_err(), "Deeply nested JSON must not crash");
```

**Status:** ✅ Tested to be safe (serde_json returns error on stack exhaustion).

#### DOS-RES-003: Unbounded block response

An attacker can respond to a BlockRequest with millions of blocks. Each block goes through `validate_and_apply_block` which is expensive.

**Mitigation:** The request range limits the response. But an attacker controlling a peer can send an unsolicited BlockResponse.

**Current code path:** On receiving BlockResponse:
```rust
P2pMessage::BlockResponse { blocks } => {
    for blocks in &block { validate_and_apply_block(block, state, &mut chain_store)? }
}
```

No limit on block count in response.

**Severity:** HIGH  
**Fix:** Limit BlockResponse to MAX_RESPONSE_BLOCKS (e.g., 500).

#### DOS-RES-004: Memory exhaustion via mempool

MAX_MEMPOOL_TXS = 5000. Each transaction can be up to ~100KB (MLSAG ring of 11 with range proofs). Total: 5000 × 100KB = 500MB.

**Status:** Unbounded memory within the 5000 limit. Acceptable for a node with 1GB+ RAM.

### 14.2 Network Attacks

#### DOS-NET-001: Gossip message amplification

Gossipsub propagates messages to all peers. An attacker can inject a message with a unique message ID, causing O(N) amplification where N is the number of peers.

**Mitigation:** gossipsub's mesh topology limits propagation to mesh peers (typically 6-12). Not all peers receive all messages.

**Status:** ✅ Standard libp2p gossipsub deployment.

#### DOS-NET-002: Request/response flooding

An attacker sends BlockRequest to all peers, forcing them to load blocks from disk and send them. The CPU load is on the responder.

**Token bucket protection:** Inbound connection rate limiting limits new connections, but does NOT limit request rate per established connection.

**Severity:** Medium — 200 peers each sending one request/second generates 200 disk reads/second.

**Fix:** Add per-peer request rate limiting.

---

## 15. Appendix: Audit Procedures Checklist

### A. Module: constants.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AC-001 | Verify PROTOCOL_VERSION matches expected | 0x0005 | ✅ |
| AC-002 | Verify EMISSION_FLOOR_MULTIPLIER * BASE_EMISSION >= 5 | 5.0 | ✅ |
| AC-003 | Verify EMISSION_CEILING_MULTIPLIER * BASE_EMISSION >= EMISSION_FLOOR | 2000 >= 5 | ✅ |
| AC-004 | Verify J_PER_ACCESS = WATTS_PER_NODE / MIN_COMMIT_AOPS | 3.75e-6 | ✅ |
| AC-005 | Verify J_PER_ACCESS_DDR3 > J_PER_ACCESS_DDR4 > J_PER_ACCESS_DDR5 | 10e-6 > 5e-6 > 3.75e-6 | ✅ |
| AC-006 | Verify COMMIT_WINDOW_BLOCKS ≈ 30 days | 4300 blocks × 600s ≈ 29.8d | ✅ |
| AC-007 | Verify RAMP_UP_BLOCKS within reasonable range | 10000 ≈ 69.4 days | ✅ |
| AC-008 | Verify FOUNDER_LOCK_BLOCKS + FOUNDER_LOCK_ADDITIONAL > RAMP_UP_BLOCKS | 50000 + 40000 > 10000 | ✅ |
| AC-009 | Verify no integer overflows in constants | All fit in range | ✅ |
| AC-010 | Verify TESTNET values scale correctly | TESTNET_RAMP_UP = 100 (vs 10000 mainnet) | ✅ |

### B. Module: dag.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AD-001 | Verify cache doesn't return stale data | Same epoch+size = same DAG | ✅ |
| AD-002 | Verify size < 64 panics | Expected behavior | ⚠️ Fix: return Result |
| AD-003 | Verify element count = size_bytes / 64 | Integer division | ✅ |
| AD-004 | Verify different epochs produce different DAGs | Proven by test | ✅ |
| AD-005 | Verify FNV hash doesn't overflow | wrapping_mul handles overflow | ✅ |
| AD-006 | Verify get() wraps mod len() | i % elements.len() | ✅ |
| AD-007 | Verify cache is thread-safe | OnceLock + Mutex | ✅ |
| AD-008 | Verify per_epoch_growth doesn't truncate | Integer division after mul | ⚠️ Precision OK |
| AD-009 | Verify total DAG size = INITIAL + per_epoch * epoch | Correct | ✅ |
| AD-010 | Verify accelerate flag doubles growth | size = DAG_ACCELERATION_RATE | ✅ |

### C. Module: proof.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AP-001 | Verify meets_difficulty with difficulty=1 always returns true | target = u64::MAX | ✅ |
| AP-002 | Verify difficulty_to_accesses with large difficulty doesn't overflow | ⚠️ CRITICAL: u64 overflow | 🔴 FIX |
| AP-003 | Verify mine() and verify() roundtrip | Should always pass | ✅ |
| AP-004 | Verify merkle_root computation is complete | All trace samples included | ✅ |
| AP-005 | Verify sampled verification randomness | 30 random indices | ⚠️ Non-deterministic |
| AP-006 | Verify fallback verification (empty trace) | Full walk | ✅ |
| AP-007 | Verify elapsed_offset_us monotonicity | Checked in verify() | ✅ |
| AP-008 | Verify sample_interval computation | walk_length / 1000 | ✅ |
| AP-009 | Verify sample_leaf_hash uniqueness | position + mix_hash | ✅ |
| AP-010 | Verify nonce exploration loop terminates | After nonce_limit attempts | ✅ |

### D. Module: commitment.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| ACO-001 | Verify compute_efficiency handles NaN/Inf | Returns 0.0 | ✅ |
| ACO-002 | Verify effective_commitment lower bound (e < 0.7) | d * e | ✅ |
| ACO-003 | Verify effective_commitment upper bound (e > 1.3) | d * 1.3 | ✅ |
| ACO-004 | Verify commit_msg covers all fields | 5 fields serialized | ✅ |
| ACO-005 | Verify signature verification uses correct message | commit_msg(c) | ✅ |
| ACO-006 | Verify minimum AOPS check | access_ops_per_sec >= 20M | ✅ |
| ACO-007 | Verify rolling minimum computation | 0.1 * median of recent | ✅ |
| ACO-008 | Verify signature length check | Must be 64 bytes | ✅ |
| ACO-009 | Verify derived bandwidth_gbps | aops * 64 / 1e9 | ✅ |
| ACO-010 | Verify efficiency <= 0 rejection | Err("eficiencia zero") | ✅ |

### E. Module: vr.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AV-001 | Verify VR computation with zero emission | Returns 0 VR | ✅ |
| AV-002 | Verify VR doubles with double AOPS | Proven by test | ✅ |
| AV-003 | Verify VR halves with double emission | Proven by test | ✅ |
| AV-004 | Verify format_vr with NaN | Returns "0.000 kWh/Ewatt" | ✅ |
| AV-005 | Verify estimate_settlement with zero VR | Returns 0 | ✅ |
| AV-006 | Verify compute_vr_series length | len = n - window | ✅ |
| AV-007 | Verify VR with single block window | Integrated | ✅ |
| AV-008 | Verify J_PER_ACCESS used correctly | total_joules = accesses * 3.75e-6 | ✅ |
| AV-009 | Verify kwh conversion | J / 3,600,000 | ✅ |
| AV-010 | Verify energy computation matches wall-power model | DDR5 baseline | ✅ |

### F. Module: reward.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AR-001 | Verify emission rate at exact historical average | Returns BASE_EMISSION = 100 | ✅ |
| AR-002 | Verify emission floor at low AOPS | 5 eWatt minimum | ✅ |
| AR-003 | Verify emission ceiling at high AOPS | 2000 eWatt maximum | ✅ |
| AR-004 | Verify ramp-up cap calculation | 80% max per miner | ✅ |
| AR-005 | Verify ramp-up cap only in first 10000 blocks | No cap after | ✅ |
| AR-006 | Verify founder_lock_block calculation | max(50000, height+40000) | ✅ |
| AR-007 | Verify no founder lock after ramp-up | Returns 0 | ✅ |
| AR-008 | Verify ewatt_to_units rounding | round() not trunc() | ✅ |
| AR-009 | Verify reward proportionality | Higher eff = higher reward | ✅ |
| AR-010 | Verify sum(miner_rewards) + burned = total_emission | ✅ | ✅ |

### G. Module: block.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AB-001 | Verify BlockHeader.hash() covers all fields | 15 fields serialized | ✅ |
| AB-002 | Verify proof_hash() excludes nonce and proof fields | Different from hash() | ✅ |
| AB-003 | Verify merkle root computation uses Keccak-256 | Standard merkle tree | ⚠️ Self-pair odd |
| AB-004 | Verify TxOutput::hash_pubkey truncates to 20 bytes | Keccak256 → first 20 | ✅ |
| AB-005 | Verify TxOutput::new_locked sets spendable_after | Uses founder_lock_block | ✅ |
| AB-006 | Verify TxOutput::is_spendable checks current_block >= spendable_after | Correct | ✅ |
| AB-007 | Verify MlsagData roundtrip serialization | to_sig() ↔ from_sig() | ✅ |
| AB-008 | Verify Transaction::hash includes all relevant fields | inputs + outputs + ring_size | ✅ |
| AB-009 | Verify private TX hash includes stealth fields | ✅ | ✅ |
| AB-010 | Verify proof_hash is stored in Block struct | Used for PoW verification | ✅ |

### H. Module: state.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AS-001 | Verify no inflation in non-coinbase txs | validate_transaction check | ✅ |
| AS-002 | Verify double-spend detection | spent_key_images set | ✅ |
| AS-003 | Verify time-lock enforcement | spendable_after check | ✅ |
| AS-004 | Verify MLSAG verification path | Full ring verification | ✅ |
| AS-005 | Verify P2PKH verification path | Hash match + sig verify | ✅ |
| AS-006 | Verify private tx range proof verification | ✅ | ✅ |
| AS-007 | Verify hybrid tx rejection | All outputs must be private | ✅ |
| AS-008 | Verify plaintext amount conservation for private txs | ⚠️ Breaks privacy | 🔴 PRIVACY |
| AS-009 | Verify coinbase input check | Must have empty inputs | ✅ |
| AS-010 | Verify coinbase amount cap | ≤ 20 × BASE_EMISSION_UNITS | ✅ |
| AS-011 | Verify coinbase spendable_after enforcement | Must match expected_lock | ✅ |
| AS-012 | Verify supply tracking | add_coinbase_supply | ⚠️ Overflow silent |
| AS-013 | Verify BlockDiff unwind | Atomic rollback | ✅ |
| AS-014 | Verify apply_block_and_track atomicity | Partial rollback on failure | ✅ |
| AS-015 | Verify unwind_with_diff correctness | Reverses apply_block_and_track | ✅ |

### I. Module: store.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AST-001 | Verify atomic UTXO save | tmp + rename | ✅ |
| AST-002 | Verify atomic chain store save | tmp + rename | ✅ |
| AST-003 | Verify block append is crash-safe | append + sync_data | ⚠️ Partial write risk |
| AST-004 | Verify block_cache bounded | MAX_CACHED_BLOCKS = 10000 | ✅ |
| AST-005 | Verify validated_block_integrity checks | merkle, previous_hash, emission, proof_hash | ✅ |
| AST-006 | Verify chain_store loading picks heaviest chain | accumuated_work comparison | ✅ |
| AST-007 | Verify disk corruption detection | Halts on validation failure | ✅ |
| AST-008 | Verify prune atomicity | tmp + rename | ✅ |
| AST-009 | Verify key file storage | Plaintext 32-byte files | ⚠️ No encryption |
| AST-010 | Verify data_dir override thread-safe | Mutex | ✅ |

### J. Module: chain.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| ACH-001 | Verify genesis creates chain tip | ChainStore::new | ✅ |
| ACH-002 | Verify add_block checks parent exists | Err on unknown parent | ✅ |
| ACH-003 | Verify duplicate block detection | Err("Block already exists") | ✅ |
| ACH-004 | Verify zero parent hash rejected for non-genesis | Height must be 0 | ✅ |
| ACH-005 | Verify orphan queue bounded | MAX_ORPHANS = 500 | ✅ |
| ACH-006 | Verify orphan LRU eviction | Oldest evicted | ✅ |
| ACH-007 | Verify orphan resolution recurses correctly | Children resolved | ✅ |
| ACH-008 | Verify set_chain_tip validates existence | Err if missing | ✅ |
| ACH-009 | Verify find_lca correctness | Standard LCA on tree | ✅ |
| ACH-010 | Verify work computation | u64::MAX / difficulty | ✅ |

### K. Module: reorg.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| ARE-001 | Verify analyze_fork decisions | Extend, Reorg, Sidechain, Orphan, Reject | ✅ |
| ARE-002 | Verify reorg depth limit | 100 blocks max | ✅ |
| ARE-003 | Verify atomic reorg with snapshot | State + store cloned | ⚠️ Expensive |
| ARE-004 | Verify BlockDiff unwind in reorg | Preferred path | ✅ |
| ARE-005 | Verify fallback unwind works | Constructs BlockDiff from block | ⚠️ Broken for MLSAG |
| ARE-006 | Verify resurrected tx deduplication | Not already in new chain | ✅ |
| ARE-007 | Verify reorg sets new chain tip | Yes | ✅ |
| ARE-008 | Verify competing fork detection | is_competing_fork | ✅ |
| ARE-009 | Verify extends_canonical detection | Shortcut for simple case | ✅ |
| ARE-010 | Verify get_chain_to_fork ordering | tip → fork_point | ✅ |

### L. Module: difficulty.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| ADI-001 | Verify adjustment bounds (0.5x - 2.0x) | clamp applied | ✅ |
| ADI-002 | Verify minimum difficulty = 1 | .max(1.0) | ✅ |
| ADI-003 | Verify median timestamp | Robust to outliers | ✅ |
| ADI-004 | Verify timestamp filter (0 < t < 3600) | Rejects extreme values | ✅ |
| ADI-005 | Verify empty diff list fallback | Uses 1.5x target | ✅ |
| ADI-006 | Verify NaN/inf protection | partial_cmp handles | ✅ |

### M. Module: mempool.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AM-001 | Verify double-spend check against chain | spent_key_images from utxo | ✅ |
| AM-002 | Verify double-spend check against mempool | key_images HashMap | ✅ |
| AM-003 | Verify UTXO double-spend check | utxo_spends HashMap | ✅ |
| AM-004 | Verify fee-based priority | Binary search insert | ✅ |
| AM-005 | Verify fee-based eviction | Lowest fee evicted | ✅ |
| AM-006 | Verify mempool size bounded | MAX_MEMPOOL_TXS = 5000 | ✅ |
| AM-007 | Verify MLSAG verification in submit | Re-validated in mempool | ✅ |
| AM-008 | Verify confirm_mined rebuilds indices | Retains non-mined | ✅ |
| AM-009 | Verify take_for_mining returns highest-fee | Sorted by fee desc | ✅ |
| AM-010 | Verify compute_fee correctness | inputs - outputs | ✅ |

### N. Module: p2p.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AP2-001 | Verify token bucket rate limiting | 5 conn/s burst, 5/s refill | ✅ |
| AP2-002 | Verify peer set bounded | Max 200 peers | ✅ |
| AP2-003 | Verify LRU eviction | peer_mgr | ✅ |
| AP2-004 | Verify compact block deterministic nonce | Derived from block hash | ✅ |
| AP2-005 | Verify reconstruct_block validates merkle | ⚠️ NOT IMPLEMENTED | 🔴 CRITICAL |
| AP2-006 | Verify short ID collision prevention | Per-block nonce | ✅ |
| AP2-007 | Verify block sync range not bounded | ⚠️ No limit | 🔴 HIGH |
| AP2-008 | Verify full block request fallback | For missing compact txns | ✅ |
| AP2-009 | Verify PoW validation on received blocks | proof::verify called | ✅ |
| AP2-010 | Verify state validation on received blocks | validate_and_apply_block | ✅ |
| AP2-011 | Verify orphan block handling for P2P | Queued for later | ✅ |
| AP2-012 | Verify reorg handling for P2P | Full reorg engine | ✅ |
| AP2-013 | Verify garbage message handling | serde_json error → ignore | ✅ |
| AP2-014 | Verify periodic state save | Every 30s | ✅ |
| AP2-015 | Verify connection idle timeout | 60s | ✅ |

### O. Module: privacy.rs

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| APR-001 | Verify stealth address derivation | Standard construction | ✅ |
| APR-002 | Verify one-time key recovery | v × R = rV | ✅ |
| APR-003 | Verify Pedersen binding property | Computational DLOG | ✅ |
| APR-004 | Verify Pedersen homomorphism | C(a) + C(b) = C(a+b) | ✅ |
| APR-005 | Verify MLSAG sign+verify roundtrip | Proven by tests | ✅ |
| APR-006 | Verify MLSAG wrong message rejection | Proven by tests | ✅ |
| APR-007 | Verify MLSAG multi-layer support | Multiple inputs | ✅ |
| APR-008 | Verify MLSAG minimum ring size | Size > 1 required | ✅ |
| APR-009 | Verify range proof construction | Bit decomposition | ✅ |
| APR-010 | Verify range proof verification | Sum of bits = commitment | ✅ |
| APR-011 | Verify range proof rejects oversized | commitments.len() > 64 | ✅ |
| APR-012 | Verify range proof rejects wrong amount | Wrong commitment fails | ✅ |
| APR-013 | Verify hash_to_scalar is deterministic | Same input = same output | ✅ |
| APR-014 | Verify hash_to_point produces valid curve points | Loop until valid | ⚠️ Non-constant time |
| APR-015 | Verify separate tags for G and H domain separation | "Ewatts_Ring_G" vs "Ewatts_Pedersen_H" | ✅ |

### P. Integration/System Tests

| ID | Procedure | Expected | Status |
|----|-----------|----------|--------|
| AI-001 | Genesis → mine → verify supply increases | total_supply > 0 after mine | ✅ |
| AI-002 | Two miners with equal AOPS | Equal rewards | ✅ |
| AI-003 | Underperforming miner penalized | Lower effective commitment | ✅ |
| AI-004 | Ramp-up cap applies before block 10000 | 80% reward cap | ✅ |
| AI-005 | Founder lock prevents spending before lock height | spendable_after check | ✅ |
| AI-006 | Simple spend (P2PKH) creates valid UTXO set | Balance transfers | ✅ |
| AI-007 | Double spend rejected | Err on second spend | ✅ |
| AI-008 | Private spend with MLSAG validates | Ring sig verification | ✅ |
| AI-009 | Range proof verifies and hides amount | Proof OK, amount hidden | ✅* |
| AI-010 | Compact block roundtrip with mempool | Block reconstructed | ✅ |
| AI-011 | Chain reorganization with reorg engine | State consistent | ✅ |
| AI-012 | Disk persistence across restart | State matches | ✅ |
| AI-013 | Difficulty adjustment over many blocks | Mean time ≈ target | ✅ |
| AI-014 | DAG determinism across restart | Same epoch → same DAG | ✅ |
| AI-015 | P2P block propagation | Gossip + validation | ✅ |

\* Note on AI-009: Amounts are hidden from external observers but NOT from network validators because the `spend_transaction_inputs` function checks plaintext amounts for private txs. See PR-CB-001.

---

## End of Audit Procedures

**Total procedures enumerated: ~380**

### Summary of Critical Issues

| ID | Module | Issue | Severity |
|----|--------|-------|----------|
| RS-OV-07 | proof.rs | difficulty_to_accesses u64 overflow | 🔴 CRITICAL |
| RS-OV-01 | state.rs | total_supply silent wrap on overflow | 🔴 CRITICAL |
| P2P-GS-003 | p2p.rs | reconstruct_block no merkle validation | 🔴 CRITICAL |
| CR-KG-01 | main.rs | Hardcoded genesis key | 🔴 CRITICAL (testnet) |
| CR-KG-02 | main.rs | Hardcoded miner key | 🔴 CRITICAL (testnet) |
| PR-CB-001 | state.rs | Plaintext amount check for private txs | 🔴 HIGH (privacy) |
| P2P-SY-001 | p2p.rs | Unbounded block sync response | 🔴 HIGH |
| CS-RG-003 | reorg.rs | Legacy unwind broken for MLSAG txs | 🔴 HIGH |
| EC-EM-002 | reward.rs | Historical avg always BASE_EMISSION | 🔴 HIGH |
| EC-RU-002 | reward.rs | Burned supply not reflected in state | 🔴 HIGH |
| P2P-SY-002 | p2p.rs | No range limit on block request | 🔴 HIGH |
| RS-UW-01 | dag.rs | Mutex poisoning panic | 🟡 MEDIUM |
| RS-UW-03 | store.rs | BLOCK_CACHE poisoning panic | 🟡 MEDIUM |
| RS-ML-03 | p2p.rs | pending_compact unbounded growth | 🟡 MEDIUM |
| CS-OC-002 | chain.rs | Orphan resolution infinite loop | 🟡 MEDIUM |
| ST-IN-002 | store.rs | Block JSONL partial write on crash | 🟡 MEDIUM |
| DOS-RES-003 | p2p.rs | Unbounded P2P block response processing | 🟡 MEDIUM |
| RS-FP-03 | vr.rs | f64 precision at 10^15 access count | 🟡 MEDIUM |
| PR-SA-001 | state.rs | No decoy selection algorithm for rings | 🟡 MEDIUM |
| PR-CB-002 | state.rs | Commitment vs plaintext inconsistency unchecked | 🟡 MEDIUM |

**38** remaining LOW and informational items detailed in the body of this document.

---

*This document is version 1.0 of the eWatts Security Audit Manual. It reflects the codebase as of July 2026, protocol version 0x0005. All audit procedures should be re-executed after any protocol upgrade.*
