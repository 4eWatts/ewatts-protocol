# eWatts Protocol — Official Test Vectors

**Version:** 0x0005 (V4)  
**Generated:** 2026-07-27  
**Handbook companion:** eWatts_Security_Engineering_Handbook_v4.md (Section 8)

## Files

| File | Description | Size |
|------|-------------|------|
| `test_vectors.json` | All 8 test vectors in JSON format | ~10 KB |
| (source) `ewatts-protocol-repo/tests/test_vectors.rs` | Rust generator (self-validating) | ~13 KB |

## Vector Index

| ID | Title | Code Ref | Handbook § |
|----|-------|----------|-----------|
| TV-1 | DAG Determinism — Element 0, Epoch 0 | dag.rs:generate() | 8.1 |
| TV-2 | Binary Merkle Root — Two Leaves | proof.rs:merkle_root_from_leaves | 8.2 |
| TV-3 | Commitment Efficiency (3 cases) | commitment.rs:compute_efficiency | 8.3 |
| TV-4 | Emission Rate with Clamping (4 cases) | reward.rs:compute_emission_rate | 8.4 |
| TV-5 | Pedersen Commitment & Homomorphic Add | privacy.rs:PedersenCommitment | 8.5 |
| TV-6 | Block Header Hash — Genesis (hex) | block.rs:BlockHeader::hash() | 8.6 |
| TV-7 | Difficulty Adjustment (4 cases) | difficulty.rs:adjust_difficulty | 4.3 |
| TV-8 | Reference Value (3 cases) | vr.rs:compute_vr | 6.5 |

## Verification

Each vector includes a `verification` field naming the unit test(s) that confirm correctness.
To regenerate:
```bash
cd ewatts-protocol-repo
cargo test --test test_vectors -- --nocapture
```
Parse the `JSON:` prefix line for the full output.
