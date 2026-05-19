# Security

## Audit Invariants

Certain function pairs in the codebase must remain consistent. A change in one function requires verifying the other.

### Critical Pairs

| Pair | Invariant |
|------|-----------|
| `RangeProof::prove_with_blinding` ↔ `RangeProof::verify` | Bounds checks must be identical in both: `commitments.len() > 64`, `commitments.len() == proofs.len()`, `commitments.len() == bits` |
| `MLSAGSignature::sign` ↔ `MLSAGSignature::verify` | Length validations in verify must cover everything sign assumes about ring structure |
| `Commitment::new_with_blinding` ↔ `Commitment::verify` | Pedersen formula must be identical modulo sign (additive vs subtractive check) |

### Code Review Checklist

Before submitting or merging any change to consensus-critical code, verify:

1. **Change to a `prove` / `sign` / `new` function requires a corresponding change or test in the matching `verify` function.** The most common bug pattern is fixing bounds in one but forgetting the other.

2. **New validation in a `verify` function requires a test that submits a violating input.** Without a test, the validation is dead code — it may panic or be removed in a refactor without detection.

3. **Modification to consensus-critical code requires a property test demonstrating the invariant survives the change.** Standard property tests (wrong ring size, tampered values, oversized proofs) catch regressions before they reach mainnet.

## Known Limitations

### Side-Channel Timing (MLSAG Signature)

The signing loop in `MLSAGSignature::sign()` processes ring positions in order `(real_index+1, real_index+2, ..., real_index-1)`. This is NOT constant-time with respect to `real_index`. An attacker with timing or cache observation of the signer's machine may be able to recover `real_index`, breaking anonymity.

Side-channel hardening is planned for post-testnet phase.

## Roadmap

- [ ] Fuzzing (`cargo-fuzz` / `proptest`) for verify functions — automated detection of bounds violations
- [ ] External audit (Trail of Bits / NCC Group) — when funding is available
- [ ] Reproducible builds
- [ ] 6-12 months of open testnet without incident before mainnet
